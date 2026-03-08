BEGIN;

ALTER TABLE processing_jobs
  DROP CONSTRAINT IF EXISTS processing_jobs_status_timestamps_valid;

ALTER TABLE processing_jobs
  ADD CONSTRAINT processing_jobs_finished_after_started
    CHECK (
      finished_at IS NULL
      OR started_at IS NULL
      OR finished_at >= started_at
    );

DROP INDEX IF EXISTS upload_sessions_one_open_session_per_video_idx;

ALTER TABLE videos
  DROP CONSTRAINT IF EXISTS videos_streamable_requires_streamable_status,
  DROP CONSTRAINT IF EXISTS videos_streamable_status_requires_baseline_artifacts,
  DROP CONSTRAINT IF EXISTS videos_ready_requires_ready_timestamp;

ALTER TABLE videos
  ADD CONSTRAINT videos_streamable_requires_streamable_status
    CHECK (
      NOT is_streamable
      OR status IN ('BASELINE_READY', 'READY')
    ),
  ADD CONSTRAINT videos_baseline_ready_requires_timestamp
    CHECK (
      status <> 'BASELINE_READY'
      OR baseline_ready_at IS NOT NULL
    ),
  ADD CONSTRAINT videos_ready_requires_timestamps
    CHECK (
      status <> 'READY'
      OR (
        baseline_ready_at IS NOT NULL
        AND ready_at IS NOT NULL
        AND manifest_s3_key IS NOT NULL
        AND is_streamable = TRUE
      )
    );

COMMIT;
