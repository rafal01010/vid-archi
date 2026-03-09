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

    pub async fn claim_next_baseline_job(
        &self,
        worker_id: &str,
    ) -> AppResult<Option<ClaimedBaselineJob>> {
        let mut transaction = self.pool.begin().await?;

        let claimed_job = sqlx::query_as::<_, ClaimedBaselineJob>(
            r#"
            WITH candidate AS (
                SELECT
                    p.id AS job_id,
                    p.video_id,
                    v.source_s3_key
                FROM processing_jobs p
                INNER JOIN videos v
                    ON v.id = p.video_id
                WHERE p.job_type = 'BASELINE'::processing_job_type
                  AND p.status = 'QUEUED'::processing_job_status
                  AND v.status = 'UPLOADED'::video_status
                ORDER BY p.created_at ASC
                FOR UPDATE SKIP LOCKED
                LIMIT 1
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
                candidate.source_s3_key
            "#,
        )
        .bind(worker_id)
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

    pub async fn record_source_dimensions(
        &self,
        video_id: Uuid,
        source_width: u32,
        source_height: u32,
    ) -> AppResult<()> {
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
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn queue_transcoding_job(
        &self,
        processing_job_id: Uuid,
        video_id: Uuid,
        rendition_name: &str,
    ) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO transcoding_jobs (
                id,
                processing_job_id,
                video_id,
                rendition,
                attempt,
                status
            )
            VALUES (
                $1,
                $2,
                $3,
                $4::video_rendition_name,
                1,
                'QUEUED'::transcoding_job_status
            )
            ON CONFLICT (video_id, rendition, attempt) DO NOTHING
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(processing_job_id)
        .bind(video_id)
        .bind(rendition_name)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn queue_additional_renditions_jobs(
        &self,
        video_id: Uuid,
        rendition_names: &[String],
    ) -> AppResult<()> {
        if rendition_names.is_empty() {
            return Ok(());
        }

        let mut transaction = self.pool.begin().await?;
        let additional_renditions_job_id = Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT INTO processing_jobs (
                id,
                video_id,
                job_type,
                attempt,
                status
            )
            VALUES (
                $1,
                $2,
                'ADDITIONAL_RENDITIONS'::processing_job_type,
                1,
                'QUEUED'::processing_job_status
            )
            ON CONFLICT (video_id, job_type, attempt) DO NOTHING
            "#,
        )
        .bind(additional_renditions_job_id)
        .bind(video_id)
        .execute(&mut *transaction)
        .await?;

        let queued_job = sqlx::query_scalar::<_, Uuid>(
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

        for rendition_name in rendition_names {
            sqlx::query(
                r#"
                INSERT INTO transcoding_jobs (
                    id,
                    processing_job_id,
                    video_id,
                    rendition,
                    attempt,
                    status
                )
                VALUES (
                    $1,
                    $2,
                    $3,
                    $4::video_rendition_name,
                    1,
                    'QUEUED'::transcoding_job_status
                )
                ON CONFLICT (video_id, rendition, attempt) DO NOTHING
                "#,
            )
            .bind(Uuid::new_v4())
            .bind(queued_job)
            .bind(video_id)
            .bind(rendition_name)
            .execute(&mut *transaction)
            .await?;
        }

        transaction.commit().await?;

        Ok(())
    }

    pub async fn mark_chunking_failed(
        &self,
        processing_job_id: Uuid,
        video_id: Uuid,
        error_message: &str,
    ) -> AppResult<()> {
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
        .bind(&truncated_error)
        .execute(&mut *transaction)
        .await?;

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
pub struct ClaimedBaselineJob {
    pub job_id: Uuid,
    pub video_id: Uuid,
    pub source_s3_key: String,
}
