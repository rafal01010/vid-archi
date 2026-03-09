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
    TranscoderRepository, TranscodingCompletionUpdate, TranscodingFailureDisposition,
    VideoProgressUpdate,
};
use crate::media::{
    render_master_manifest, MasterManifestVariant, MediaProcessor, PackagedRendition,
};

#[derive(Debug)]
pub enum TranscoderProcessOutcome {
    Processed {
        transcoding_job_id: uuid::Uuid,
        video_id: uuid::Uuid,
        rendition: String,
        follow_up_jobs: Vec<TranscoderJobMessage>,
    },
    RetryQueued {
        transcoding_job_id: uuid::Uuid,
        video_id: uuid::Uuid,
        rendition: String,
        next_attempt: i32,
        retry_job_message: TranscoderJobMessage,
    },
    FailedTerminal {
        transcoding_job_id: uuid::Uuid,
        video_id: uuid::Uuid,
        rendition: String,
    },
    Ignored {
        transcoding_job_id: uuid::Uuid,
    },
    WrongRendition {
        transcoding_job_id: uuid::Uuid,
        queue_rendition: String,
        worker_rendition: String,
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
            return Ok(TranscoderProcessOutcome::Ignored {
                transcoding_job_id: queue_message.transcoding_job_id,
            });
        };

        let processing_directory = self.processing_directory(&job);

