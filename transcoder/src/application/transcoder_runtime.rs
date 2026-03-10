use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use tracing::Instrument;

use crate::domain::video_policy::VideoPolicy;
use crate::error::{AppError, AppResult};
use crate::infrastructure::config::TranscoderConfig;
use crate::infrastructure::message_queue::TranscoderJobMessage;
use crate::infrastructure::object_storage::ObjectStorage;
use crate::infrastructure::postgres::{
    ClaimedTranscodingJob, QueuedTranscodingJobRecord, ReadyManifestVariantRecord,
    RenditionCompletionUpdate, TranscoderRepository, TranscodingFailureDisposition,
    VideoProgressUpdate,
};
use crate::media::{render_master_manifest, MasterManifestVariant, MediaProcessor, TranscodedRendition};

#[derive(Debug)]
pub enum TranscoderProcessOutcome {
    Processed {
        transcoding_job_id: uuid::Uuid,
        video_id: uuid::Uuid,
        rendition: String,
        segment_index: i32,
        follow_up_jobs: Vec<TranscoderJobMessage>,
    },
    RetryQueued {
        transcoding_job_id: uuid::Uuid,
        video_id: uuid::Uuid,
        rendition: String,
        segment_index: i32,
        next_attempt: i32,
        retry_job_message: TranscoderJobMessage,
    },
    FailedTerminal {
        transcoding_job_id: uuid::Uuid,
        video_id: uuid::Uuid,
        rendition: String,
        segment_index: i32,
    },
    Ignored {
        transcoding_job_id: uuid::Uuid,
    },
    WrongRendition {
        transcoding_job_id: uuid::Uuid,
        queue_rendition: String,
        worker_rendition: String,
        reroute_job_message: TranscoderJobMessage,
    },
}

#[derive(Clone)]
pub struct TranscoderRuntime {
    config: Arc<TranscoderConfig>,
    policy: Arc<VideoPolicy>,
    repository: TranscoderRepository,
    object_storage: ObjectStorage,
    media_processor: MediaProcessor,
}

impl TranscoderRuntime {
    pub fn new(
        config: TranscoderConfig,
        policy: Arc<VideoPolicy>,
        repository: TranscoderRepository,
        object_storage: ObjectStorage,
        media_processor: MediaProcessor,
    ) -> Self {
        Self {
            config: Arc::new(config),
            policy,
            repository,
            object_storage,
            media_processor,
        }
    }

