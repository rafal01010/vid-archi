use sqlx::{postgres::PgPoolOptions, FromRow, PgPool};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Clone)]
pub struct TranscoderRepository {
    pool: PgPool,
}

impl TranscoderRepository {
    pub async fn connect(database_url: &str) -> AppResult<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await?;

        Ok(Self { pool })
    }

    pub async fn claim_next_transcoding_job(
        &self,
        worker_id: &str,
        rendition_name: &str,
    ) -> AppResult<Option<ClaimedTranscodingJob>> {
        let mut transaction = self.pool.begin().await?;

        let claimed_job = sqlx::query_as::<_, ClaimedTranscodingJob>(
            r#"
            WITH candidate AS (
                SELECT
                    tj.id AS transcoding_job_id,
                    tj.processing_job_id,
                    tj.video_id,
                    tj.rendition::text AS rendition,
                    v.source_s3_key,
                    v.source_width,
                    v.source_height
                FROM transcoding_jobs tj
                INNER JOIN videos v
                    ON v.id = tj.video_id
                INNER JOIN processing_jobs p
                    ON p.id = tj.processing_job_id
                WHERE tj.status = 'QUEUED'::transcoding_job_status
                  AND tj.rendition = $2::video_rendition_name
                  AND p.status = 'RUNNING'::processing_job_status
                  AND v.status IN ('PROCESSING_BASELINE'::video_status, 'PROCESSING_FULL'::video_status)
                ORDER BY tj.created_at ASC
                FOR UPDATE SKIP LOCKED
                LIMIT 1
            )
            UPDATE transcoding_jobs tj
            SET status = 'RUNNING'::transcoding_job_status,
                worker_id = $1,
                started_at = NOW(),
                finished_at = NULL,
                error = NULL
            FROM candidate
            WHERE tj.id = candidate.transcoding_job_id
            RETURNING
                tj.id AS transcoding_job_id,
                candidate.processing_job_id,
                candidate.video_id,
                candidate.rendition,
                candidate.source_s3_key,
                candidate.source_width,
                candidate.source_height
            "#,
        )
        .bind(worker_id)
        .bind(rendition_name)
        .fetch_optional(&mut *transaction)
        .await?;

        transaction.commit().await?;

        Ok(claimed_job)
    }

    pub async fn mark_rendition_processing(
        &self,
        video_id: Uuid,
        rendition_name: &str,
        codec: &str,
        container: &str,
        playlist_key: &str,
    ) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO video_renditions (
                video_id,
                rendition,
                codec,
                container,
                playlist_key,
                status,
                segment_count
            )
            VALUES (
                $1,
                $2::video_rendition_name,
                $3,
                $4,
                $5,
                'PROCESSING'::video_rendition_status,
                0
            )
            ON CONFLICT (video_id, rendition) DO UPDATE
            SET codec = EXCLUDED.codec,
                container = EXCLUDED.container,
                playlist_key = EXCLUDED.playlist_key,
                status = 'PROCESSING'::video_rendition_status,
                updated_at = NOW()
            "#,
        )
        .bind(video_id)
        .bind(rendition_name)
        .bind(codec)
        .bind(container)
        .bind(playlist_key)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn mark_transcoding_succeeded(
        &self,
        update: TranscodingCompletionUpdate,
        promote_video: bool,
    ) -> AppResult<()> {
        let mut transaction = self.pool.begin().await?;

        sqlx::query(
            r#"
            UPDATE video_renditions
            SET codec = $3,
                container = $4,
                playlist_key = $5,
                status = 'READY'::video_rendition_status,
                segment_count = $6,
                updated_at = NOW()
            WHERE video_id = $1
              AND rendition = $2::video_rendition_name
            "#,
        )
        .bind(update.video_id)
        .bind(&update.rendition_name)
        .bind(&update.codec)
        .bind(&update.container)
        .bind(&update.playlist_key)
        .bind(update.segment_count)
        .execute(&mut *transaction)
        .await?;

        sqlx::query(
            r#"
            UPDATE transcoding_jobs
            SET status = 'SUCCEEDED'::transcoding_job_status,
                finished_at = NOW(),
                error = NULL,
                updated_at = NOW()
            WHERE id = $1
              AND status = 'RUNNING'::transcoding_job_status
            "#,
        )
        .bind(update.transcoding_job_id)
        .execute(&mut *transaction)
        .await?;

        if promote_video {
            let manifest_s3_key = update.manifest_s3_key.ok_or_else(|| {
                AppError::conflict("baseline transcoder completion requires a master manifest key")
            })?;

            sqlx::query(
                r#"
                UPDATE videos
                SET status = 'BASELINE_READY'::video_status,
                    is_streamable = TRUE,
                    manifest_s3_key = $2,
                    baseline_ready_at = COALESCE(baseline_ready_at, NOW()),
                    error_code = NULL,
                    error_message = NULL,
                    updated_at = NOW()
                WHERE id = $1
                  AND status = 'PROCESSING_BASELINE'::video_status
                "#,
            )
            .bind(update.video_id)
            .bind(&manifest_s3_key)
            .execute(&mut *transaction)
            .await?;

            sqlx::query(
                r#"
                UPDATE processing_jobs
                SET status = 'SUCCEEDED'::processing_job_status,
                    finished_at = NOW(),
                    error = NULL,
                    updated_at = NOW()
                WHERE id = $1
                  AND status = 'RUNNING'::processing_job_status
                "#,
            )
            .bind(update.processing_job_id)
            .execute(&mut *transaction)
            .await?;
        }

        transaction.commit().await?;

        Ok(())
    }

    pub async fn mark_transcoding_failed(
        &self,
        transcoding_job_id: Uuid,
        processing_job_id: Uuid,
        video_id: Uuid,
        rendition_name: &str,
        error_message: &str,
        promote_failure_to_video: bool,
    ) -> AppResult<()> {
        let mut transaction = self.pool.begin().await?;
        let truncated_error = truncate_error_message(error_message);

        sqlx::query(
            r#"
            UPDATE transcoding_jobs
            SET status = 'FAILED'::transcoding_job_status,
                finished_at = NOW(),
                error = $2,
                updated_at = NOW()
            WHERE id = $1
              AND status = 'RUNNING'::transcoding_job_status
            "#,
        )
        .bind(transcoding_job_id)
        .bind(&truncated_error)
        .execute(&mut *transaction)
        .await?;

        sqlx::query(
            r#"
            UPDATE video_renditions
            SET status = 'FAILED'::video_rendition_status,
                updated_at = NOW()
            WHERE video_id = $1
              AND rendition = $2::video_rendition_name
            "#,
        )
        .bind(video_id)
        .bind(rendition_name)
        .execute(&mut *transaction)
        .await?;

        if promote_failure_to_video {
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

            sqlx::query(
                r#"
                UPDATE videos
                SET status = 'FAILED'::video_status,
                    is_streamable = FALSE,
                    error_code = 'TRANSCODER_FAILED',
                    error_message = $2,
                    updated_at = NOW()
                WHERE id = $1
                  AND status = 'PROCESSING_BASELINE'::video_status
                "#,
            )
            .bind(video_id)
            .bind(&truncated_error)
            .execute(&mut *transaction)
            .await?;
        }

        transaction.commit().await?;

        Ok(())
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
pub struct ClaimedTranscodingJob {
    pub transcoding_job_id: Uuid,
    pub processing_job_id: Uuid,
    pub video_id: Uuid,
    pub rendition: String,
    pub source_s3_key: String,
    pub source_width: Option<i32>,
    pub source_height: Option<i32>,
}

#[derive(Debug)]
pub struct TranscodingCompletionUpdate {
    pub transcoding_job_id: Uuid,
    pub processing_job_id: Uuid,
    pub video_id: Uuid,
    pub rendition_name: String,
    pub codec: String,
    pub container: String,
    pub playlist_key: String,
    pub segment_count: i32,
    pub manifest_s3_key: Option<String>,
}
