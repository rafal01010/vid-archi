# Backend

This directory contains the Rust API service responsible for:
- creating video records and upload sessions
- hashing and verifying upload-time delete codes
- issuing presigned multipart upload instructions
- generating deterministic public share links
- listing recent videos for the homepage
- resolving public share IDs for the share page
- deleting source and processed assets by public link + delete code
- inserting the parent baseline processing job after upload completion

## Layout

```text
src/
  application/     use cases and orchestration
  domain/          upload policy and public-id generation
  http/            Axum handlers, request DTOs, response DTOs
  infrastructure/  Postgres, S3-compatible storage, config
migrations/        SQL migrations
scripts/           deploy/run helpers
```

## Implemented Endpoints

- `POST /api/videos`
- `GET /api/videos`
- `GET /api/videos/{publicId}`
- `DELETE /api/videos/{publicId}`
- `GET /api/videos/{publicId}/playback`
- `POST /api/videos/{videoId}/parts/sign`
- `POST /api/videos/{videoId}/complete`
- `GET /healthz`

## Endpoint Semantics

`POST /api/videos`:
- validates `filename`, `contentType`, and `sizeBytes`
- requires `deleteCode` and stores only a salted hash in Postgres
- generates a deterministic `public_id`
- initializes the multipart upload in object storage using `videos/{video_id}/source/original`

`POST /api/videos/{videoId}/parts/sign`:
- validates the upload session
- signs only the requested part numbers
- moves the video to `UPLOADING` on first valid use

`POST /api/videos/{videoId}/complete`:
- validates the final multipart part list
- completes the source object upload
- updates the video to `UPLOADED`
- inserts the initial `BASELINE` row into `processing_jobs`

`GET /api/videos`:
- returns the recent-video library ordered newest first
- includes pagination metadata

`GET /api/videos/{publicId}`:
- treats `publicId` as the stable share lookup key
- returns lifecycle state from shared Postgres metadata
- exposes `manifestUrl` only after the baseline transcoder has published HLS artifacts and set `manifest_s3_key`
- builds that URL from `PROCESSED_ASSET_BASE_URL`, `CDN_BASE_URL`, or the local S3-compatible base

`GET /api/videos/{publicId}/playback`:
- returns the player-specific contract used by `/v/[publicId]`
- exposes `manifestUrl` for `Auto` ABR playback
- exposes `availableQualities[].playlistUrl` for fixed-quality playback
- reads ready rendition rows from `video_renditions` and orders them by the shared video policy ladder
- returns `pollIntervalMs` so the share page can refresh while higher qualities are still processing

`DELETE /api/videos/{publicId}`:
- requires the `deleteCode` that was set during upload
- verifies the salted hash in Postgres
- deletes the source object prefix from the upload bucket
- deletes the processed HLS prefix from the processed bucket
- removes the `videos` row, which cascades related metadata rows

## Processing Contract

The backend does not run media processing. It hands work off to the processing side through database state:
- `processing_jobs` is the parent job table claimed by `chunker`
- `transcoding_jobs` is the per-rendition table claimed by `transcoder`

Current baseline path:
1. backend inserts a `BASELINE` `processing_jobs` row
2. `chunker` claims it and persists source dimensions
3. `chunker` inserts a `360p` child row into `transcoding_jobs`
4. `transcoder-360p` produces the baseline HLS package
5. share-page reads playback metadata only after `BASELINE_READY`

## Local Run

Preferred local bootstrap:

```bash
./scripts/local/deploy-local-lite.sh --bootstrap-db
```

The local wrapper starts MinIO, the Postgres container, the API/frontend processes, and the Dockerized `chunker` plus `transcoder` services from their own folders.

## Testing

Run backend tests with:

```bash
cargo test --manifest-path backend/Cargo.toml
```

## Deploy Helper

Package the backend with:

```bash
./backend/scripts/deploy-backend.sh --run-tests
```

## Logging

- logs are emitted as structured JSON
- default level is `INFO`
- set `RUST_LOG=debug` before starting the service for deeper traces
- `x-correlation-id` is accepted on API requests and echoed back in the response