    pub async fn process_job(
        &self,
        queue_message: &TranscoderJobMessage,
    ) -> AppResult<TranscoderProcessOutcome> {
        if queue_message.rendition != self.config.rendition_name {
            return Ok(TranscoderProcessOutcome::WrongRendition {
                transcoding_job_id: queue_message.transcoding_job_id,
                queue_rendition: queue_message.rendition.clone(),
                worker_rendition: self.config.rendition_name.clone(),
                reroute_job_message: queue_message.clone(),
            });
        }

        let is_baseline_rendition = self
            .policy
            .is_baseline_rendition(&self.config.rendition_name);
        let Some(job) = self
            .repository
            .claim_transcoding_job(
                queue_message.transcoding_job_id,
                &self.config.transcoder_id,
                &self.config.rendition_name,
                is_baseline_rendition,
            )
            .await?
        else {
            if is_baseline_rendition {
                let follow_up_jobs = self
                    .repository
                    .list_queued_additional_transcoding_jobs(queue_message.video_id)
                    .await?;
                if !follow_up_jobs.is_empty() {
                    return Ok(TranscoderProcessOutcome::Processed {
                        transcoding_job_id: queue_message.transcoding_job_id,
                        video_id: queue_message.video_id,
                        rendition: queue_message.rendition.clone(),
                        segment_index: queue_message.segment_index,
                        follow_up_jobs: follow_up_jobs
                            .into_iter()
                            .map(map_queued_job_to_message)
                            .collect(),
                    });
                }
            }

            return Ok(TranscoderProcessOutcome::Ignored {
                transcoding_job_id: queue_message.transcoding_job_id,
            });
        };

        let processing_directory = self.processing_directory(&job);

        if let Err(error) = self
            .prepare_processing_directory(&processing_directory)
            .await
        {
            if !self.repository.video_exists(job.video_id).await? {
                self.cleanup_processing_directory(&processing_directory);
                return Ok(TranscoderProcessOutcome::Ignored {
                    transcoding_job_id: job.transcoding_job_id,
                });
            }

            let failure = self
                .repository
                .mark_transcoding_failed(
                    job.transcoding_job_id,
                    job.processing_job_id,
                    job.video_id,
                    job.segment_index,
                    &job.rendition,
                    &job.source_segment_s3_key,
                    job.source_segment_duration_seconds,
                    job.attempt,
                    &error.to_string(),
                    self.config.max_transcoding_attempts,
                    self.policy.is_baseline_rendition(&job.rendition),
                    &job.correlation_id,
                )
                .await?;
            self.cleanup_processing_directory(&processing_directory);
            return Ok(map_transcoding_failure(&job, failure));
        }

        let job_span = tracing::info_span!(
            "transcoder_job",
            correlation_id = %job.correlation_id,
            transcoding_job_id = %job.transcoding_job_id,
            processing_job_id = %job.processing_job_id,
            video_id = %job.video_id,
            rendition = %job.rendition,
            segment_index = job.segment_index,
            attempt = job.attempt
        );
        let outcome = self
            .process_claimed_job(&job, &processing_directory)
            .instrument(job_span)
            .await;
        self.cleanup_processing_directory(&processing_directory);

        match outcome {
            Ok(follow_up_jobs) => Ok(TranscoderProcessOutcome::Processed {
                transcoding_job_id: job.transcoding_job_id,
                video_id: job.video_id,
                rendition: job.rendition.clone(),
                segment_index: job.segment_index,
                follow_up_jobs,
            }),
            Err(error) => {
                if !self.repository.video_exists(job.video_id).await? {
                    tracing::info!(
                        video_id = %job.video_id,
                        transcoding_job_id = %job.transcoding_job_id,
                        rendition = %job.rendition,
                        segment_index = job.segment_index,
                        "video was deleted while the segment job was running; ignoring failure"
                    );
                    return Ok(TranscoderProcessOutcome::Ignored {
                        transcoding_job_id: job.transcoding_job_id,
                    });
                }

                let failure = self
                    .repository
                    .mark_transcoding_failed(
                        job.transcoding_job_id,
                        job.processing_job_id,
                        job.video_id,
                        job.segment_index,
                        &job.rendition,
                        &job.source_segment_s3_key,
                        job.source_segment_duration_seconds,
                        job.attempt,
                        &error.to_string(),
                        self.config.max_transcoding_attempts,
                        self.policy.is_baseline_rendition(&job.rendition),
                        &job.correlation_id,
                    )
                    .await?;
                tracing::warn!(
                    transcoding_job_id = %job.transcoding_job_id,
                    video_id = %job.video_id,
                    rendition = %job.rendition,
                    segment_index = job.segment_index,
                    attempt = job.attempt,
                    error = %error,
                    "transcoder job failed"
                );
                Ok(map_transcoding_failure(&job, failure))
            }
        }
    }

