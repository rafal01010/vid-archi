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
    ChunkerFailureDisposition, ChunkerRepository, ClaimedBaselineJob,
};
use crate::media::SourceProbe;

#[derive(Debug)]
pub enum ChunkerProcessOutcome {
    Dispatched {
        job_id: Uuid,
        video_id: Uuid,
        transcoder_message: TranscoderJobMessage,
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

    pub async fn process_job(&self, processing_job_id: Uuid) -> AppResult<ChunkerProcessOutcome> {
        let Some(job) = self
            .repository
            .claim_baseline_job(processing_job_id, &self.config.chunker_id)
            .await?
        else {
            return Ok(ChunkerProcessOutcome::Ignored {
                job_id: processing_job_id,
            });
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
            Ok(transcoder_message) => Ok(ChunkerProcessOutcome::Dispatched {
                job_id: job.job_id,
                video_id: job.video_id,
                transcoder_message,
            }),
            Err(error) => {
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
    ) -> AppResult<TranscoderJobMessage> {
        tracing::info!(source_s3_key = %job.source_s3_key, "claimed baseline processing job");

        let source_path = processing_directory.join("source").join("original");
        self.object_storage
            .download_source_object(&job.source_s3_key, &source_path)
            .await?;
        tracing::info!(path = %source_path.display(), "downloaded source object for probing");

        let source_metadata = self.source_probe.probe(&source_path).await?;
        let baseline_rendition = self.policy.baseline_rendition_name();
        let additional_renditions = self
            .policy
            .source_eligible_additional_renditions(source_metadata.width, source_metadata.height);
        tracing::info!(
            source_width = source_metadata.width,
            source_height = source_metadata.height,
            baseline_rendition = %baseline_rendition,
            additional_rendition_count = additional_renditions.len(),
            "probed source media and prepared transcoding dispatch"
        );
        let dispatch_result = self
            .repository
            .dispatch_transcoding_jobs(
                job.job_id,
                job.video_id,
                &job.correlation_id,
                source_metadata.width,
                source_metadata.height,
                &baseline_rendition,
                &additional_renditions,
                job.attempt,
            )
            .await?;
        tracing::info!("queued baseline and additional transcoding jobs");

        Ok(TranscoderJobMessage {
            transcoding_job_id: dispatch_result.baseline_transcoding_job_id,
            processing_job_id: job.job_id,
            video_id: job.video_id,
            rendition: baseline_rendition,
            correlation_id: job.correlation_id.clone(),
            attempt: job.attempt,
        })
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
        ChunkerFailureDisposition::RetryQueued {
            retry_job_id,
            next_attempt,
        } => {
            ChunkerProcessOutcome::RetryQueued {
                job_id: job.job_id,
                video_id: job.video_id,
                retry_job_id,
                next_attempt,
            }
        }
        ChunkerFailureDisposition::Terminal => ChunkerProcessOutcome::FailedTerminal {
            job_id: job.job_id,
            video_id: job.video_id,
        },
    }
}
