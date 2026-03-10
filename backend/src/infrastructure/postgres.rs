use chrono::{DateTime, Utc};
use sqlx::{postgres::PgPoolOptions, FromRow, PgPool};
use uuid::Uuid;

use crate::http::error::{AppError, AppResult};

#[derive(Clone)]
pub struct VideoRepository {
    pool: PgPool,
}

impl VideoRepository {
    pub async fn connect(database_url: &str) -> AppResult<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await?;

        Ok(Self { pool })
    }

    pub async fn public_id_exists(&self, public_id: &str) -> AppResult<bool> {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM videos WHERE public_id = $1)",
        )
        .bind(public_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(exists)
    }

    pub async fn count_videos(&self) -> AppResult<i64> {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM videos")
            .fetch_one(&self.pool)
            .await
            .map_err(AppError::from)
    }

    pub async fn list_recent_videos(
        &self,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<VideoSummaryRecord>> {
        sqlx::query_as::<_, VideoSummaryRecord>(
            r#"
            SELECT
                public_id,
                title,
                original_filename,
                status::text AS status,
                is_streamable,
                created_at,
                updated_at
            FROM videos
            ORDER BY created_at DESC
            LIMIT $1
            OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)
    }

    pub async fn find_video_by_public_id(
        &self,
        public_id: &str,
    ) -> AppResult<Option<VideoDetailRecord>> {
        sqlx::query_as::<_, VideoDetailRecord>(
            r#"
            SELECT
                id,
                public_id,
                title,
                original_filename,
                status::text AS status,
                is_streamable,
                created_at,
                updated_at,
                baseline_ready_at,
                CASE
                    WHEN status = 'READY'::video_status THEN ready_at
                    ELSE NULL
                END AS processing_completed_at,
                manifest_s3_key,
                error_code,
                error_message
            FROM videos
            WHERE public_id = $1
            "#,
        )
        .bind(public_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::from)
    }

    pub async fn list_ready_renditions(
        &self,
        video_id: Uuid,
    ) -> AppResult<Vec<ReadyRenditionRecord>> {
        sqlx::query_as::<_, ReadyRenditionRecord>(
            r#"
            SELECT
                rendition::text AS rendition,
                codec,
                container,
                playlist_key,
                output_width,
                output_height
            FROM video_renditions
            WHERE video_id = $1
              AND status = 'READY'::video_rendition_status
            ORDER BY created_at ASC
            "#,
        )
        .bind(video_id)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)
    }

    pub async fn find_video_delete_context(
        &self,
        public_id: &str,
    ) -> AppResult<Option<VideoDeleteContext>> {
        sqlx::query_as::<_, VideoDeleteContext>(
            r#"
            SELECT
                v.id,
                v.public_id,
                v.source_s3_key,
                v.status::text AS status,
                s.s3_upload_id AS active_upload_id
            FROM videos v
            LEFT JOIN upload_sessions s
                ON s.video_id = v.id
               AND s.status = 'OPEN'::upload_session_status
            WHERE v.public_id = $1
            "#,
        )
        .bind(public_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::from)
    }

    pub async fn verify_delete_code(&self, video_id: Uuid, delete_code: &str) -> AppResult<bool> {
        sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM videos
                WHERE id = $1
                  AND delete_code_hash = encode(
                      digest(delete_code_salt || ':' || $2, 'sha256'),
                      'hex'
                  )
            )
            "#,
        )
        .bind(video_id)
        .bind(delete_code)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::from)
    }

    pub async fn delete_video(&self, video_id: Uuid) -> AppResult<DeletedVideoRecord> {
        sqlx::query_as::<_, DeletedVideoRecord>(
            r#"
            DELETE FROM videos
            WHERE id = $1
            RETURNING public_id
            "#,
        )
        .bind(video_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| match error {
            sqlx::Error::RowNotFound => AppError::not_found("video not found"),
            other => AppError::from(other),
        })
    }

    pub async fn create_video_and_upload_session(
        &self,
        video: CreateVideoRecord,
        upload_session: CreateUploadSessionRecord,
    ) -> AppResult<()> {
        let mut transaction = self.pool.begin().await?;

        sqlx::query(
            r#"
            WITH generated_delete_code AS (
                SELECT encode(gen_random_bytes(16), 'hex') AS delete_code_salt
            )
            INSERT INTO videos (
                id,
                public_id,
                delete_code_salt,
                delete_code_hash,
                title,
                original_filename,
                content_type,
                size_bytes,
                source_s3_key,
                status
            )
            SELECT
                $1,
                $2,
                generated_delete_code.delete_code_salt,
                encode(
                    digest(generated_delete_code.delete_code_salt || ':' || $3, 'sha256'),
                    'hex'
                ),
                $4,
                $5,
                $6,
                $7,
                $8,
                'INITIATED'::video_status
            FROM generated_delete_code
            "#,
        )
        .bind(video.id)
        .bind(video.public_id)
        .bind(video.delete_code)
        .bind(video.title)
        .bind(video.original_filename)
        .bind(video.content_type)
        .bind(video.size_bytes)
        .bind(video.source_s3_key)
        .execute(&mut *transaction)
        .await
        .map_err(map_insert_video_error)?;

        sqlx::query(
            r#"
            INSERT INTO upload_sessions (
                id,
                video_id,
                s3_upload_id,
                part_size_bytes,
                expires_at,
                status
            )
            VALUES ($1, $2, $3, $4, $5, 'OPEN'::upload_session_status)
            "#,
        )
        .bind(upload_session.id)
        .bind(upload_session.video_id)
        .bind(upload_session.s3_upload_id)
        .bind(upload_session.part_size_bytes)
        .bind(upload_session.expires_at)
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;
        Ok(())
    }

    pub async fn find_upload_session(
        &self,
        video_id: Uuid,
        upload_session_id: Uuid,
    ) -> AppResult<Option<UploadSessionContext>> {
        sqlx::query_as::<_, UploadSessionContext>(
            r#"
            SELECT
                v.id AS video_id,
                v.public_id,
                v.status::text AS video_status,
                v.size_bytes,
                v.source_s3_key,
                s.id AS upload_session_id,
                s.s3_upload_id,
                s.part_size_bytes,
                s.expires_at,
                s.status::text AS upload_session_status
            FROM videos v
            INNER JOIN upload_sessions s
                ON s.video_id = v.id
            WHERE v.id = $1
              AND s.id = $2
            "#,
        )
        .bind(video_id)
        .bind(upload_session_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::from)
    }

    pub async fn mark_video_uploading(&self, video_id: Uuid) -> AppResult<()> {
        sqlx::query(
            r#"
            UPDATE videos
            SET status = 'UPLOADING'::video_status,
                updated_at = NOW()
            WHERE id = $1
              AND status = 'INITIATED'::video_status
            "#,
        )
        .bind(video_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn finalize_completed_upload(
        &self,
        video_id: Uuid,
        upload_session_id: Uuid,
        correlation_id: &str,
    ) -> AppResult<FinalizedUploadRecord> {
        let mut transaction = self.pool.begin().await?;

        let updated_rows = sqlx::query(
            r#"
            UPDATE upload_sessions
            SET status = 'COMPLETED'::upload_session_status,
                updated_at = NOW()
            WHERE id = $1
              AND video_id = $2
              AND status IN ('OPEN'::upload_session_status, 'COMPLETED'::upload_session_status)
            "#,
        )
        .bind(upload_session_id)
        .bind(video_id)
        .execute(&mut *transaction)
        .await?
        .rows_affected();

        if updated_rows == 0 {
            return Err(AppError::conflict(
                "upload session is not open and cannot be completed",
            ));
        }

        sqlx::query(
            r#"
            UPDATE videos
            SET status = CASE
                    WHEN status IN ('INITIATED'::video_status, 'UPLOADING'::video_status)
                        THEN 'UPLOADED'::video_status
                    ELSE status
                END,
                error_code = NULL,
                error_message = NULL,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(video_id)
        .execute(&mut *transaction)
        .await?;

        let processing_job_id = Uuid::new_v4();

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
                1,
                'QUEUED'::processing_job_status,
                $3
            )
            ON CONFLICT (video_id, job_type, attempt) DO NOTHING
            "#,
        )
        .bind(processing_job_id)
        .bind(video_id)
        .bind(correlation_id)
        .execute(&mut *transaction)
        .await?;

        let finalized_upload = sqlx::query_as::<_, FinalizedUploadRecord>(
            r#"
            SELECT
                v.id AS video_id,
                v.public_id,
                v.status::text AS video_status,
                p.id AS processing_job_id
            FROM videos v
            INNER JOIN processing_jobs p
                ON p.video_id = v.id
            WHERE v.id = $1
              AND p.job_type = 'BASELINE'::processing_job_type
            ORDER BY p.attempt DESC
            LIMIT 1
            "#,
        )
        .bind(video_id)
        .fetch_one(&mut *transaction)
        .await?;

        transaction.commit().await?;

        Ok(finalized_upload)
    }

    pub async fn get_finalized_upload(
        &self,
        video_id: Uuid,
    ) -> AppResult<Option<FinalizedUploadRecord>> {
        sqlx::query_as::<_, FinalizedUploadRecord>(
            r#"
            SELECT
                v.id AS video_id,
                v.public_id,
                v.status::text AS video_status,
                p.id AS processing_job_id
            FROM videos v
            INNER JOIN processing_jobs p
                ON p.video_id = v.id
            WHERE v.id = $1
              AND p.job_type = 'BASELINE'::processing_job_type
            ORDER BY p.attempt DESC
            LIMIT 1
            "#,
        )
        .bind(video_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::from)
    }
}

