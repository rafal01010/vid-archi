# Video Streaming Take-Home

This repository contains the implementation for a minimal private video streaming service built for the take-home exam described in [PROJECT_IMPLEMENTATION_PLAN.md](/Users/dave/LabBase/vid-archi/PROJECT_IMPLEMENTATION_PLAN.md).

The project is being developed with these constraints in mind:
- Rust for the backend API and background worker.
- Svelte for the frontend.
- AWS S3, SQS, CloudFront, EC2, and Postgres-backed metadata.
- Architecture quality and maintainable code are higher priority than feature breadth.

The authoritative requirement and design references are:
- [SYSTEM_DESIGN_PROJECT_REQUIREMENTS_GUIDELINES.md](/Users/dave/LabBase/vid-archi/references/SYSTEM_DESIGN_PROJECT_REQUIREMENTS_GUIDELINES.md)
- [youtube_system_design_reference.md](/Users/dave/LabBase/vid-archi/references/youtube_system_design_reference.md)
- [Youtube-problem-writeup.md](/Users/dave/LabBase/vid-archi/references/Youtube-problem-writeup.md)
- [rust_clean_code_guidelines.md](/Users/dave/LabBase/vid-archi/references/rust_clean_code_guidelines.md)
- [svelte_clean_code_guidelines.md](/Users/dave/LabBase/vid-archi/references/svelte_clean_code_guidelines.md)

## Repository Layout

```text
backend/    Rust API service
worker/     Rust background processing service
frontend/   Svelte frontend
docs/       Architecture, API, and operational documentation
config/     Shared policy/config source-of-truth files used by multiple services
references/ Provided exam requirements and design references
```

## Shared Policy Source Of Truth

Upload and transcoding constants are defined in [config/video_policy.json](/Users/dave/LabBase/vid-archi/config/video_policy.json). Later Rust and Svelte code should read from this policy file or mirror it directly with the same canonical names.

The current policy captures:
- maximum upload size: `1GB`
- allowed input MIME types: `video/mp4`, `video/quicktime`, `video/webm`, `video/x-msvideo`, `video/vnd.avi`
- baseline rendition profile: `360p` HLS with `H.264 + AAC`

## Local Development Bootstrap

1. Copy `.env.example` to `.env`.
2. Start local dependencies with `./scripts/local/start-infra.sh`.
3. Apply database migrations with `./scripts/local/db-migrate.sh`.
4. Run the backend API with `./backend/scripts/run-local-api.sh`.

The local stack is intentionally lightweight:
- Postgres for metadata and job state.
- MinIO as an S3-compatible local object store.
- No local queue container yet; SQS wiring will be added when the upload and worker flows are implemented.

## Local Run Scripts

The infrastructure and the backend upload API can now run locally. The worker and frontend are still pending.

Available scripts:
- `./scripts/local/prepare-env.sh`
- `./scripts/local/start-infra.sh`
- `./scripts/local/status-infra.sh`
- `./scripts/local/logs-infra.sh`
- `./scripts/local/stop-infra.sh`
- `./scripts/local/db-migrate.sh`
- `./scripts/local/db-backup.sh`
- `./scripts/local/db-restore.sh`
- `./backend/scripts/run-local-api.sh`
- `./backend/scripts/deploy-backend.sh`

Typical local flow:

```bash
./scripts/local/start-infra.sh
./scripts/local/status-infra.sh
./scripts/local/db-migrate.sh
./backend/scripts/run-local-api.sh
```

macOS runtime note:
- If Docker Desktop is already running, the scripts will use it.
- If Docker Desktop is not running and `colima` is installed, `start-infra.sh` will try to start Colima automatically.
- If neither runtime is available, the scripts will print the exact next step instead of failing with the raw Docker socket error.

What "running locally" means right now:
- Postgres container is up.
- MinIO container is up.
- Local buckets for uploads and processed artifacts are created.
- Axum upload API is available on `http://127.0.0.1:8080`.

What is not available yet:
- worker processing pipeline
- Svelte upload/playback pages

## Implemented Upload API

Checklist step `3a/3b/3c` is now implemented in the Rust backend:
- `POST /api/videos`
- `POST /api/videos/{videoId}/parts/sign`
- `POST /api/videos/{videoId}/complete`

Current flow:
1. `POST /api/videos` validates the upload against [config/video_policy.json](/Users/dave/LabBase/vid-archi/config/video_policy.json), generates a deterministic `public_id`, creates the `videos` row, opens an `upload_sessions` row, and initializes the S3 multipart upload.
2. `POST /api/videos/{videoId}/parts/sign` returns presigned `PUT` URLs for the requested part numbers and moves the video from `INITIATED` to `UPLOADING` on first use.
3. `POST /api/videos/{videoId}/complete` completes the multipart upload in S3, marks the video as `UPLOADED`, and inserts the baseline processing job in `processing_jobs`.

The backend reads the shared policy file for:
- max upload size
- allowed MIME types
- allowed file extensions

Example create-upload request:

```json
{
  "filename": "demo.mp4",
  "contentType": "video/mp4",
  "sizeBytes": 52428800,
  "title": "Demo upload"
}
```

The `publicId` share slug is deterministic. It is built from a sanitized title-or-filename slug plus a lowercase base32 token derived from the internal `video_id`. That keeps share links stable, readable, and unique without adding a separate random slug generator.

More detail is in [docs/api.md](/Users/dave/LabBase/vid-archi/docs/api.md) and [backend/README.md](/Users/dave/LabBase/vid-archi/backend/README.md).

## Minimal Testing

The backend now includes Rust unit tests for the main generated upload-path logic.

Rust unit tests cover the main generated logic:
- deterministic `public_id` generation
- upload policy validation
- filename sanitizing and multipart normalization helpers

Run them with:

```bash
cargo test --manifest-path backend/Cargo.toml
```

## Deploy-Time Test Gate

The deployment helper can run unit tests before packaging the backend binary:

```bash
./backend/scripts/deploy-backend.sh --run-tests
```

That is a standard CI/CD pattern. Unit tests are the usual pre-build or pre-deploy gate.

## Postgres Persistence

The current Docker Compose setup stores Postgres data in a named Docker volume defined in [docker-compose.yml](/Users/dave/LabBase/vid-archi/docker-compose.yml#L52).

That means:
- `docker compose down` removes the container but keeps the data volume
- restarting or recreating the container on the same machine keeps the database data
- data is lost only if you remove the volume explicitly, such as `docker compose down -v`, `docker volume rm ...`, or if the host disk is lost

Backup and restore helpers:

```bash
./scripts/local/db-backup.sh
./scripts/local/db-restore.sh backups/postgres/<backup-file>.sql
```

For STG on EC2, the same persistence rule applies if the Postgres container uses a named volume or bind mount on the instance. Container restarts do not delete the database by themselves, but losing the EC2 disk or instance without a backup will.
