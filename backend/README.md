# Backend

This directory now contains the Rust API service responsible for:
- creating video records and upload sessions
- issuing presigned multipart upload instructions
- generating deterministic public share links
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
- `POST /api/videos/{videoId}/parts/sign`
  Returns presigned `PUT` URLs for explicit multipart part numbers.
- `POST /api/videos/{videoId}/complete`
  Completes the multipart upload, transitions the video to `UPLOADED`, and inserts a `BASELINE` job into `processing_jobs`.
- `GET /healthz`
  Lightweight process health check.

## Local Run

Prerequisites:
- Rust toolchain installed locally (`cargo` must exist)
- local infra started with `./scripts/local/start-infra.sh`
- migrations applied with `./scripts/local/db-migrate.sh`

Run the API:

```bash
./backend/scripts/run-local-api.sh
```

The service loads `.env`, reads the shared upload policy from `VIDEO_POLICY_FILE`, connects to Postgres via `DATABASE_URL`, and targets MinIO automatically when `LOCAL_S3_ENDPOINT` is set.

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