    async fn process_claimed_job(
        &self,
        job: &ClaimedTranscodingJob,
        processing_directory: &PathBuf,
    ) -> AppResult<Vec<TranscoderJobMessage>> {
        tracing::info!(
            source_segment_s3_key = %job.source_segment_s3_key,
            segment_index = job.segment_index,
            "claimed transcoding rendition job"
        );

        let source_width = job.source_width.ok_or_else(|| {
            AppError::conflict("source width is missing; chunker must persist it first")
        })?;
        let source_height = job.source_height.ok_or_else(|| {
            AppError::conflict("source height is missing; chunker must persist it first")
        })?;
        let rendition_profile = self
            .policy
            .find_rendition_profile(&job.rendition)
            .ok_or_else(|| {
                AppError::conflict("requested rendition is not defined in the video policy")
            })?;
        let intermediate_prefix = intermediate_prefix_from_playlist_key(&job.source_segment_s3_key)?;
        let source_directory = processing_directory.join("source");
        let source_playlist_path = source_directory.join(
            PathBuf::from(&job.source_segment_s3_key)
                .file_name()
                .ok_or_else(|| AppError::internal("intermediate playlist key is missing a file name"))?,
        );

        self.object_storage
            .download_upload_prefix(&intermediate_prefix, &source_directory)
            .await?;
        tracing::info!(
            path = %source_playlist_path.display(),
            source_width,
            source_height,
            rendition = %job.rendition,
            "downloaded intermediate rendition playlist for transcoding"
        );

        let transcoded_rendition = self
            .media_processor
            .transcode_playlist_to_rendition(
                &source_playlist_path,
                &processing_directory.join("hls"),
                &rendition_profile,
                source_width as u32,
                source_height as u32,
            )
            .await?;
        tracing::info!(
            rendition = %job.rendition,
            output_width = transcoded_rendition.output_width,
            output_height = transcoded_rendition.output_height,
            segment_count = transcoded_rendition.segment_count,
            "transcoded rendition from intermediate playlist"
        );

        if !self.repository.video_exists(job.video_id).await? {
            tracing::info!(
                video_id = %job.video_id,
                transcoding_job_id = %job.transcoding_job_id,
                rendition = %job.rendition,
                "video was deleted while the rendition was transcoding; skipping artifact upload"
            );
            return Ok(Vec::new());
        }

        let playlist_key = format!(
            "videos/{}/hls/{}/{}",
            job.video_id, job.rendition, rendition_profile.variant_playlist_file_name
        );
        self.repository
            .mark_rendition_processing(
                job.video_id,
                &job.rendition,
                &transcoded_rendition.codec,
                &transcoded_rendition.container,
                &playlist_key,
                transcoded_rendition.output_width as i32,
                transcoded_rendition.output_height as i32,
                transcoded_rendition.target_video_bitrate_kbps as i32,
                transcoded_rendition.target_audio_bitrate_kbps as i32,
            )
            .await?;

        self.object_storage
            .upload_processing_directory(
                &format!("videos/{}/hls/{}", job.video_id, job.rendition),
                &processing_directory.join("hls").join(&job.rendition),
            )
            .await?;
        tracing::info!(rendition = %job.rendition, "uploaded transcoded rendition artifacts to processed storage");

        let mut completion_session = self
            .repository
            .begin_video_completion_session(job.video_id)
            .await?;
        let completion_result = async {
            self.repository
                .mark_transcoding_segment_succeeded_in_session(
                    &mut completion_session,
                    job.transcoding_job_id,
                    &playlist_key,
                )
                .await?;

            let existing_ready_variants = self
                .repository
                .list_ready_manifest_variants_in_session(&mut completion_session, job.video_id)
                .await?;
            let manifest_variants = self.build_master_manifest_variants(
                job.video_id,
                &job.rendition,
                &transcoded_rendition,
                existing_ready_variants,
                &playlist_key,
            );
            let master_manifest_key = self
                .write_master_manifest(job.video_id, processing_directory, &manifest_variants)
                .await?;
            self.object_storage
                .upload_processing_file(
                    &master_manifest_key,
                    &processing_directory.join("hls").join("master.m3u8"),
                )
                .await?;
            tracing::info!(master_manifest_key = %master_manifest_key, "uploaded refreshed master manifest");

            let video_progress = self.determine_video_progress_update(
                job,
                source_width as u32,
                source_height as u32,
                &manifest_variants,
                &master_manifest_key,
            );
            let should_cleanup_source_segments =
                matches!(video_progress, VideoProgressUpdate::Ready { .. });
            let completion = RenditionCompletionUpdate {
                processing_job_id: job.processing_job_id,
                video_id: job.video_id,
                rendition_name: job.rendition.clone(),
                codec: transcoded_rendition.codec.clone(),
                container: transcoded_rendition.container.clone(),
                playlist_key: playlist_key.clone(),
                output_width: transcoded_rendition.output_width as i32,
                output_height: transcoded_rendition.output_height as i32,
                target_video_bitrate_kbps: transcoded_rendition.target_video_bitrate_kbps as i32,
                target_audio_bitrate_kbps: transcoded_rendition.target_audio_bitrate_kbps as i32,
                segment_count: transcoded_rendition.segment_count,
            };

            self.repository
                .mark_rendition_ready_in_session(
                    &mut completion_session,
                    completion,
                    video_progress,
                )
                .await?;

            if self.policy.is_baseline_rendition(&job.rendition) {
                let queued_jobs = self
                    .repository
                    .list_queued_additional_transcoding_jobs(job.video_id)
                    .await?;
                return Ok((
                    queued_jobs
                        .into_iter()
                        .map(map_queued_job_to_message)
                        .collect(),
                    should_cleanup_source_segments,
                ));
            }

            Ok((Vec::new(), should_cleanup_source_segments))
        }
        .await;

        match completion_result {
            Ok((follow_up_jobs, should_cleanup_source_segments)) => {
                self.repository
                    .commit_video_completion_session(completion_session)
                    .await?;

                if should_cleanup_source_segments {
                    let prefix = format!("videos/{}/source/intermediate/", job.video_id);
                    match self.object_storage.delete_upload_prefix(&prefix).await {
                        Ok(deleted_count) => {
                            tracing::info!(
                                video_id = %job.video_id,
                                deleted_object_count = deleted_count,
                                prefix = %prefix,
                                "deleted intermediate source segments after final transcoding completion"
                            );
                        }
                        Err(error) => {
                            tracing::error!(
                                video_id = %job.video_id,
                                prefix = %prefix,
                                error = %error,
                                "failed to delete intermediate source segments after final transcoding completion"
                            );
                        }
                    }
                }
                Ok(follow_up_jobs)
            }
            Err(error) => {
                self.repository
                    .rollback_video_completion_session(completion_session)
                    .await?;
                Err(error)
            }
        }
    }

