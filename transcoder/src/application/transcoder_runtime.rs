use std::path::PathBuf;
use std::sync::Arc;

use crate::domain::video_policy::VideoPolicy;
use crate::error::{AppError, AppResult};
use crate::infrastructure::config::TranscoderConfig;
use crate::infrastructure::object_storage::ObjectStorage;
use crate::infrastructure::postgres::{
    ClaimedTranscodingJob, TranscoderRepository, TranscodingCompletionUpdate,
};
use crate::media::{MediaProcessor, PackagedRendition};

#[derive(Debug)]
pub enum TranscoderProcessOutcome {
    Processed {
        transcoding_job_id: uuid::Uuid,
        video_id: uuid::Uuid,
        rendition: String,
    },
    Idle,
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

    pub async fn process_next_job(&self) -> AppResult<TranscoderProcessOutcome> {
        let Some(job) = self
            .repository
            .claim_next_transcoding_job(&self.config.transcoder_id, &self.config.rendition_name)
            .await?
        else {
            return Ok(TranscoderProcessOutcome::Idle);
        };

        let processing_directory = self.processing_directory(&job);

        if let Err(error) = self
            .prepare_processing_directory(&processing_directory)
            .await
        {
            self.repository
                .mark_transcoding_failed(
                    job.transcoding_job_id,
                    job.processing_job_id,
                    job.video_id,
                    &job.rendition,
                    &error.to_string(),
                    self.policy.is_baseline_rendition(&job.rendition),
                )
                .await?;
            self.cleanup_processing_directory(&processing_directory);
            return Err(error);
        }

        let outcome = self.process_claimed_job(&job, &processing_directory).await;
        self.cleanup_processing_directory(&processing_directory);

        match outcome {
            Ok(()) => Ok(TranscoderProcessOutcome::Processed {
                transcoding_job_id: job.transcoding_job_id,
                video_id: job.video_id,
                rendition: job.rendition.clone(),
            }),
            Err(error) => {
                self.repository
                    .mark_transcoding_failed(
                        job.transcoding_job_id,
                        job.processing_job_id,
                        job.video_id,
                        &job.rendition,
                        &error.to_string(),
                        self.policy.is_baseline_rendition(&job.rendition),
                    )
                    .await?;
                Err(error)
            }
        }
    }

    async fn process_claimed_job(
        &self,
        job: &ClaimedTranscodingJob,
        processing_directory: &PathBuf,
    ) -> AppResult<()> {
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

        let packaged = self
            .media_processor
            .package_rendition_hls(
                &source_path,
                &processing_directory.join("hls"),
                &rendition_profile,
                self.policy.is_baseline_rendition(&job.rendition),
                source_width as u32,
                source_height as u32,
            )
            .await?;

        self.object_storage
            .upload_processing_directory(
                &format!("videos/{}/hls", job.video_id),
                &processing_directory.join("hls"),
            )
            .await?;

        let completion = self.build_completion_update(job, &packaged);

        self.repository
            .mark_rendition_processing(
                job.video_id,
                &job.rendition,
                &packaged.codec,
                &packaged.container,
                &completion.playlist_key,
            )
            .await?;

        self.repository
            .mark_transcoding_succeeded(
                completion,
                self.policy.is_baseline_rendition(&job.rendition),
            )
            .await?;

        Ok(())
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
            segment_count: packaged.segment_count as i32,
            manifest_s3_key: packaged
                .master_manifest_file_name
                .as_deref()
                .map(|file_name| format!("videos/{}/hls/{}", job.video_id, file_name)),
        }
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
