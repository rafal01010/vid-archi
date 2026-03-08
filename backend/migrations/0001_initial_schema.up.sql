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
      'ENHANCE'
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
      OR status IN ('BASELINE_READY', 'READY')
    ),
  CONSTRAINT videos_baseline_ready_requires_timestamp
    CHECK (
      status <> 'BASELINE_READY'
      OR baseline_ready_at IS NOT NULL
    ),
  CONSTRAINT videos_ready_requires_timestamps
    CHECK (
      status <> 'READY'
      OR (
        baseline_ready_at IS NOT NULL
        AND ready_at IS NOT NULL
        AND manifest_s3_key IS NOT NULL
        AND is_streamable = TRUE
      )
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

CREATE TABLE IF NOT EXISTS upload_parts (
  session_id UUID NOT NULL REFERENCES upload_sessions (id) ON DELETE CASCADE,
  part_number INTEGER NOT NULL,
  etag TEXT NOT NULL,
  size_bytes BIGINT NOT NULL,
  uploaded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (session_id, part_number),
  CONSTRAINT upload_parts_part_number_valid
    CHECK (part_number > 0 AND part_number <= 10000),
  CONSTRAINT upload_parts_size_bytes_valid
    CHECK (size_bytes > 0)
);

CREATE INDEX IF NOT EXISTS upload_parts_uploaded_at_idx
  ON upload_parts (uploaded_at DESC);

CREATE TABLE IF NOT EXISTS video_renditions (
  video_id UUID NOT NULL REFERENCES videos (id) ON DELETE CASCADE,
  rendition video_rendition_name NOT NULL,
  codec TEXT NOT NULL,
  container TEXT NOT NULL,
  playlist_key TEXT NOT NULL,
  status video_rendition_status NOT NULL DEFAULT 'PROCESSING',
  segment_count INTEGER NOT NULL DEFAULT 0,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (video_id, rendition),
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
  worker_id TEXT,
  started_at TIMESTAMPTZ,
  finished_at TIMESTAMPTZ,
  error TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  CONSTRAINT processing_jobs_attempt_valid
    CHECK (attempt > 0),
  CONSTRAINT processing_jobs_finished_after_started
    CHECK (
      finished_at IS NULL
      OR started_at IS NULL
      OR finished_at >= started_at
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS processing_jobs_video_id_job_type_attempt_idx
  ON processing_jobs (video_id, job_type, attempt);

CREATE INDEX IF NOT EXISTS processing_jobs_status_idx
  ON processing_jobs (status);

CREATE INDEX IF NOT EXISTS processing_jobs_created_at_idx
  ON processing_jobs (created_at DESC);

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

COMMIT;