    fn build_master_manifest_variants(
        &self,
        video_id: uuid::Uuid,
        rendition: &str,
        transcoded_rendition: &TranscodedRendition,
        existing_ready_variants: Vec<ReadyManifestVariantRecord>,
        playlist_key: &str,
    ) -> Vec<MasterManifestVariant> {
        let mut variants = existing_ready_variants
            .into_iter()
            .filter_map(|variant| {
                Some(MasterManifestVariant {
                    rendition: variant.rendition,
                    playlist_path: relative_playlist_path(video_id, &variant.playlist_key)?,
                    width: variant.output_width? as u32,
                    height: variant.output_height? as u32,
                    video_bitrate_kbps: variant.target_video_bitrate_kbps? as u32,
                    audio_bitrate_kbps: variant.target_audio_bitrate_kbps? as u32,
                })
            })
            .collect::<Vec<_>>();

        variants.retain(|variant| variant.rendition != rendition);
        variants.push(MasterManifestVariant {
            rendition: rendition.to_owned(),
            playlist_path: relative_playlist_path(video_id, playlist_key)
                .unwrap_or_else(|| format!("{rendition}/{rendition}.m3u8")),
            width: transcoded_rendition.output_width,
            height: transcoded_rendition.output_height,
            video_bitrate_kbps: transcoded_rendition.target_video_bitrate_kbps,
            audio_bitrate_kbps: transcoded_rendition.target_audio_bitrate_kbps,
        });
        variants.sort_by_key(|variant| self.policy.rendition_ladder_position(&variant.rendition));
        variants
    }

    async fn write_master_manifest(
        &self,
        video_id: uuid::Uuid,
        processing_directory: &PathBuf,
        manifest_variants: &[MasterManifestVariant],
    ) -> AppResult<String> {
        let manifest_path = processing_directory.join("hls").join("master.m3u8");
        let manifest_body = render_master_manifest(manifest_variants);
        tokio::fs::write(&manifest_path, manifest_body).await?;

        Ok(format!("videos/{}/hls/master.m3u8", video_id))
    }

