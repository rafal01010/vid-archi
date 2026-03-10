use sqlx::{postgres::PgPoolOptions, FromRow, PgPool};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Clone)]
pub struct ChunkerRepository {
    pool: PgPool,
}

impl ChunkerRepository {
    pub async fn connect(database_url: &str) -> AppResult<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await?;

        Ok(Self { pool })
    }

    pub async fn claim_baseline_job(
        &self,
        processing_job_id: Uuid,
        worker_id: &str,
    ) -> AppResult<Option<ClaimedBaselineJob>> {
        let mut transaction = self.pool.begin().await?;

        let claimed_job = sqlx::query_as::<_, ClaimedBaselineJob>(
            r#"
            WITH candidate AS (
                SELECT
                    p.id AS job_id,
                    p.video_id,
                    p.attempt,
                    v.source_s3_key,
                    p.correlation_id
                FROM processing_jobs p
                INNER JOIN videos v
                    ON v.id = p.video_id
                WHERE p.id = $2
                  AND p.job_type = 'BASELINE'::processing_job_type
                  AND p.status = 'QUEUED'::processing_job_status
                  AND v.status = 'UPLOADED'::video_status
                FOR UPDATE SKIP LOCKED
            )
            UPDATE processing_jobs p
            SET status = 'RUNNING'::processing_job_status,
                worker_id = $1,
                started_at = NOW(),
                finished_at = NULL,
                error = NULL
            FROM candidate
            WHERE p.id = candidate.job_id
            RETURNING
                p.id AS job_id,
                p.video_id,
                candidate.attempt,
                candidate.source_s3_key,
                candidate.correlation_id
            "#,
        )
        .bind(worker_id)
        .bind(processing_job_id)
        .fetch_optional(&mut *transaction)
        .await?;

        let Some(claimed_job) = claimed_job else {
            transaction.commit().await?;
            return Ok(None);
        };

        let updated_rows = sqlx::query(
            r#"
            UPDATE videos
            SET status = 'PROCESSING_BASELINE'::video_status,
                error_code = NULL,
                error_message = NULL,
                updated_at = NOW()
            WHERE id = $1
              AND status = 'UPLOADED'::video_status
            "#,
        )
        .bind(claimed_job.video_id)
        .execute(&mut *transaction)
        .await?
        .rows_affected();

        if updated_rows != 1 {
            return Err(AppError::conflict(
                "video was not in UPLOADED state when chunker claimed the baseline job",
            ));
        }

        transaction.commit().await?;

        Ok(Some(claimed_job))
    }

    pub async fn video_exists(&self, video_id: Uuid) -> AppResult<bool> {
        sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM videos
                WHERE id = $1
            )
            "#,
        )
        .bind(video_id)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::from)
    }

    pub async fn dispatch_transcoding_jobs(
        &self,
        processing_job_id: Uuid,
        video_id: Uuid,
        correlation_id: &str,
        source_width: u32,
        source_height: u32,
        baseline_rendition_name: &str,
        additional_rendition_names: &[String],
        source_segments: &[SourceSegmentRecord],
        attempt: i32,
    ) -> AppResult<Vec<QueuedTranscodingJobRecord>> {
        let mut transaction = self.pool.begin().await?;

        sqlx::query(
            r#"
            UPDATE videos
            SET source_width = $2,
                source_height = $3,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(video_id)
        .bind(source_width as i32)
        .bind(source_height as i32)
        .execute(&mut *transaction)
        .await?;

        for source_segment in source_segments {
            sqlx::query(
                r#"
                INSERT INTO transcoding_jobs (
                    id,
                    processing_job_id,
                    video_id,
                    rendition,
                    segment_index,
                    source_segment_s3_key,
                    source_segment_duration_seconds,
                    attempt,
                    status,
                    correlation_id
                )
                VALUES (
                    $1,
                    $2,
                    $3,
                    $4::video_rendition_name,
                    $5,
                    $6,
                    $7,
                    $8,
                    'QUEUED'::transcoding_job_status,
                    $9
                )
                ON CONFLICT (video_id, rendition, segment_index, attempt) DO NOTHING
                "#,
            )
            .bind(Uuid::new_v4())
            .bind(processing_job_id)
            .bind(video_id)
            .bind(baseline_rendition_name)
            .bind(source_segment.segment_index)
            .bind(&source_segment.source_segment_s3_key)
            .bind(source_segment.duration_seconds)
            .bind(attempt)
            .bind(correlation_id)
            .execute(&mut *transaction)
            .await?;
        }

        if !additional_rendition_names.is_empty() {
            let additional_renditions_job_id = Uuid::new_v4();

            sqlx::query(
                r#"
                INSERT INTO processing_jobs (
                    id,
                    video_id,
                    job_type,
                    attempt,
                    status,
                    correlation_id
                )
                VALUES (
                    $1,
                    $2,
                    'ADDITIONAL_RENDITIONS'::processing_job_type,
                    1,
                    'QUEUED'::processing_job_status,
                    $3
                )
                ON CONFLICT (video_id, job_type, attempt) DO NOTHING
                "#,
            )
            .bind(additional_renditions_job_id)
            .bind(video_id)
            .bind(correlation_id)
            .execute(&mut *transaction)
            .await?;

            let queued_additional_job_id = sqlx::query_scalar::<_, Uuid>(
                r#"
                SELECT id
                FROM processing_jobs
                WHERE video_id = $1
                  AND job_type = 'ADDITIONAL_RENDITIONS'::processing_job_type
                ORDER BY attempt DESC
                LIMIT 1
                "#,
            )
            .bind(video_id)
            .fetch_one(&mut *transaction)
            .await?;

            for rendition_name in additional_rendition_names {
                for source_segment in source_segments {
                    sqlx::query(
                        r#"
                        INSERT INTO transcoding_jobs (
                            id,
                            processing_job_id,
                            video_id,
                            rendition,
                            segment_index,
                            source_segment_s3_key,
                            source_segment_duration_seconds,
                            attempt,
                            status,
                            correlation_id
                        )
                        VALUES (
                            $1,
                            $2,
                            $3,
                            $4::video_rendition_name,
                            $5,
                            $6,
                            $7,
                            1,
                            'QUEUED'::transcoding_job_status,
                            $8
                        )
                        ON CONFLICT (video_id, rendition, segment_index, attempt) DO NOTHING
                        "#,
                    )
                    .bind(Uuid::new_v4())
                    .bind(queued_additional_job_id)
                    .bind(video_id)
                    .bind(rendition_name)
                    .bind(source_segment.segment_index)
                    .bind(&source_segment.source_segment_s3_key)
                    .bind(source_segment.duration_seconds)
                    .bind(correlation_id)
                    .execute(&mut *transaction)
                    .await?;
                }
            }
        }

        let baseline_jobs = sqlx::query_as::<_, QueuedTranscodingJobRecord>(
            r#"
            SELECT
                id AS transcoding_job_id,
                processing_job_id,
                video_id,
                rendition::text AS rendition,
                segment_index,
                correlation_id,
                attempt
            FROM transcoding_jobs
            WHERE video_id = $1
              AND processing_job_id = $2
              AND rendition = $3::video_rendition_name
              AND attempt = $4
            ORDER BY segment_index ASC
            "#,
        )
        .bind(video_id)
        .bind(processing_job_id)
        .bind(baseline_rendition_name)
        .bind(attempt)
        .fetch_all(&mut *transaction)
        .await?;

        transaction.commit().await?;

        Ok(baseline_jobs)
    }

    pub async fn list_queued_transcoding_jobs_for_processing_job(
        &self,
        processing_job_id: Uuid,
    ) -> AppResult<Vec<QueuedTranscodingJobRecord>> {
        sqlx::query_as::<_, QueuedTranscodingJobRecord>(
            r#"
            SELECT
                id AS transcoding_job_id,
                processing_job_id,
                video_id,
                rendition::text AS rendition,
                segment_index,
                correlation_id,
                attempt
            FROM transcoding_jobs
            WHERE processing_job_id = $1
              AND status = 'QUEUED'::transcoding_job_status
            ORDER BY segment_index ASC, rendition ASC
            "#,
        )
        .bind(processing_job_id)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)
    }

    pub async fn mark_chunking_failed(
        &self,
        processing_job_id: Uuid,
        video_id: Uuid,
        attempt: i32,
        correlation_id: &str,
        error_message: &str,
        max_processing_attempts: i32,
    ) -> AppResult<ChunkerFailureDisposition> {
        let mut transaction = self.pool.begin().await?;
        let truncated_error = truncate_error_message(error_message);

        sqlx::query(
            r#"
            UPDATE processing_jobs
            SET status = 'FAILED'::processing_job_status,
                finished_at = NOW(),
                error = $2,
                updated_at = NOW()
            WHERE id = $1
              AND status = 'RUNNING'::processing_job_status
            "#,
        )
        .bind(processing_job_id)
        .bind(&truncated_error)
        .execute(&mut *transaction)
        .await?;

        if attempt < max_processing_attempts {
            let retry_job_id = Uuid::new_v4();
            sqlx::query(
                r#"
                INSERT INTO processing_jobs (
                    id,
                    video_id,
                    job_type,
                    attempt,
                    status,
                    correlation_id
                )
                VALUES (
                    $1,
                    $2,
                    'BASELINE'::processing_job_type,
                    $3,
                    'QUEUED'::processing_job_status,
                    $4
                )
                ON CONFLICT (video_id, job_type, attempt) DO NOTHING
                "#,
            )
            .bind(retry_job_id)
            .bind(video_id)
            .bind(attempt + 1)
            .bind(correlation_id)
            .execute(&mut *transaction)
            .await?;

            sqlx::query(
                r#"
                UPDATE videos
                SET status = 'UPLOADED'::video_status,
                    error_code = NULL,
                    error_message = NULL,
                    updated_at = NOW()
                WHERE id = $1
                  AND status = 'PROCESSING_BASELINE'::video_status
                "#,
            )
            .bind(video_id)
            .execute(&mut *transaction)
            .await?;

            transaction.commit().await?;

            return Ok(ChunkerFailureDisposition::RetryQueued {
                retry_job_id,
                next_attempt: attempt + 1,
            });
        }

        sqlx::query(
            r#"
            UPDATE videos
            SET status = 'FAILED'::video_status,
                is_streamable = FALSE,
                error_code = 'CHUNKER_DISPATCH_FAILED',
                error_message = $2,
                updated_at = NOW()
            WHERE id = $1
              AND status = 'PROCESSING_BASELINE'::video_status
            "#,
        )
        .bind(video_id)
        .bind("This video could not be prepared for playback.")
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;

        Ok(ChunkerFailureDisposition::Terminal)
    }
}

fn truncate_error_message(message: &str) -> String {
    const MAX_ERROR_LENGTH: usize = 4000;

    if message.len() <= MAX_ERROR_LENGTH {
        return message.to_owned();
    }

    message[..MAX_ERROR_LENGTH].to_owned()
}

#[derive(Debug, FromRow)]
pub struct ClaimedBaselineJob {
    pub job_id: Uuid,
    pub video_id: Uuid,
    pub attempt: i32,
    pub source_s3_key: String,
    pub correlation_id: String,
}

#[derive(Debug, Clone)]
pub struct SourceSegmentRecord {
    pub segment_index: i32,
    pub source_segment_s3_key: String,
    pub duration_seconds: f64,
}

#[derive(Debug)]
pub enum ChunkerFailureDisposition {
    RetryQueued {
        retry_job_id: Uuid,
        next_attempt: i32,
    },
    Terminal,
}

#[derive(Debug, FromRow, Clone)]
pub struct QueuedTranscodingJobRecord {
    pub transcoding_job_id: Uuid,
    pub processing_job_id: Uuid,
    pub video_id: Uuid,
    pub rendition: String,
    pub segment_index: i32,
    pub correlation_id: String,
    pub attempt: i32,
}