        if let Err(error) = self
            .prepare_processing_directory(&processing_directory)
            .await
        {
            let failure = self
                .repository
                .mark_transcoding_failed(
                    job.transcoding_job_id,
                    job.processing_job_id,
                    job.video_id,
                    job.attempt,
                    &job.rendition,
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
                follow_up_jobs,
            }),
            Err(error) => {
                let failure = self
                    .repository
                    .mark_transcoding_failed(
                        job.transcoding_job_id,
                        job.processing_job_id,
                        job.video_id,
                        job.attempt,
                        &job.rendition,
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
        tracing::info!(source_s3_key = %job.source_s3_key, "claimed transcoding job");

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
        let source_path = processing_directory.join("source").join("original");

        self.object_storage
            .download_source_object(&job.source_s3_key, &source_path)
            .await?;
        tracing::info!(
            path = %source_path.display(),
            source_width,
            source_height,
            "downloaded source object for transcoding"
        );

        let packaged = self
            .media_processor
            .package_rendition_hls(
                &source_path,
                &processing_directory.join("hls"),
                &rendition_profile,
                source_width as u32,
                source_height as u32,
            )
            .await?;
        tracing::info!(
            rendition = %job.rendition,
            output_width = packaged.output_width,
            output_height = packaged.output_height,
            segment_count = packaged.segment_count,
            "packaged HLS rendition"
        );

        self.repository
            .mark_rendition_processing(
                job.video_id,
                &job.rendition,
                &packaged.codec,
                &packaged.container,
                &format!(
                    "videos/{}/hls/{}/{}",
                    job.video_id, job.rendition, packaged.playlist_file_name
                ),
                packaged.output_width as i32,
                packaged.output_height as i32,
                packaged.target_video_bitrate_kbps as i32,
                packaged.target_audio_bitrate_kbps as i32,
            )
            .await?;

        self.object_storage
            .upload_processing_directory(
                &format!("videos/{}/hls/{}", job.video_id, job.rendition),
                &processing_directory.join("hls").join(&job.rendition),
            )
            .await?;
        tracing::info!("uploaded rendition artifacts to processed storage");

        let mut completion_session = self
            .repository
            .begin_video_completion_session(job.video_id)
            .await?;
        let completion_result = async {
            let existing_ready_variants = self
                .repository
                .list_ready_manifest_variants_in_session(&mut completion_session, job.video_id)
                .await?;
            let manifest_variants = self.build_master_manifest_variants(
                job.video_id,
                &packaged,
                existing_ready_variants,
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
            let completion = self.build_completion_update(job, &packaged);

            self.repository
                .mark_transcoding_succeeded_in_session(
                    &mut completion_session,
                    completion,
                    video_progress,
                )
                .await
        }
        .await;

        match completion_result {
            Ok(()) => {
                self.repository
                    .commit_video_completion_session(completion_session)
                    .await?;
            }
            Err(error) => {
                self.repository
                    .rollback_video_completion_session(completion_session)
                    .await?;
                return Err(error);
            }
        }

        if self.policy.is_baseline_rendition(&job.rendition) {
            let queued_jobs = self
                .repository
                .list_queued_additional_transcoding_jobs(job.video_id)
                .await?;
            return Ok(queued_jobs
                .into_iter()
                .map(map_queued_job_to_message)
                .collect());
        }

        Ok(Vec::new())
    }

    fn build_completion_update(
        &self,
        job: &ClaimedTranscodingJob,
        packaged: &PackagedRendition,
    ) -> TranscodingCompletionUpdate {
        TranscodingCompletionUpdate {
            transcoding_job_id: job.transcoding_job_id,
            processing_job_id: job.processing_job_id,
            video_id: job.video_id,
            rendition_name: job.rendition.clone(),
            codec: packaged.codec.clone(),
            container: packaged.container.clone(),
            playlist_key: format!(
                "videos/{}/hls/{}/{}",
                job.video_id, job.rendition, packaged.playlist_file_name
            ),
            output_width: packaged.output_width as i32,
            output_height: packaged.output_height as i32,
            target_video_bitrate_kbps: packaged.target_video_bitrate_kbps as i32,
            target_audio_bitrate_kbps: packaged.target_audio_bitrate_kbps as i32,
            segment_count: packaged.segment_count as i32,
        }
    }

    fn build_master_manifest_variants(
        &self,
        video_id: uuid::Uuid,
        packaged: &PackagedRendition,
        existing_ready_variants: Vec<ReadyManifestVariantRecord>,
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

        variants.retain(|variant| variant.rendition != packaged.rendition);
        variants.push(MasterManifestVariant {
            rendition: packaged.rendition.clone(),
            playlist_path: format!("{}/{}", packaged.rendition, packaged.playlist_file_name),
            width: packaged.output_width,
            height: packaged.output_height,
            video_bitrate_kbps: packaged.target_video_bitrate_kbps,
            audio_bitrate_kbps: packaged.target_audio_bitrate_kbps,
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
            .collect::<std::collections::HashSet<_>>();
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

fn map_transcoding_failure(
    job: &ClaimedTranscodingJob,
    disposition: TranscodingFailureDisposition,
) -> TranscoderProcessOutcome {
    match disposition {
        TranscodingFailureDisposition::RetryQueued {
            retry_transcoding_job_id,
            next_attempt,
        } => {
            TranscoderProcessOutcome::RetryQueued {
                transcoding_job_id: job.transcoding_job_id,
                video_id: job.video_id,
                rendition: job.rendition.clone(),
                next_attempt,
                retry_job_message: TranscoderJobMessage {
                    transcoding_job_id: retry_transcoding_job_id,
                    processing_job_id: job.processing_job_id,
                    video_id: job.video_id,
                    rendition: job.rendition.clone(),
                    correlation_id: job.correlation_id.clone(),
                    attempt: next_attempt,
                },
            }
        }
        TranscodingFailureDisposition::Terminal => TranscoderProcessOutcome::FailedTerminal {
            transcoding_job_id: job.transcoding_job_id,
            video_id: job.video_id,
            rendition: job.rendition.clone(),
        },
    }
}

fn map_queued_job_to_message(job: QueuedTranscodingJobRecord) -> TranscoderJobMessage {
    TranscoderJobMessage {
        transcoding_job_id: job.transcoding_job_id,
        processing_job_id: job.processing_job_id,
        video_id: job.video_id,
        rendition: job.rendition,
        correlation_id: job.correlation_id,
        attempt: job.attempt,
    }
}