    fn determine_video_progress_update(
        &self,
        job: &ClaimedTranscodingJob,
        source_width: u32,
        source_height: u32,
        ready_manifest_variants: &[MasterManifestVariant],
        master_manifest_key: &str,
    ) -> VideoProgressUpdate {
        let planned_renditions = self
            .policy
            .source_eligible_rendition_names(source_width, source_height);
        let completed_renditions = ready_manifest_variants
            .iter()
            .map(|variant| variant.rendition.as_str())
            .collect::<HashSet<_>>();
        let all_planned_renditions_ready = planned_renditions
            .iter()
            .all(|rendition| completed_renditions.contains(rendition.as_str()));

        if self.policy.is_baseline_rendition(&job.rendition) {
            if all_planned_renditions_ready {
                return VideoProgressUpdate::Ready {
                    manifest_s3_key: master_manifest_key.to_owned(),
                };
            }

            return VideoProgressUpdate::BaselineReady {
                manifest_s3_key: master_manifest_key.to_owned(),
            };
        }

        if all_planned_renditions_ready {
            return VideoProgressUpdate::Ready {
                manifest_s3_key: master_manifest_key.to_owned(),
            };
        }

        VideoProgressUpdate::RenditionReadyOnly
    }

    async fn prepare_processing_directory(&self, processing_directory: &PathBuf) -> AppResult<()> {
        tokio::fs::create_dir_all(processing_directory.join("source")).await?;
        tokio::fs::create_dir_all(processing_directory.join("hls")).await?;
        Ok(())
    }

    fn processing_directory(&self, job: &ClaimedTranscodingJob) -> PathBuf {
        self.config
            .transcoder_temp_dir
            .join(job.rendition.to_ascii_lowercase())
            .join(job.video_id.to_string())
            .join(job.transcoding_job_id.to_string())
    }

    fn cleanup_processing_directory(&self, processing_directory: &PathBuf) {
        let path = processing_directory.clone();

        tokio::spawn(async move {
            if let Err(error) = tokio::fs::remove_dir_all(&path).await {
                tracing::warn!(path = %path.display(), error = %error, "failed to remove transcoder temp directory");
            }
        });
    }
}

fn relative_playlist_path(video_id: uuid::Uuid, playlist_key: &str) -> Option<String> {
    let prefix = format!("videos/{video_id}/hls/");
    playlist_key.strip_prefix(&prefix).map(str::to_owned)
}

fn intermediate_prefix_from_playlist_key(playlist_key: &str) -> AppResult<String> {
    let playlist_path = PathBuf::from(playlist_key);
    let parent = playlist_path
        .parent()
        .ok_or_else(|| AppError::internal("intermediate playlist key is missing a parent prefix"))?;

    Ok(format!("{}/", parent.to_string_lossy()))
}

fn map_transcoding_failure(
    job: &ClaimedTranscodingJob,
    disposition: TranscodingFailureDisposition,
) -> TranscoderProcessOutcome {
    match disposition {
        TranscodingFailureDisposition::RetryQueued {
            retry_transcoding_job_id,
            next_attempt,
        } => TranscoderProcessOutcome::RetryQueued {
            transcoding_job_id: job.transcoding_job_id,
            video_id: job.video_id,
            rendition: job.rendition.clone(),
            segment_index: job.segment_index,
            next_attempt,
            retry_job_message: TranscoderJobMessage {
                transcoding_job_id: retry_transcoding_job_id,
                processing_job_id: job.processing_job_id,
                video_id: job.video_id,
                rendition: job.rendition.clone(),
                segment_index: job.segment_index,
                correlation_id: job.correlation_id.clone(),
                attempt: next_attempt,
            },
        },
        TranscodingFailureDisposition::Terminal => TranscoderProcessOutcome::FailedTerminal {
            transcoding_job_id: job.transcoding_job_id,
            video_id: job.video_id,
            rendition: job.rendition.clone(),
            segment_index: job.segment_index,
        },
    }
}

fn map_queued_job_to_message(job: QueuedTranscodingJobRecord) -> TranscoderJobMessage {
    TranscoderJobMessage {
        transcoding_job_id: job.transcoding_job_id,
        processing_job_id: job.processing_job_id,
        video_id: job.video_id,
        rendition: job.rendition,
        segment_index: job.segment_index,
        correlation_id: job.correlation_id,
        attempt: job.attempt,
    }
}
