BEGIN;

ALTER TABLE videos
  DROP CONSTRAINT IF EXISTS videos_streamable_requires_streamable_status,
  DROP CONSTRAINT IF EXISTS videos_baseline_ready_requires_timestamp,
  DROP CONSTRAINT IF EXISTS videos_ready_requires_timestamps;

ALTER TABLE videos
  ADD CONSTRAINT videos_streamable_requires_streamable_status
    CHECK (
      NOT is_streamable
      OR status IN ('BASELINE_READY', 'PROCESSING_FULL', 'READY')
    ),
  ADD CONSTRAINT videos_streamable_status_requires_baseline_artifacts
    CHECK (
      status NOT IN ('BASELINE_READY', 'PROCESSING_FULL', 'READY')
      OR (
        is_streamable = TRUE
        AND manifest_s3_key IS NOT NULL
        AND baseline_ready_at IS NOT NULL
      )
    ),
  ADD CONSTRAINT videos_ready_requires_ready_timestamp
    CHECK (
      status <> 'READY'
      OR ready_at IS NOT NULL
    );

CREATE UNIQUE INDEX IF NOT EXISTS upload_sessions_one_open_session_per_video_idx
  ON upload_sessions (video_id)
  WHERE status = 'OPEN';

ALTER TABLE processing_jobs
  DROP CONSTRAINT IF EXISTS processing_jobs_finished_after_started;

ALTER TABLE processing_jobs
  ADD CONSTRAINT processing_jobs_status_timestamps_valid
    CHECK (
      (
        status = 'QUEUED'
        AND started_at IS NULL
        AND finished_at IS NULL
      )
      OR (
        status = 'RUNNING'
        AND started_at IS NOT NULL
        AND finished_at IS NULL
      )
      OR (
        status IN ('SUCCEEDED', 'FAILED')
        AND started_at IS NOT NULL
        AND finished_at IS NOT NULL
        AND finished_at >= started_at
      )
    );

COMMIT;
