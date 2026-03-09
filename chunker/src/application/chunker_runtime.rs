use std::path::PathBuf;
use std::sync::Arc;

use uuid::Uuid;

use crate::domain::video_policy::VideoPolicy;
use crate::error::AppResult;
use crate::infrastructure::config::ChunkerConfig;
use crate::infrastructure::object_storage::ObjectStorage;
use crate::infrastructure::postgres::{
    ChunkerFailureDisposition, ChunkerRepository, ClaimedBaselineJob,
};
use crate::media::SourceProbe;

#[derive(Debug)]
pub enum ChunkerProcessOutcome {
    Dispatched {
        job_id: Uuid,
        video_id: Uuid,
    },
    RetryQueued {
        job_id: Uuid,
        video_id: Uuid,
        next_attempt: i32,
    },
    FailedTerminal {
        job_id: Uuid,
        video_id: Uuid,
    },
    Idle,
}

#[derive(Clone)]
pub struct ChunkerRuntime {
    config: Arc<ChunkerConfig>,
    policy: Arc<VideoPolicy>,
    repository: ChunkerRepository,
    object_storage: ObjectStorage,
    source_probe: SourceProbe,
}

impl ChunkerRuntime {
    pub fn new(
        config: ChunkerConfig,
        policy: Arc<VideoPolicy>,
        repository: ChunkerRepository,
        object_storage: ObjectStorage,
        source_probe: SourceProbe,
    ) -> Self {
        Self {
            config: Arc::new(config),
            policy,
            repository,
            object_storage,
            source_probe,
        }
    }

    pub async fn process_next_job(&self) -> AppResult<ChunkerProcessOutcome> {
        let Some(job) = self
            .repository
            .claim_next_baseline_job(&self.config.chunker_id)
            .await?
        else {
            return Ok(ChunkerProcessOutcome::Idle);
        };

        let processing_directory = self.processing_directory(&job);

        if let Err(error) = self
            .prepare_processing_directory(&processing_directory)
            .await
        {
            let failure = self
                .repository
                .mark_chunking_failed(
                    job.job_id,
                    job.video_id,
                    job.attempt,
                    &error.to_string(),
                    self.config.max_processing_attempts,
                )
                .await?;
            self.cleanup_processing_directory(&processing_directory);
            return Ok(map_chunker_failure(&job, failure));
        }

        let outcome = self.process_claimed_job(&job, &processing_directory).await;
        self.cleanup_processing_directory(&processing_directory);

        match outcome {
            Ok(()) => Ok(ChunkerProcessOutcome::Dispatched {
                job_id: job.job_id,
                video_id: job.video_id,
            }),
            Err(error) => {
                let failure = self
                    .repository
                    .mark_chunking_failed(
                        job.job_id,
                        job.video_id,
                        job.attempt,
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
    ) -> AppResult<()> {
        let source_path = processing_directory.join("source").join("original");
        self.object_storage
            .download_source_object(&job.source_s3_key, &source_path)
            .await?;

        let source_metadata = self.source_probe.probe(&source_path).await?;
        let baseline_rendition = self.policy.baseline_rendition_name();
        let additional_renditions = self
            .policy
            .source_eligible_additional_renditions(source_metadata.width, source_metadata.height);
        self.repository
            .dispatch_transcoding_jobs(
                job.job_id,
                job.video_id,
                source_metadata.width,
                source_metadata.height,
                &baseline_rendition,
                &additional_renditions,
                job.attempt,
            )
            .await?;

        Ok(())
    }

    async fn prepare_processing_directory(&self, processing_directory: &PathBuf) -> AppResult<()> {
        tokio::fs::create_dir_all(processing_directory.join("source")).await?;
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
}

fn map_chunker_failure(
    job: &ClaimedBaselineJob,
    disposition: ChunkerFailureDisposition,
) -> ChunkerProcessOutcome {
    match disposition {
        ChunkerFailureDisposition::RetryQueued { next_attempt } => {
            ChunkerProcessOutcome::RetryQueued {
                job_id: job.job_id,
                video_id: job.video_id,
                next_attempt,
            }
        }
        ChunkerFailureDisposition::Terminal => ChunkerProcessOutcome::FailedTerminal {
            job_id: job.job_id,
            video_id: job.video_id,
        },
    }
}
