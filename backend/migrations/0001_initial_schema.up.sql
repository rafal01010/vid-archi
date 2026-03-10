BEGIN;

CREATE EXTENSION IF NOT EXISTS pgcrypto;

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM pg_type
    WHERE typname = 'video_status'
  ) THEN
    CREATE TYPE video_status AS ENUM (
      'INITIATED',
      'UPLOADING',
      'UPLOADED',
      'PROCESSING_BASELINE',
      'BASELINE_READY',
      'PROCESSING_FULL',
      'READY',
      'FAILED'
    );
  END IF;
END
$$;

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM pg_type
    WHERE typname = 'transcoding_job_status'
  ) THEN
    CREATE TYPE transcoding_job_status AS ENUM (
      'QUEUED',
      'RUNNING',
      'SUCCEEDED',
      'FAILED'
    );
  END IF;
END
$$;

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM pg_type
    WHERE typname = 'upload_session_status'
  ) THEN
    CREATE TYPE upload_session_status AS ENUM (
      'OPEN',
      'COMPLETED',
      'ABORTED',
      'EXPIRED'
    );
  END IF;
END
$$;

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM pg_type
    WHERE typname = 'video_rendition_name'
  ) THEN
    CREATE TYPE video_rendition_name AS ENUM (
      '360p',
      '480p',
      '720p',
      '1080p',
      '1440p',
      '2160p'
    );
  END IF;
END
$$;

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM pg_type
    WHERE typname = 'video_rendition_status'
  ) THEN
    CREATE TYPE video_rendition_status AS ENUM (
      'PROCESSING',
      'READY',
      'FAILED'
    );
  END IF;
END
$$;

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM pg_type
    WHERE typname = 'processing_job_type'
  ) THEN
    CREATE TYPE processing_job_type AS ENUM (
      'BASELINE',
      'INTERMEDIATE_RENDITIONS',
      'ADDITIONAL_RENDITIONS'
    );
  END IF;
END
$$;

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM pg_type
    WHERE typname = 'processing_job_status'
  ) THEN
    CREATE TYPE processing_job_status AS ENUM (
      'QUEUED',
      'RUNNING',
      'SUCCEEDED',
      'FAILED'
    );
  END IF;
END
$$;

CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
  NEW.updated_at = NOW();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TABLE IF NOT EXISTS videos (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  public_id TEXT NOT NULL UNIQUE,
  delete_code_salt TEXT NOT NULL,
  delete_code_hash TEXT NOT NULL,
  title TEXT,
  original_filename TEXT NOT NULL,
  content_type TEXT NOT NULL,
  size_bytes BIGINT NOT NULL,
  source_s3_key TEXT NOT NULL,
  source_width INTEGER,
  source_height INTEGER,
  manifest_s3_key TEXT,
  status video_status NOT NULL DEFAULT 'INITIATED',
  is_streamable BOOLEAN NOT NULL DEFAULT FALSE,
  baseline_ready_at TIMESTAMPTZ,
  ready_at TIMESTAMPTZ,
  error_code TEXT,
  error_message TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  CONSTRAINT videos_size_bytes_valid
    CHECK (size_bytes > 0 AND size_bytes <= 1073741824),
  CONSTRAINT videos_content_type_allowed
    CHECK (
      content_type IN (
        'video/mp4',
        'video/quicktime',
        'video/webm',
        'video/x-msvideo',
        'video/vnd.avi'
      )
    ),
  CONSTRAINT videos_source_dimensions_valid
    CHECK (
      (
        source_width IS NULL
        AND source_height IS NULL
      )
      OR (
        source_width IS NOT NULL
        AND source_height IS NOT NULL
        AND source_width > 0
        AND source_height > 0
      )
    ),
  CONSTRAINT videos_streamable_requires_manifest
    CHECK (NOT is_streamable OR manifest_s3_key IS NOT NULL),
  CONSTRAINT videos_streamable_requires_streamable_status
    CHECK (
      NOT is_streamable
      OR status IN ('BASELINE_READY', 'PROCESSING_FULL', 'READY', 'FAILED')
    ),
  CONSTRAINT videos_streamable_status_requires_baseline_artifacts
    CHECK (
      (
        status IN ('BASELINE_READY', 'PROCESSING_FULL', 'READY')
        AND is_streamable = TRUE
        AND manifest_s3_key IS NOT NULL
        AND baseline_ready_at IS NOT NULL
      )
      OR (
        status = 'FAILED'
        AND (
          is_streamable = FALSE
          OR (
            manifest_s3_key IS NOT NULL
            AND baseline_ready_at IS NOT NULL
          )
        )
      )
      OR status NOT IN ('BASELINE_READY', 'PROCESSING_FULL', 'READY', 'FAILED')
    ),
  CONSTRAINT videos_ready_requires_ready_timestamp
    CHECK (
      status <> 'READY'
      OR ready_at IS NOT NULL
    )
);

