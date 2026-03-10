use std::path::PathBuf;
use std::sync::Arc;

use tracing::Instrument;
use uuid::Uuid;

use crate::domain::video_policy::VideoPolicy;
use crate::error::AppResult;
use crate::infrastructure::config::ChunkerConfig;
use crate::infrastructure::message_queue::TranscoderJobMessage;
use crate::infrastructure::object_storage::ObjectStorage;
use crate::infrastructure::postgres::{
    ChunkerFailureDisposition, ChunkerRepository, ClaimedBaselineJob, QueuedTranscodingJobRecord,
    ReadyManifestVariantRecord, ReadyRenditionSeed,
};
use crate::media::{PreparedIntermediateRendition, SourceProbe, SourceSegmenter};

#[derive(Debug)]
pub enum ChunkerProcessOutcome {
    Dispatched {
        job_id: Uuid,
        video_id: Uuid,
        transcoder_messages: Vec<TranscoderJobMessage>,
    },
    RetryQueued {
        job_id: Uuid,
        video_id: Uuid,
        retry_job_id: Uuid,
        next_attempt: i32,
    },
    FailedTerminal {
        job_id: Uuid,
        video_id: Uuid,
    },
    Ignored {
        job_id: Uuid,
    },
}

#[derive(Clone)]
pub struct ChunkerRuntime {
    config: Arc<ChunkerConfig>,
    policy: Arc<VideoPolicy>,
    repository: ChunkerRepository,
    object_storage: ObjectStorage,
    source_probe: SourceProbe,
    source_segmenter: SourceSegmenter,
}

impl ChunkerRuntime {
    pub fn new(
        config: ChunkerConfig,
        policy: Arc<VideoPolicy>,
        repository: ChunkerRepository,
        object_storage: ObjectStorage,
        source_probe: SourceProbe,
        source_segmenter: SourceSegmenter,
    ) -> Self {
        Self {
            config: Arc::new(config),
            policy,
            repository,
            object_storage,
            source_probe,
            source_segmenter,
        }
    }

    pub async fn process_job(&self, processing_job_id: Uuid) -> AppResult<ChunkerProcessOutcome> {
        let Some(job) = self
            .repository
            .claim_baseline_job(processing_job_id, &self.config.chunker_id)
            .await?
        else {
            let queued_jobs = self
                .repository
                .list_queued_transcoding_jobs_for_processing_job(processing_job_id)
                .await?;

            if queued_jobs.is_empty() {
                return Ok(ChunkerProcessOutcome::Ignored {
                    job_id: processing_job_id,
                });
            }

            let video_id = queued_jobs[0].video_id;
            return Ok(ChunkerProcessOutcome::Dispatched {
                job_id: processing_job_id,
                video_id,
                transcoder_messages: queued_jobs.into_iter().map(map_queued_job_to_message).collect(),
            });
        };

        let processing_directory = self.processing_directory(&job);

        if let Err(error) = self
            .prepare_processing_directory(&processing_directory)
            .await
        {
            if !self.repository.video_exists(job.video_id).await? {
                self.cleanup_processing_directory(&processing_directory);
                return Ok(ChunkerProcessOutcome::Ignored { job_id: job.job_id });
            }

            let failure = self
                .repository
                .mark_chunking_failed(
                    job.job_id,
                    job.video_id,
                    job.attempt,
                    &job.correlation_id,
                    &error.to_string(),
                    self.config.max_processing_attempts,
                )
                .await?;
            self.cleanup_processing_directory(&processing_directory);
            return Ok(map_chunker_failure(&job, failure));
        }

        let job_span = tracing::info_span!(
            "chunker_job",
            correlation_id = %job.correlation_id,
            job_id = %job.job_id,
            video_id = %job.video_id,
            attempt = job.attempt
        );
        let outcome = self
            .process_claimed_job(&job, &processing_directory)
            .instrument(job_span)
            .await;
        self.cleanup_processing_directory(&processing_directory);

        match outcome {
            Ok(transcoder_messages) => Ok(ChunkerProcessOutcome::Dispatched {
                job_id: job.job_id,
                video_id: job.video_id,
                transcoder_messages,
            }),
            Err(error) => {
                if !self.repository.video_exists(job.video_id).await? {
                    tracing::info!(
                        job_id = %job.job_id,
                        video_id = %job.video_id,
                        "video was deleted while the chunker job was running; ignoring failure"
                    );
                    return Ok(ChunkerProcessOutcome::Ignored { job_id: job.job_id });
                }

                let failure = self
                    .repository
                    .mark_chunking_failed(
                        job.job_id,
                        job.video_id,
                        job.attempt,
                        &job.correlation_id,
                        &error.to_string(),
                        self.config.max_processing_attempts,
                    )
                    .await?;
                tracing::warn!(job_id = %job.job_id, video_id = %job.video_id, attempt = job.attempt, error = %error, "chunker job failed");
                Ok(map_chunker_failure(&job, failure))
            }
        }
    }

