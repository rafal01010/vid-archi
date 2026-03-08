BEGIN;

DROP TRIGGER IF EXISTS processing_jobs_set_updated_at ON processing_jobs;
DROP TRIGGER IF EXISTS video_renditions_set_updated_at ON video_renditions;
DROP TRIGGER IF EXISTS upload_sessions_set_updated_at ON upload_sessions;
DROP TRIGGER IF EXISTS videos_set_updated_at ON videos;

DROP TABLE IF EXISTS processing_jobs;
DROP TABLE IF EXISTS video_renditions;
DROP TABLE IF EXISTS upload_parts;
DROP TABLE IF EXISTS upload_sessions;
DROP TABLE IF EXISTS videos;

DROP FUNCTION IF EXISTS set_updated_at();

DROP TYPE IF EXISTS processing_job_status;
DROP TYPE IF EXISTS processing_job_type;
DROP TYPE IF EXISTS video_rendition_status;
DROP TYPE IF EXISTS video_rendition_name;
DROP TYPE IF EXISTS upload_session_status;
DROP TYPE IF EXISTS video_status;

COMMIT;