CREATE INDEX IF NOT EXISTS videos_status_idx
  ON videos (status);

CREATE INDEX IF NOT EXISTS videos_created_at_idx
  ON videos (created_at DESC);

CREATE INDEX IF NOT EXISTS videos_streamable_idx
  ON videos (is_streamable)
  WHERE is_streamable = TRUE;

CREATE TABLE IF NOT EXISTS upload_sessions (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  video_id UUID NOT NULL REFERENCES videos (id) ON DELETE CASCADE,
  s3_upload_id TEXT NOT NULL UNIQUE,
  part_size_bytes BIGINT NOT NULL,
  expires_at TIMESTAMPTZ NOT NULL,
  status upload_session_status NOT NULL DEFAULT 'OPEN',
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  CONSTRAINT upload_sessions_part_size_bytes_valid
    CHECK (part_size_bytes > 0)
);

CREATE INDEX IF NOT EXISTS upload_sessions_video_id_idx
  ON upload_sessions (video_id);

CREATE INDEX IF NOT EXISTS upload_sessions_status_idx
  ON upload_sessions (status);

CREATE INDEX IF NOT EXISTS upload_sessions_expires_at_idx
  ON upload_sessions (expires_at);

CREATE UNIQUE INDEX IF NOT EXISTS upload_sessions_one_open_session_per_video_idx
  ON upload_sessions (video_id)
  WHERE status = 'OPEN';

CREATE TABLE IF NOT EXISTS video_renditions (
  video_id UUID NOT NULL REFERENCES videos (id) ON DELETE CASCADE,
  rendition video_rendition_name NOT NULL,
  codec TEXT NOT NULL,
  container TEXT NOT NULL,
  playlist_key TEXT NOT NULL,
  output_width INTEGER,
  output_height INTEGER,
  target_video_bitrate_kbps INTEGER,
  target_audio_bitrate_kbps INTEGER,
  status video_rendition_status NOT NULL DEFAULT 'PROCESSING',
  segment_count INTEGER NOT NULL DEFAULT 0,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (video_id, rendition),
  CONSTRAINT video_renditions_output_dimensions_valid
    CHECK (
      (
        output_width IS NULL
        AND output_height IS NULL
      )
      OR (
        output_width IS NOT NULL
        AND output_height IS NOT NULL
        AND output_width > 0
        AND output_height > 0
      )
    ),
  CONSTRAINT video_renditions_target_bitrates_valid
    CHECK (
      (
        target_video_bitrate_kbps IS NULL
        AND target_audio_bitrate_kbps IS NULL
      )
      OR (
        target_video_bitrate_kbps IS NOT NULL
        AND target_audio_bitrate_kbps IS NOT NULL
        AND target_video_bitrate_kbps > 0
        AND target_audio_bitrate_kbps > 0
      )
    ),
  CONSTRAINT video_renditions_segment_count_valid
    CHECK (segment_count >= 0)
);

CREATE INDEX IF NOT EXISTS video_renditions_status_idx
  ON video_renditions (status);

