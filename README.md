# Video Streaming Project

This repository contains a minimal private video streaming system for the take-home exam described in [PROJECT_IMPLEMENTATION_PLAN.md](/Users/dave/LabBase/vid-archi/PROJECT_IMPLEMENTATION_PLAN.md).

The project constraints are:
- Rust for the backend API plus processing services.
- Svelte for the frontend.
- AWS S3, SQS, CloudFront, EC2, and Postgres-backed metadata.
- Architecture quality and clean code are more important than breadth.

Primary references:
- [SYSTEM_DESIGN_PROJECT_REQUIREMENTS_GUIDELINES.md](/Users/dave/LabBase/vid-archi/references/SYSTEM_DESIGN_PROJECT_REQUIREMENTS_GUIDELINES.md)
- [youtube_system_design_reference.md](/Users/dave/LabBase/vid-archi/references/youtube_system_design_reference.md)
- [Youtube-problem-writeup.md](/Users/dave/LabBase/vid-archi/references/Youtube-problem-writeup.md)
- [rust_clean_code_guidelines.md](/Users/dave/LabBase/vid-archi/references/rust_clean_code_guidelines.md)
- [svelte_clean_code_guidelines.md](/Users/dave/LabBase/vid-archi/references/svelte_clean_code_guidelines.md)

## Repository Layout

```text
backend/      Rust API service
chunker/      Rust processing service that claims baseline jobs and dispatches rendition jobs
transcoder/   Rust processing service that builds one configured HLS rendition
frontend/     Svelte frontend
docs/         Architecture, API, and operational documentation
config/       Shared policy/config source-of-truth files
references/   Provided exam requirements and design references
```

## Shared Policy Source Of Truth

[config/video_policy.json](/Users/dave/LabBase/vid-archi/config/video_policy.json) defines the upload and transcoding policy used across services.

Current policy highlights:
- max upload size: `1GB`
- accepted input MIME types: `video/mp4`, `video/quicktime`, `video/webm`, `video/x-msvideo`, `video/vnd.avi`
- baseline rendition: `360p` HLS with `H.264 + AAC`
- adaptive ladder: `360p`, `480p`, `720p`, `1080p`, `1440p`, `2160p`
- no upscaling beyond source dimensions

## Runtime Shape

The implemented shape is now split the way the project plan describes:
- app host EC2:
  - Rust API
  - Svelte frontend
  - Postgres container
- chunker host EC2:
  - Dockerized `chunker` container(s) from `chunker/docker-compose.yml`
- transcoder host EC2:
  - Dockerized `transcoder` containers from `transcoder/docker-compose.yml`

The runtime YAML now lives inside the service folders so they are portable on their own. Examples:

```bash
docker compose -f chunker/docker-compose.yml up -d --scale chunker=2 chunker
```

```bash
docker compose -f transcoder/docker-compose.yml up -d \
  --scale transcoder-360p=1 \
  --scale transcoder-480p=1 \
  --scale transcoder-720p=1 \
  --scale transcoder-1080p=1 \
  --scale transcoder-1440p=1 \
  --scale transcoder-2160p=1 \
  transcoder-360p transcoder-480p transcoder-720p transcoder-1080p transcoder-1440p transcoder-2160p
```

That keeps `chunker/` and `transcoder/` independently deployable while still letting you scale each workload separately.

## Processing Flow

1. `POST /api/videos` validates input, creates metadata, and opens a multipart upload.
2. `POST /api/videos/{videoId}/parts/sign` returns presigned part URLs.
3. `POST /api/videos/{videoId}/complete` finalizes the source upload, marks the video `UPLOADED`, and inserts a parent `BASELINE` row into `processing_jobs`.
4. A `chunker` container atomically claims that baseline job, moves the video to `PROCESSING_BASELINE`, downloads the source object, runs `ffprobe`, stores source dimensions, and inserts one row into `transcoding_jobs` for the baseline `360p` rendition.
5. A `transcoder` container that is configured for `TRANSCODER_RENDITION=360p` claims that job, runs `ffmpeg` to completion for a full VOD-style `360p` package, uploads `videos/{video_id}/hls/master.m3u8`, `videos/{video_id}/hls/360p/360p.m3u8`, and all HLS segments into the processed bucket, then marks the rendition `READY`.
6. Only after that full baseline package exists does the transcoder set `videos.manifest_s3_key`, `is_streamable=true`, and `status=BASELINE_READY`.
7. `GET /api/videos/{publicId}` returns `manifestUrl` only after that shared DB state is present.

The code already supports per-rendition transcoder containers. Step `7a` through `7d` in the plan still covers finishing the non-baseline renditions and final `READY` state.

## Scripts

Primary deploy/start scripts:
- app host deploy: `./scripts/stg/deploy-stg-app-host.sh`
- app host start: `./scripts/stg/start-stg-app-host.sh`
- chunker host deploy: `./scripts/stg/deploy-stg-chunker-host.sh`
- chunker host start: `./scripts/stg/start-stg-chunker-host.sh`
- transcoder host deploy: `./scripts/stg/deploy-stg-transcoder-host.sh`
- transcoder host start: `./scripts/stg/start-stg-transcoder-host.sh`

Service packaging scripts:
- `./backend/scripts/deploy-backend.sh`
- `./chunker/scripts/deploy-chunker.sh`
- `./transcoder/scripts/deploy-transcoder.sh`

Local wrappers:
- `./scripts/local/deploy-local-lite.sh`
- `./scripts/local/start-local-lite.sh`
- `./scripts/local/stop-local-lite.sh`

For the real EC2 layout, run the app-host scripts on the app EC2 instance, the chunker scripts on the chunker EC2 instance, and the transcoder scripts on the transcoder EC2 instance.

## Local Development

1. Copy `.env.example` to `.env.local` or `.env`.
2. First local bootstrap: `./scripts/local/deploy-local-lite.sh --bootstrap-db`
3. Later local redeploys: `./scripts/local/deploy-local-lite.sh`
4. Frontend: `http://127.0.0.1:4173`
5. API: `http://127.0.0.1:8080`

Local runtime:
- Postgres container
- MinIO
- Rust API
- Svelte frontend
- Dockerized `chunker` and `transcoder` services started from their own folders

## Current Status

Implemented now:
- multipart upload API
- deterministic public share links
- homepage recent-video listing
- share-page metadata endpoint
- split processing pipeline for `chunker` and `transcoder`
- per-service Docker runtime files inside `chunker/` and `transcoder/`
- baseline `360p` HLS generation and `BASELINE_READY` gating

Still pending:
- dedicated playback metadata endpoint from step `6a`
- browser HLS playback integration from step `6b` to `6d`
- non-baseline rendition production flow from step `7a` to `7d`
- retry/metrics/health work from later steps

## Tests

Run the implemented Rust tests with:

```bash
cargo test --manifest-path backend/Cargo.toml
cargo test --manifest-path chunker/Cargo.toml --offline
cargo test --manifest-path transcoder/Cargo.toml --offline
```