    async fn process_claimed_job(
        &self,
        job: &ClaimedBaselineJob,
        processing_directory: &PathBuf,
    ) -> AppResult<Vec<TranscoderJobMessage>> {
        tracing::info!(source_s3_key = %job.source_s3_key, "claimed post-baseline intermediate processing job");

        let source_path = processing_directory.join("source").join("original");
        self.object_storage
            .download_source_object(&job.source_s3_key, &source_path)
            .await?;
        tracing::info!(path = %source_path.display(), "downloaded source object for chunking");

        if !self.repository.video_exists(job.video_id).await? {
            tracing::info!(
                job_id = %job.job_id,
                video_id = %job.video_id,
                "video was deleted after the source download; skipping chunker work"
            );
            return Ok(Vec::new());
        }

        let source_metadata = self.source_probe.probe(&source_path).await?;
        let baseline_rendition = self.policy.baseline_rendition_name();
        let highest_rendition = self
            .policy
            .highest_eligible_rendition_profile(source_metadata.width, source_metadata.height)
            .ok_or_else(|| crate::error::AppError::internal("no eligible rendition for source"))?;
        let additional_renditions = self
            .policy
            .source_eligible_additional_renditions(source_metadata.width, source_metadata.height);
        let intermediate_rendition = self
            .source_segmenter
            .build_intermediate_rendition(
                &source_path,
                &processing_directory.join("intermediate"),
                source_metadata.width,
                source_metadata.height,
                &highest_rendition,
                self.policy.source_segment_duration_seconds(),
            )
            .await?;
        tracing::info!(
            source_width = source_metadata.width,
            source_height = source_metadata.height,
            baseline_rendition = %baseline_rendition,
            highest_rendition = %highest_rendition.name,
            additional_rendition_count = additional_renditions.len(),
            segment_count = intermediate_rendition.segments.len(),
            "prepared the highest eligible intermediate rendition from the uploaded source"
        );

        if !self.repository.video_exists(job.video_id).await? {
            tracing::info!(
                job_id = %job.job_id,
                video_id = %job.video_id,
                "video was deleted after intermediate rendition preparation; skipping uploads"
            );
            return Ok(Vec::new());
        }

        let intermediate_playlist_s3_key = self
            .upload_intermediate_rendition(job.video_id, &intermediate_rendition)
            .await?;
        let ready_rendition_seed = if intermediate_rendition.rendition_name != baseline_rendition {
            Some(
                self.publish_highest_ready_rendition(job.video_id, &intermediate_rendition)
                    .await?,
            )
        } else {
            None
        };

        if !self.repository.video_exists(job.video_id).await? {
            tracing::info!(
                job_id = %job.job_id,
                video_id = %job.video_id,
                "video was deleted after intermediate rendition upload; skipping transcoder dispatch"
            );
            return Ok(Vec::new());
        }

        let source_duration_seconds = intermediate_rendition
            .segments
            .iter()
            .map(|segment| segment.duration_seconds)
            .sum::<f64>();
        let queued_jobs = self
            .repository
            .dispatch_transcoding_jobs(
                job.job_id,
                job.video_id,
                &job.correlation_id,
                source_metadata.width,
                source_metadata.height,
                &additional_renditions,
                &intermediate_playlist_s3_key,
                source_duration_seconds,
                ready_rendition_seed.as_ref(),
            )
            .await?;
        let ready_variants = self.repository.list_ready_manifest_variants(job.video_id).await?;
        let master_manifest_key = self
            .write_master_manifest(job.video_id, processing_directory, &ready_variants)
            .await?;
        self.object_storage
            .upload_processed_file(
                &master_manifest_key,
                &processing_directory.join("hls").join("master.m3u8"),
                "application/vnd.apple.mpegurl",
            )
            .await?;
        self.repository
            .mark_intermediate_job_succeeded(
                job.job_id,
                job.video_id,
                queued_jobs.is_empty(),
                &master_manifest_key,
            )
            .await?;
        tracing::info!(queued_job_count = queued_jobs.len(), "queued lower-rendition transcoding jobs");

        Ok(queued_jobs
            .into_iter()
            .map(map_queued_job_to_message)
            .collect())
    }

    async fn upload_intermediate_rendition(
        &self,
        video_id: Uuid,
        rendition: &PreparedIntermediateRendition,
    ) -> AppResult<String> {
        let prefix = format!(
            "videos/{video_id}/source/intermediate/{}/",
            rendition.rendition_name
        );
        let playlist_key = format!("{prefix}{}.m3u8", rendition.rendition_name);
        self.object_storage
            .upload_source_playlist(&playlist_key, &rendition.variant_playlist_path)
            .await?;

        for segment in &rendition.segments {
            let object_key = format!(
                "{prefix}segment_{:05}.ts",
                segment.segment_index
            );
            self.object_storage
                .upload_source_segment(&object_key, &segment.path)
                .await?;
        }

        Ok(playlist_key)
    }