CREATE TABLE IF NOT EXISTS processing_jobs (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  video_id UUID NOT NULL REFERENCES videos (id) ON DELETE CASCADE,
  job_type processing_job_type NOT NULL,
  attempt INTEGER NOT NULL,
  status processing_job_status NOT NULL DEFAULT 'QUEUED',
  correlation_id TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  worker_id TEXT,
  started_at TIMESTAMPTZ,
  finished_at TIMESTAMPTZ,
  error TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  CONSTRAINT processing_jobs_attempt_valid
    CHECK (attempt > 0),
  CONSTRAINT processing_jobs_status_timestamps_valid
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
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS processing_jobs_video_id_job_type_attempt_idx
  ON processing_jobs (video_id, job_type, attempt);

CREATE INDEX IF NOT EXISTS processing_jobs_status_idx
  ON processing_jobs (status);

CREATE INDEX IF NOT EXISTS processing_jobs_created_at_idx
  ON processing_jobs (created_at DESC);

CREATE INDEX IF NOT EXISTS processing_jobs_correlation_id_idx
  ON processing_jobs (correlation_id);

CREATE TABLE IF NOT EXISTS transcoding_jobs (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  processing_job_id UUID NOT NULL REFERENCES processing_jobs (id) ON DELETE CASCADE,
  video_id UUID NOT NULL REFERENCES videos (id) ON DELETE CASCADE,
  rendition video_rendition_name NOT NULL,
  segment_index INTEGER NOT NULL,
  source_segment_s3_key TEXT NOT NULL,
  source_segment_duration_seconds DOUBLE PRECISION NOT NULL,
  attempt INTEGER NOT NULL DEFAULT 1,
  status transcoding_job_status NOT NULL DEFAULT 'QUEUED',
  correlation_id TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  worker_id TEXT,
  started_at TIMESTAMPTZ,
  finished_at TIMESTAMPTZ,
  output_segment_s3_key TEXT,
  error TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  CONSTRAINT transcoding_jobs_attempt_valid
    CHECK (attempt > 0),
  CONSTRAINT transcoding_jobs_segment_index_valid
    CHECK (segment_index >= 0),
  CONSTRAINT transcoding_jobs_source_segment_duration_valid
    CHECK (source_segment_duration_seconds > 0),
  CONSTRAINT transcoding_jobs_status_timestamps_valid
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
    ),
  CONSTRAINT transcoding_jobs_succeeded_requires_output_segment
    CHECK (
      status <> 'SUCCEEDED'
      OR output_segment_s3_key IS NOT NULL
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS transcoding_jobs_video_id_rendition_segment_attempt_idx
  ON transcoding_jobs (video_id, rendition, segment_index, attempt);

CREATE UNIQUE INDEX IF NOT EXISTS transcoding_jobs_processing_job_id_rendition_segment_attempt_idx
  ON transcoding_jobs (processing_job_id, rendition, segment_index, attempt);

CREATE INDEX IF NOT EXISTS transcoding_jobs_status_idx
  ON transcoding_jobs (status);

CREATE INDEX IF NOT EXISTS transcoding_jobs_rendition_status_idx
  ON transcoding_jobs (rendition, status, segment_index, created_at);

CREATE INDEX IF NOT EXISTS transcoding_jobs_video_id_rendition_segment_idx
  ON transcoding_jobs (video_id, rendition, segment_index, attempt DESC);

CREATE INDEX IF NOT EXISTS transcoding_jobs_correlation_id_idx
  ON transcoding_jobs (correlation_id);

DROP TRIGGER IF EXISTS videos_set_updated_at ON videos;
CREATE TRIGGER videos_set_updated_at
BEFORE UPDATE ON videos
FOR EACH ROW
EXECUTE FUNCTION set_updated_at();

DROP TRIGGER IF EXISTS upload_sessions_set_updated_at ON upload_sessions;
CREATE TRIGGER upload_sessions_set_updated_at
BEFORE UPDATE ON upload_sessions
FOR EACH ROW
EXECUTE FUNCTION set_updated_at();

DROP TRIGGER IF EXISTS video_renditions_set_updated_at ON video_renditions;
CREATE TRIGGER video_renditions_set_updated_at
BEFORE UPDATE ON video_renditions
FOR EACH ROW
EXECUTE FUNCTION set_updated_at();

DROP TRIGGER IF EXISTS processing_jobs_set_updated_at ON processing_jobs;
CREATE TRIGGER processing_jobs_set_updated_at
BEFORE UPDATE ON processing_jobs
FOR EACH ROW
EXECUTE FUNCTION set_updated_at();

DROP TRIGGER IF EXISTS transcoding_jobs_set_updated_at ON transcoding_jobs;
CREATE TRIGGER transcoding_jobs_set_updated_at
BEFORE UPDATE ON transcoding_jobs
FOR EACH ROW
EXECUTE FUNCTION set_updated_at();

COMMIT;
