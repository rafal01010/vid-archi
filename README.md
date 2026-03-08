# Video Streaming project

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
- adaptive rendition ladder: `360p`, `480p`, `720p`, `1080p`, `1440p`, `2160p`
- no upscaling: renditions must not exceed the original source resolution

## Local Development Bootstrap

1. Copy `.env.example` to `.env`.
2. First-time local setup: run `./scripts/local/deploy-local-lite.sh --bootstrap-db`.
3. Normal local redeploys: run `./scripts/local/deploy-local-lite.sh`.
4. Open the frontend at `http://127.0.0.1:4173`.
5. Optional: stop the packaged local stack with `./scripts/local/stop-local-lite.sh`.

The local stack is intentionally lightweight:
- Postgres for metadata and job state.
- MinIO as an S3-compatible local object store.
- No local queue container yet; SQS wiring will be added when the upload and worker flows are implemented.

## Local Run Scripts

The infrastructure, backend upload API, and Svelte homepage/upload flow are now scaffolded locally behind one packaged local deploy path. The worker is still pending.

Available scripts:
- `./backend/scripts/deploy-backend.sh`
- `./frontend/scripts/deploy-frontend.sh`
- `./frontend/scripts/run-stg-frontend.sh`
- `./backend/scripts/run-stg-api.sh`
- `./scripts/local/deploy-local-lite.sh`
- `./scripts/local/start-local-lite.sh`
- `./scripts/local/stop-local-lite.sh`
- `./scripts/stg/deploy-stg-lite.sh`
- `./scripts/stg/start-stg-lite.sh`
- `./.env.local`

Typical local flow:

```bash
./scripts/local/deploy-local-lite.sh --bootstrap-db
```

Later local redeploys:

```bash
./scripts/local/deploy-local-lite.sh
```

Reset the local packaged Postgres database completely:

```bash
./scripts/local/deploy-local-lite.sh --reset-db
```

Restart the packaged local services without rebuilding:

```bash
./scripts/local/start-local-lite.sh
```

Stop the packaged local services:

```bash
./scripts/local/stop-local-lite.sh
```

Apply only new database migrations:

```bash
./scripts/local/deploy-local-lite.sh --migrate-db
```

What "running locally" means right now:
- Local packaged Postgres container is up.
- MinIO container is up.
- Local buckets for uploads and processed artifacts are created.
- Axum upload API is available on `http://127.0.0.1:8080`.
- Svelte homepage is available on `http://127.0.0.1:4173`.

What is not available yet:
- worker processing pipeline
- HLS playback integration

## Implemented API And Frontend Flow

Checklist step `3a/3b/3c` is implemented in the Rust backend, and step `4a/4b/4c` now has a frontend implementation:
- `POST /api/videos`
- `GET /api/videos`
- `GET /api/videos/{publicId}`
- `POST /api/videos/{videoId}/parts/sign`
- `POST /api/videos/{videoId}/complete`

Current flow:
1. `POST /api/videos` validates the upload against [config/video_policy.json](/Users/dave/LabBase/vid-archi/config/video_policy.json), generates a deterministic `public_id`, creates the `videos` row, opens an `upload_sessions` row, and initializes the S3 multipart upload.
2. `POST /api/videos/{videoId}/parts/sign` returns presigned `PUT` URLs for the requested part numbers and moves the video from `INITIATED` to `UPLOADING` on first use.
3. `POST /api/videos/{videoId}/complete` completes the multipart upload in S3, marks the video as `UPLOADED`, and inserts the baseline processing job in `processing_jobs`.
4. `GET /api/videos` returns the homepage library, sorted newest first with pagination metadata.
5. `GET /api/videos/{publicId}` resolves the share-page route and returns the current lifecycle state for that public video ID.

Frontend behavior:
- `/` is now both the upload page and the recent-upload homepage.
- The homepage shows the newest 10 videos first and exposes next/previous pagination.
- Multipart parts are uploaded directly from the browser to S3/MinIO using presigned `PUT` URLs.
- After completion, the page displays the share URL and refreshes the homepage library.
- `/v/[publicId]` now resolves the public video ID and shows current status; HLS playback wiring remains a later checklist item.
- The frontend theme is monochrome: black, white, and gray only.

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

The packaged local deployment flow stores Postgres data in a named Docker volume attached to the local packaged Postgres container (`local-lite-postgres` by default).

That means:
- stopping the container does not remove the data volume
- restarting or recreating the container on the same machine keeps the database data when the named volume is reused
- data is lost only if you remove the named volume explicitly, such as `docker volume rm local-lite-postgres-data`, or if the host disk is lost

For STG on EC2, the same persistence rule applies if the Postgres container uses a named volume or bind mount on the instance. Container restarts do not delete the database by themselves, but losing the EC2 disk or instance without a backup will.