    async fn publish_highest_ready_rendition(
        &self,
        video_id: Uuid,
        rendition: &PreparedIntermediateRendition,
    ) -> AppResult<ReadyRenditionSeed> {
        let playlist_key = format!(
            "videos/{video_id}/hls/{}/{}.m3u8",
            rendition.rendition_name, rendition.rendition_name
        );
        self.object_storage
            .upload_processed_file(
                &playlist_key,
                &rendition.variant_playlist_path,
                "application/vnd.apple.mpegurl",
            )
            .await?;

        for segment in &rendition.segments {
            let object_key = format!(
                "videos/{video_id}/hls/{}/segment_{:05}.ts",
                rendition.rendition_name, segment.segment_index
            );
            self.object_storage
                .upload_processed_file(&object_key, &segment.path, "video/mp2t")
                .await?;
        }

        Ok(ReadyRenditionSeed {
            rendition_name: rendition.rendition_name.clone(),
            playlist_key,
            output_width: rendition.width as i32,
            output_height: rendition.height as i32,
            target_video_bitrate_kbps: rendition.video_bitrate_kbps as i32,
            target_audio_bitrate_kbps: rendition.audio_bitrate_kbps as i32,
            segment_count: rendition.segments.len() as i32,
        })
    }

    async fn prepare_processing_directory(&self, processing_directory: &PathBuf) -> AppResult<()> {
        tokio::fs::create_dir_all(processing_directory.join("source")).await?;
        tokio::fs::create_dir_all(processing_directory.join("intermediate")).await?;
        Ok(())
    }

    fn processing_directory(&self, job: &ClaimedBaselineJob) -> PathBuf {
        self.config
            .chunker_temp_dir
            .join(job.video_id.to_string())
            .join(job.job_id.to_string())
    }

    fn cleanup_processing_directory(&self, processing_directory: &PathBuf) {
        let path = processing_directory.clone();

        tokio::spawn(async move {
            if let Err(error) = tokio::fs::remove_dir_all(&path).await {
                tracing::warn!(path = %path.display(), error = %error, "failed to remove chunker temp directory");
            }
        });
    }

    async fn write_master_manifest(
        &self,
        video_id: Uuid,
        processing_directory: &PathBuf,
        ready_variants: &[ReadyManifestVariantRecord],
    ) -> AppResult<String> {
        let variants = self.build_master_manifest_variants(video_id, ready_variants);
        let manifest_body = render_master_manifest(&variants);
        let manifest_path = processing_directory.join("hls").join("master.m3u8");
        tokio::fs::create_dir_all(processing_directory.join("hls")).await?;
        tokio::fs::write(&manifest_path, manifest_body).await?;

        Ok(format!("videos/{video_id}/hls/master.m3u8"))
    }

    fn build_master_manifest_variants(
        &self,
        video_id: Uuid,
        ready_variants: &[ReadyManifestVariantRecord],
    ) -> Vec<MasterManifestVariant> {
        let mut variants = ready_variants
            .iter()
            .filter_map(|variant| {
                Some(MasterManifestVariant {
                    rendition: variant.rendition.clone(),
                    playlist_path: relative_playlist_path(video_id, &variant.playlist_key)?,
                    width: variant.output_width? as u32,
                    height: variant.output_height? as u32,
                    video_bitrate_kbps: variant.target_video_bitrate_kbps? as u32,
                    audio_bitrate_kbps: variant.target_audio_bitrate_kbps? as u32,
                })
            })
            .collect::<Vec<_>>();

        variants.sort_by_key(|variant| self.policy.rendition_ladder_position(&variant.rendition));
        variants
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

fn relative_playlist_path(video_id: Uuid, playlist_key: &str) -> Option<String> {
    let prefix = format!("videos/{video_id}/hls/");
    playlist_key.strip_prefix(&prefix).map(str::to_owned)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MasterManifestVariant {
    rendition: String,
    playlist_path: String,
    width: u32,
    height: u32,
    video_bitrate_kbps: u32,
    audio_bitrate_kbps: u32,
}

fn render_master_manifest(variants: &[MasterManifestVariant]) -> String {
    let mut manifest = String::from("#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-INDEPENDENT-SEGMENTS\n");

    for variant in variants {
        let bandwidth = (variant.video_bitrate_kbps + variant.audio_bitrate_kbps) * 1000;
        manifest.push_str(&format!(
            "#EXT-X-STREAM-INF:BANDWIDTH={bandwidth},AVERAGE-BANDWIDTH={bandwidth},RESOLUTION={}x{},CODECS=\"avc1.42e01e,mp4a.40.2\"\n{}\n",
            variant.width, variant.height, variant.playlist_path
        ));
    }

    manifest
}

fn map_chunker_failure(
    job: &ClaimedBaselineJob,
    disposition: ChunkerFailureDisposition,
) -> ChunkerProcessOutcome {
    match disposition {
        ChunkerFailureDisposition::RetryQueued {
            retry_job_id,
            next_attempt,
        } => ChunkerProcessOutcome::RetryQueued {
            job_id: job.job_id,
            video_id: job.video_id,
            retry_job_id,
            next_attempt,
        },
        ChunkerFailureDisposition::Terminal => ChunkerProcessOutcome::FailedTerminal {
            job_id: job.job_id,
            video_id: job.video_id,
        },
    }
}
