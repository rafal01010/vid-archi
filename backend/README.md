# Backend

This directory now contains the Rust API service responsible for:
- creating video records and upload sessions
- issuing presigned multipart upload instructions
- generating deterministic public share links
- listing recent videos for the homepage
- resolving public share IDs for the share page
- queuing the baseline processing job after upload completion

Current internal layout:

```text
src/
  application/     use cases and orchestration
  domain/          upload policy and public-id generation
  http/            Axum handlers, request DTOs, response DTOs
  infrastructure/  Postgres, S3-compatible storage, config
migrations/        SQL migrations
scripts/           local run helpers
```

Implementation rules:
- Keep handlers thin.
- Model lifecycle state explicitly with enums.
- Use the names from [config/video_policy.json](/Users/dave/LabBase/vid-archi/config/video_policy.json) consistently.

## Implemented Endpoints

- `POST /api/videos`
  Creates the `videos` row, creates the `upload_sessions` row, starts the S3 multipart upload, and returns the session metadata the frontend needs.
- `GET /api/videos`
  Returns the recent-video homepage library ordered by newest upload first, with pagination metadata.
- `GET /api/videos/{publicId}`
  Resolves the public share ID and returns the current lifecycle state and route metadata for the share page.
- `POST /api/videos/{videoId}/parts/sign`
  Returns presigned `PUT` URLs for explicit multipart part numbers.
- `POST /api/videos/{videoId}/complete`
  Completes the multipart upload, transitions the video to `UPLOADED`, and inserts a `BASELINE` job into `processing_jobs`.
- `GET /healthz`
  Lightweight process health check.

## Local Run

Prerequisites:
- Rust toolchain installed locally (`cargo` must exist)
- packaged local stack started with `./scripts/local/deploy-local-lite.sh`

Preferred local command:

```bash
./scripts/local/deploy-local-lite.sh --bootstrap-db
```

The first-time deploy wrapper starts MinIO, provisions the local packaged Postgres container, applies pending migrations, packages the backend and frontend, and launches the services in the background. Later redeploys should use `./scripts/local/deploy-local-lite.sh` without `--bootstrap-db`. The API then loads `.env.local`, reads the shared upload policy from `VIDEO_POLICY_FILE`, connects to Postgres via `STG_DATABASE_URL`, and targets MinIO automatically when `LOCAL_S3_ENDPOINT` is set.

## Testing

Rust unit tests cover the main generated logic in:
- `src/domain/public_id.rs`
- `src/domain/video_policy.rs`
- `src/application/upload_service.rs`

Run them with:

```bash
cargo test --manifest-path backend/Cargo.toml
```

## Deploy Helper

`./backend/scripts/deploy-backend.sh` builds a release artifact bundle and can optionally fail fast on unit tests before packaging:

```bash
./backend/scripts/deploy-backend.sh --run-tests
```

This is aligned with normal CI/CD practice:
- unit tests are standard pre-build or pre-deploy gates

## Endpoint Semantics

`POST /api/videos`:
- validates `filename`, `contentType`, and `sizeBytes`
- normalizes the incoming filename so browser path fragments are not persisted
- generates `public_id` deterministically from the title or filename plus the internal UUID-derived token
- initializes the multipart upload in S3 using object key `videos/{video_id}/source/original`

`POST /api/videos/{videoId}/parts/sign`:
- requires the caller to send the `uploadSessionId`
- validates that the session is still `OPEN` and not expired
- signs only the requested part numbers, which keeps the frontend in control of retry behavior

`POST /api/videos/{videoId}/complete`:
- validates the final part list and ETags
- asks S3 to stitch the uploaded parts into the source object
- updates the database state to `UPLOADED`
- inserts the initial `BASELINE` processing job that the worker will pick up in step `5`

`GET /api/videos`:
- uses `created_at DESC` so the homepage shows the latest uploaded videos first
- defaults cleanly to page `1` and page size `10`
- returns `page`, `pageSize`, `totalCount`, `totalPages`, `hasPreviousPage`, and `hasNextPage`
- returns only metadata and route info, never the video bytes themselves

`GET /api/videos/{publicId}`:
- treats the public share ID as the stable lookup key for the share page
- returns lifecycle state immediately, even before streaming playback is implemented
- leaves `manifestUrl` empty until the worker and playback steps publish streamable artifacts

Resolution-ladder note:
- the shared policy now defines `360p`, `480p`, `720p`, `1080p`, `1440p`, and `2160p`
- no upload API change is required for this
- the worker should persist source dimensions and only generate renditions at or below the original resolution
