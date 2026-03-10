use sqlx::{pool::PoolConnection, postgres::PgPoolOptions, FromRow, PgPool, Postgres};
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

    pub async fn claim_transcoding_job(
        &self,
        transcoding_job_id: Uuid,
        worker_id: &str,
        rendition_name: &str,
        is_baseline_rendition: bool,
    ) -> AppResult<Option<ClaimedTranscodingJob>> {
        if is_baseline_rendition {
            return self
                .claim_baseline_transcoding_job(transcoding_job_id, worker_id, rendition_name)
                .await;
        }

        self.claim_additional_renditions_job(transcoding_job_id, worker_id, rendition_name)
            .await
    }

    async fn claim_baseline_transcoding_job(
        &self,
        transcoding_job_id: Uuid,
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
                    tj.segment_index,
                    tj.source_segment_s3_key,
                    tj.source_segment_duration_seconds,
                    tj.attempt,
                    v.source_width,
                    v.source_height,
                    tj.correlation_id
                FROM transcoding_jobs tj
                INNER JOIN videos v
                    ON v.id = tj.video_id
                INNER JOIN processing_jobs p
                    ON p.id = tj.processing_job_id
                WHERE tj.id = $3
                  AND tj.status = 'QUEUED'::transcoding_job_status
                  AND tj.rendition = $2::video_rendition_name
                  AND p.job_type = 'BASELINE'::processing_job_type
                  AND p.status = 'RUNNING'::processing_job_status
                  AND v.status = 'PROCESSING_BASELINE'::video_status
                FOR UPDATE SKIP LOCKED
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
                candidate.segment_index,
                candidate.source_segment_s3_key,
                candidate.source_segment_duration_seconds,
                candidate.attempt,
                candidate.source_width,
                candidate.source_height,
                candidate.correlation_id
            "#,
        )
        .bind(worker_id)
        .bind(rendition_name)
        .bind(transcoding_job_id)
        .fetch_optional(&mut *transaction)
        .await?;

        transaction.commit().await?;

        Ok(claimed_job)
    }

    async fn claim_additional_renditions_job(
        &self,
        transcoding_job_id: Uuid,
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
                    tj.segment_index,
                    tj.source_segment_s3_key,
                    tj.source_segment_duration_seconds,
                    tj.attempt,
                    v.source_width,
                    v.source_height,
                    tj.correlation_id
                FROM transcoding_jobs tj
                INNER JOIN videos v
                    ON v.id = tj.video_id
                INNER JOIN processing_jobs p
                    ON p.id = tj.processing_job_id
                WHERE tj.id = $3
                  AND tj.status = 'QUEUED'::transcoding_job_status
                  AND tj.rendition = $2::video_rendition_name
                  AND p.job_type = 'ADDITIONAL_RENDITIONS'::processing_job_type
                  AND p.status IN ('QUEUED'::processing_job_status, 'RUNNING'::processing_job_status)
                  AND v.status IN ('BASELINE_READY'::video_status, 'PROCESSING_FULL'::video_status)
                FOR UPDATE SKIP LOCKED
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
                candidate.segment_index,
                candidate.source_segment_s3_key,
                candidate.source_segment_duration_seconds,
                candidate.attempt,
                candidate.source_width,
                candidate.source_height,
                candidate.correlation_id
            "#,
        )
        .bind(worker_id)
        .bind(rendition_name)
        .bind(transcoding_job_id)
        .fetch_optional(&mut *transaction)
        .await?;

        let Some(claimed_job) = claimed_job else {
            transaction.commit().await?;
            return Ok(None);
        };

        sqlx::query(
            r#"
            UPDATE processing_jobs
            SET status = 'RUNNING'::processing_job_status,
                worker_id = $2,
                started_at = COALESCE(started_at, NOW()),
                finished_at = NULL,
                error = NULL,
                updated_at = NOW()
            WHERE id = $1
              AND job_type = 'ADDITIONAL_RENDITIONS'::processing_job_type
              AND status IN ('QUEUED'::processing_job_status, 'RUNNING'::processing_job_status)
            "#,
        )
        .bind(claimed_job.processing_job_id)
        .bind(worker_id)
        .execute(&mut *transaction)
        .await?;

        sqlx::query(
            r#"
            UPDATE videos
            SET status = 'PROCESSING_FULL'::video_status,
                updated_at = NOW()
            WHERE id = $1
              AND status = 'BASELINE_READY'::video_status
            "#,
        )
        .bind(claimed_job.video_id)
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;

        Ok(Some(claimed_job))
    }

    pub async fn list_queued_additional_transcoding_jobs(
        &self,
        video_id: Uuid,
    ) -> AppResult<Vec<QueuedTranscodingJobRecord>> {
        sqlx::query_as::<_, QueuedTranscodingJobRecord>(
            r#"
            SELECT
                tj.id AS transcoding_job_id,
                tj.processing_job_id,
                tj.video_id,
                tj.rendition::text AS rendition,
                tj.segment_index,
                tj.correlation_id,
                tj.attempt
            FROM transcoding_jobs tj
            INNER JOIN processing_jobs p
                ON p.id = tj.processing_job_id
            WHERE tj.video_id = $1
              AND tj.status = 'QUEUED'::transcoding_job_status
              AND p.job_type = 'ADDITIONAL_RENDITIONS'::processing_job_type
            ORDER BY tj.rendition ASC, tj.segment_index ASC, tj.created_at ASC
            "#,
        )
        .bind(video_id)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)
    }

    pub async fn begin_video_completion_session(
        &self,
        video_id: Uuid,
    ) -> AppResult<VideoCompletionSession> {
        let mut connection = self.pool.acquire().await?;

        sqlx::query("BEGIN")
            .execute(&mut *connection)
            .await
            .map_err(AppError::from)?;

        let session_result: AppResult<()> = async {
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
                .bind(video_id.to_string())
                .execute(&mut *connection)
                .await
                .map_err(AppError::from)?;

            sqlx::query("SELECT id FROM videos WHERE id = $1 FOR UPDATE")
                .bind(video_id)
                .execute(&mut *connection)
                .await
                .map_err(AppError::from)?;

            Ok(())
        }
        .await;

        if let Err(error) = session_result {
            let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
            return Err(error);
        }

        Ok(VideoCompletionSession { connection })
    }

    pub async fn commit_video_completion_session(
        &self,
        mut session: VideoCompletionSession,
    ) -> AppResult<()> {
        sqlx::query("COMMIT")
            .execute(&mut *session.connection)
            .await
            .map_err(AppError::from)?;

        Ok(())
    }

    pub async fn rollback_video_completion_session(
        &self,
        mut session: VideoCompletionSession,
    ) -> AppResult<()> {
        sqlx::query("ROLLBACK")
            .execute(&mut *session.connection)
            .await
            .map_err(AppError::from)?;

        Ok(())
    }

    pub async fn mark_rendition_processing(
        &self,
        video_id: Uuid,
        rendition_name: &str,
        codec: &str,
        container: &str,
        playlist_key: &str,
        output_width: i32,
        output_height: i32,
        target_video_bitrate_kbps: i32,
        target_audio_bitrate_kbps: i32,
    ) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO video_renditions (
                video_id,
                rendition,
                codec,
                container,
                playlist_key,
                output_width,
                output_height,
                target_video_bitrate_kbps,
                target_audio_bitrate_kbps,
                status,
                segment_count
            )
            VALUES (
                $1,
                $2::video_rendition_name,
                $3,
                $4,
                $5,
                $6,
                $7,
                $8,
                $9,
                'PROCESSING'::video_rendition_status,
                0
            )
            ON CONFLICT (video_id, rendition) DO UPDATE
            SET codec = EXCLUDED.codec,
                container = EXCLUDED.container,
                playlist_key = EXCLUDED.playlist_key,
                output_width = EXCLUDED.output_width,
                output_height = EXCLUDED.output_height,
                target_video_bitrate_kbps = EXCLUDED.target_video_bitrate_kbps,
                target_audio_bitrate_kbps = EXCLUDED.target_audio_bitrate_kbps,
                status = 'PROCESSING'::video_rendition_status,
                updated_at = NOW()
            "#,
        )
        .bind(video_id)
        .bind(rendition_name)
        .bind(codec)
        .bind(container)
        .bind(playlist_key)
        .bind(output_width)
        .bind(output_height)
        .bind(target_video_bitrate_kbps)
        .bind(target_audio_bitrate_kbps)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn mark_transcoding_segment_succeeded_in_session(
        &self,
        session: &mut VideoCompletionSession,
        transcoding_job_id: Uuid,
        output_segment_s3_key: &str,
    ) -> AppResult<()> {
        sqlx::query(
            r#"
            UPDATE transcoding_jobs
            SET status = 'SUCCEEDED'::transcoding_job_status,
                output_segment_s3_key = $2,
                finished_at = NOW(),
                error = NULL,
                updated_at = NOW()
            WHERE id = $1
              AND status = 'RUNNING'::transcoding_job_status
            "#,
        )
        .bind(transcoding_job_id)
        .bind(output_segment_s3_key)
        .execute(&mut *session.connection)
        .await?;

        Ok(())
    }

    pub async fn list_latest_segment_job_states_in_session(
        &self,
        session: &mut VideoCompletionSession,
        video_id: Uuid,
        rendition_name: &str,
    ) -> AppResult<Vec<LatestSegmentJobStateRecord>> {
        sqlx::query_as::<_, LatestSegmentJobStateRecord>(
            r#"
            WITH latest_attempts AS (
                SELECT DISTINCT ON (segment_index)
                    segment_index,
                    status::text AS status,
                    source_segment_duration_seconds,
                    output_segment_s3_key
                FROM transcoding_jobs
                WHERE video_id = $1
                  AND rendition = $2::video_rendition_name
                ORDER BY segment_index, attempt DESC
            )
            SELECT
                segment_index,
                status,
                source_segment_duration_seconds,
                output_segment_s3_key
            FROM latest_attempts
            ORDER BY segment_index ASC
            "#,
        )
        .bind(video_id)
        .bind(rendition_name)
        .fetch_all(&mut *session.connection)
        .await
        .map_err(AppError::from)
    }

    pub async fn list_ready_manifest_variants_in_session(
        &self,
        session: &mut VideoCompletionSession,
        video_id: Uuid,
    ) -> AppResult<Vec<ReadyManifestVariantRecord>> {
        sqlx::query_as::<_, ReadyManifestVariantRecord>(
            r#"
            SELECT
                rendition::text AS rendition,
                playlist_key,
                output_width,
                output_height,
                target_video_bitrate_kbps,
                target_audio_bitrate_kbps
            FROM video_renditions
            WHERE video_id = $1
              AND status = 'READY'::video_rendition_status
            "#,
        )
        .bind(video_id)
        .fetch_all(&mut *session.connection)
        .await
        .map_err(AppError::from)
    }

    pub async fn mark_rendition_ready_in_session(
        &self,
        session: &mut VideoCompletionSession,
        update: RenditionCompletionUpdate,
        video_progress: VideoProgressUpdate,
    ) -> AppResult<()> {
        sqlx::query(
            r#"
            UPDATE video_renditions
            SET codec = $3,
                container = $4,
                playlist_key = $5,
                output_width = $6,
                output_height = $7,
                target_video_bitrate_kbps = $8,
                target_audio_bitrate_kbps = $9,
                status = 'READY'::video_rendition_status,
                segment_count = $10,
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
        .bind(update.output_width)
        .bind(update.output_height)
        .bind(update.target_video_bitrate_kbps)
        .bind(update.target_audio_bitrate_kbps)
        .bind(update.segment_count)
        .execute(&mut *session.connection)
        .await?;

        match video_progress {
            VideoProgressUpdate::BaselineReady { manifest_s3_key } => {
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
                .execute(&mut *session.connection)
                .await?;

                self.mark_processing_job_succeeded_in_session(session, update.processing_job_id)
                    .await?;
            }
            VideoProgressUpdate::Ready { manifest_s3_key } => {
                sqlx::query(
                    r#"
                    UPDATE videos
                    SET status = 'READY'::video_status,
                        is_streamable = TRUE,
                        manifest_s3_key = $2,
                        baseline_ready_at = COALESCE(baseline_ready_at, NOW()),
                        ready_at = COALESCE(ready_at, NOW()),
                        error_code = NULL,
                        error_message = NULL,
                        updated_at = NOW()
                    WHERE id = $1
                      AND status IN (
                        'PROCESSING_BASELINE'::video_status,
                        'BASELINE_READY'::video_status,
                        'PROCESSING_FULL'::video_status
                      )
                    "#,
                )
                .bind(update.video_id)
                .bind(&manifest_s3_key)
                .execute(&mut *session.connection)
                .await?;

                self.mark_processing_job_succeeded_in_session(session, update.processing_job_id)
                    .await?;
            }
            VideoProgressUpdate::RenditionReadyOnly => {}
        }

        Ok(())
    }

    async fn mark_processing_job_succeeded_in_session(
        &self,
        session: &mut VideoCompletionSession,
        processing_job_id: Uuid,
    ) -> AppResult<()> {
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
        .bind(processing_job_id)
        .execute(&mut *session.connection)
        .await?;

        Ok(())
    }

    pub async fn mark_transcoding_failed(
        &self,
        transcoding_job_id: Uuid,
        processing_job_id: Uuid,
        video_id: Uuid,
        segment_index: i32,
        rendition_name: &str,
        source_segment_s3_key: &str,
        source_segment_duration_seconds: f64,
        attempt: i32,
        error_message: &str,
        max_transcoding_attempts: i32,
        is_baseline_rendition: bool,
        correlation_id: &str,
    ) -> AppResult<TranscodingFailureDisposition> {
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

        if attempt < max_transcoding_attempts {
            let retry_transcoding_job_id = Uuid::new_v4();
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
            .bind(retry_transcoding_job_id)
            .bind(processing_job_id)
            .bind(video_id)
            .bind(rendition_name)
            .bind(segment_index)
            .bind(source_segment_s3_key)
            .bind(source_segment_duration_seconds)
            .bind(attempt + 1)
            .bind(correlation_id)
            .execute(&mut *transaction)
            .await?;

            transaction.commit().await?;

            return Ok(TranscodingFailureDisposition::RetryQueued {
                retry_transcoding_job_id,
                next_attempt: attempt + 1,
            });
        }

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

        if is_baseline_rendition {
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
            .bind("This video could not be prepared for playback.")
            .execute(&mut *transaction)
            .await?;
        } else {
            sqlx::query(
                r#"
                UPDATE processing_jobs
                SET status = 'FAILED'::processing_job_status,
                    finished_at = NOW(),
                    error = $2,
                    updated_at = NOW()
                WHERE id = $1
                  AND status IN ('QUEUED'::processing_job_status, 'RUNNING'::processing_job_status)
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
                    is_streamable = TRUE,
                    error_code = 'ADDITIONAL_RENDITION_TRANSCODER_FAILED',
                    error_message = $2,
                    updated_at = NOW()
                WHERE id = $1
                  AND status IN (
                    'BASELINE_READY'::video_status,
                    'PROCESSING_FULL'::video_status,
                    'READY'::video_status
                  )
                "#,
            )
            .bind(video_id)
            .bind("Playback is available, but some higher quality options could not be finished.")
            .execute(&mut *transaction)
            .await?;
        }

        transaction.commit().await?;

        Ok(TranscodingFailureDisposition::Terminal)
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
    pub segment_index: i32,
    pub source_segment_s3_key: String,
    pub source_segment_duration_seconds: f64,
    pub attempt: i32,
    pub source_width: Option<i32>,
    pub source_height: Option<i32>,
    pub correlation_id: String,
}

#[derive(Debug)]
pub struct RenditionCompletionUpdate {
    pub processing_job_id: Uuid,
    pub video_id: Uuid,
    pub rendition_name: String,
    pub codec: String,
    pub container: String,
    pub playlist_key: String,
    pub output_width: i32,
    pub output_height: i32,
    pub target_video_bitrate_kbps: i32,
    pub target_audio_bitrate_kbps: i32,
    pub segment_count: i32,
}

pub struct VideoCompletionSession {
    connection: PoolConnection<Postgres>,
}

#[derive(Debug)]
pub enum VideoProgressUpdate {
    BaselineReady { manifest_s3_key: String },
    Ready { manifest_s3_key: String },
    RenditionReadyOnly,
}

#[derive(Debug, FromRow)]
pub struct ReadyManifestVariantRecord {
    pub rendition: String,
    pub playlist_key: String,
    pub output_width: Option<i32>,
    pub output_height: Option<i32>,
    pub target_video_bitrate_kbps: Option<i32>,
    pub target_audio_bitrate_kbps: Option<i32>,
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

#[derive(Debug, FromRow)]
pub struct LatestSegmentJobStateRecord {
    pub segment_index: i32,
    pub status: String,
    pub source_segment_duration_seconds: f64,
    pub output_segment_s3_key: Option<String>,
}

#[derive(Debug)]
pub enum TranscodingFailureDisposition {
    RetryQueued {
        retry_transcoding_job_id: Uuid,
        next_attempt: i32,
    },
    Terminal,
}