fn map_insert_video_error(error: sqlx::Error) -> AppError {
    match error {
        sqlx::Error::Database(database_error)
            if database_error.constraint() == Some("videos_public_id_key") =>
        {
            AppError::conflict("generated public share id already exists")
        }
        other => AppError::from(other),
    }
}

#[derive(Debug)]
pub struct CreateVideoRecord {
    pub id: Uuid,
    pub public_id: String,
    pub delete_code: String,
    pub title: Option<String>,
    pub original_filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub source_s3_key: String,
}

#[derive(Debug)]
pub struct CreateUploadSessionRecord {
    pub id: Uuid,
    pub video_id: Uuid,
    pub s3_upload_id: String,
    pub part_size_bytes: i64,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
pub struct UploadSessionContext {
    pub video_id: Uuid,
    pub public_id: String,
    pub video_status: String,
    pub size_bytes: i64,
    pub source_s3_key: String,
    pub upload_session_id: Uuid,
    pub s3_upload_id: String,
    pub part_size_bytes: i64,
    pub expires_at: DateTime<Utc>,
    pub upload_session_status: String,
}

#[derive(Debug, FromRow)]
pub struct FinalizedUploadRecord {
    pub video_id: Uuid,
    pub public_id: String,
    pub video_status: String,
    pub processing_job_id: Uuid,
}

#[derive(Debug, FromRow)]
pub struct VideoSummaryRecord {
    pub public_id: String,
    pub title: Option<String>,
    pub original_filename: String,
    pub status: String,
    pub is_streamable: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
pub struct VideoDetailRecord {
    pub id: Uuid,
    pub public_id: String,
    pub title: Option<String>,
    pub original_filename: String,
    pub status: String,
    pub is_streamable: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub baseline_ready_at: Option<DateTime<Utc>>,
    pub processing_completed_at: Option<DateTime<Utc>>,
    pub manifest_s3_key: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, FromRow)]
pub struct VideoDeleteContext {
    pub id: Uuid,
    pub public_id: String,
    pub source_s3_key: String,
    pub status: String,
    pub active_upload_id: Option<String>,
}

#[derive(Debug, FromRow)]
pub struct DeletedVideoRecord {
    pub public_id: String,
}

#[derive(Debug, FromRow)]
pub struct ReadyRenditionRecord {
    pub rendition: String,
    pub codec: String,
    pub container: String,
    pub playlist_key: String,
    pub output_width: Option<i32>,
    pub output_height: Option<i32>,
}
