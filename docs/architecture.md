# Architecture

This document describes the implemented system shape and the intended scale-out direction for the private video streaming service described in [SYSTEM_DESIGN_PROJECT_REQUIREMENTS_GUIDELINES.md](/Users/dave/LabBase/vid-archi/references/SYSTEM_DESIGN_PROJECT_REQUIREMENTS_GUIDELINES.md), [youtube_system_design_reference.md](/Users/dave/LabBase/vid-archi/references/youtube_system_design_reference.md), and [Youtube-problem-writeup.md](/Users/dave/LabBase/vid-archi/references/Youtube-problem-writeup.md).

## Goals

- anonymous users can upload videos up to `1GB`
- upload bytes bypass the API and go directly to object storage
- the first streamable milestone is `BASELINE_READY`
- the codebase matches the documented split between app host, chunker host, and transcoder host

## Implemented Runtime Shape

```text
Browser
  -> Rust API on app EC2
     -> Postgres on app EC2
     -> upload bucket

Browser
  -> upload bucket (direct multipart PUT)

Chunker container(s) on processing EC2
  -> processing_jobs
  -> upload bucket (download source)
  -> ffprobe
  -> transcoding_jobs

Transcoder container(s) on processing EC2
  -> transcoding_jobs filtered by one configured rendition
  -> upload bucket (download source)
  -> ffmpeg
  -> processed bucket (master manifest, variant playlist, HLS segments)

Browser
  -> GET /api/videos/{publicId}/playback
     -> Postgres
     -> manifestUrl + rendition playlist URLs derived from processed-asset base URL / CDN base URL
```

## Why It Is Split This Way

The direct browser-to-object-storage upload path follows the large-blob guidance in the YouTube system design references: the API should coordinate uploads, not proxy video bytes.

The processing side is intentionally split into `chunker` and `transcoder` because that is the scaling story the take-home expects:
- chunkers can scale horizontally when many uploads complete at once
- transcoders can scale by resolution
- heavy renditions such as `2160p` can get more replicas without also scaling `360p`

Docker is part of that story. The runtime YAML lives with the service that uses it so `chunker/` and `transcoder/` can be deployed independently onto separate EC2 instances.

## Upload And Baseline Processing Sequence

1. `POST /api/videos` creates the `videos` row, creates the `upload_sessions` row, and starts a multipart upload in the upload bucket.
2. `POST /api/videos/{videoId}/parts/sign` returns presigned multipart URLs and moves the video to `UPLOADING` on first use.
3. `POST /api/videos/{videoId}/complete` finalizes the source upload, sets the video to `UPLOADED`, and inserts a parent `BASELINE` row into `processing_jobs`.
4. A `chunker` container claims one queued baseline job atomically, marks the parent job `RUNNING`, and moves the video to `PROCESSING_BASELINE`.
5. The chunker downloads `videos/{video_id}/source/original`, probes source dimensions with `ffprobe`, persists `source_width` and `source_height`, inserts a child row into `transcoding_jobs` for the baseline rendition, and queues an `ADDITIONAL_RENDITIONS` parent job plus higher renditions that do not exceed the source resolution.
6. A `transcoder` container with `TRANSCODER_RENDITION=360p` claims that job, downloads the same source object, runs `ffmpeg` to completion for a VOD package, and writes:
   - `videos/{video_id}/hls/master.m3u8`
   - `videos/{video_id}/hls/360p/360p.m3u8`
   - `videos/{video_id}/hls/360p/segment_*.ts`
7. After the full `360p` playlist and all of its segments are uploaded to the processed bucket, the transcoder marks the `360p` rendition `READY`, stores `videos.manifest_s3_key`, sets `is_streamable=true`, transitions the video to `BASELINE_READY` or directly to `READY` when no higher renditions are planned, and marks the parent baseline job `SUCCEEDED`.
8. Higher-rendition transcoders claim their queued rows only after the video is already streamable. The first additional-renditions claim moves the video to `PROCESSING_FULL`.
9. Every successful rendition rebuilds `videos/{video_id}/hls/master.m3u8` from the current set of ready renditions so `Auto` playback and fixed-quality playback stay aligned.
10. The last source-eligible additional rendition moves the video to `READY` and marks the `ADDITIONAL_RENDITIONS` parent job `SUCCEEDED`.
11. `GET /api/videos/{publicId}/playback` returns:
   - `manifestUrl` for `Auto` adaptive bitrate playback from the master manifest
   - one `playlistUrl` per ready rendition for fixed-resolution playback such as `360p` or `1080p`
   - actual encoded rendition dimensions from `video_renditions`, so source-capped variants remain accurate in the UI
   - a polling interval so the share page can keep discovering new qualities while processing continues

## State And Queue Guardrails

The current guardrails live in Postgres:
- `processing_jobs` holds the parent baseline job lifecycle
- `transcoding_jobs` holds per-rendition work items
- chunkers and transcoders claim rows with atomic `RUNNING` transitions
- `BASELINE_READY` is written only after the full baseline VOD package exists
- the share-page response reads streamability from shared DB metadata, not local process state

That matches the consistency requirements in the exam references and keeps the current implementation compatible with a later SQS-driven version.

## Dockerized Service Folders

[chunker/docker-compose.yml](/Users/dave/LabBase/vid-archi/chunker/docker-compose.yml) defines:
- `chunker`

[transcoder/docker-compose.yml](/Users/dave/LabBase/vid-archi/transcoder/docker-compose.yml) defines:
- `transcoder-360p`
- `transcoder-480p`
- `transcoder-720p`
- `transcoder-1080p`
- `transcoder-1440p`
- `transcoder-2160p`

Scaling examples:

```bash
docker compose -f chunker/docker-compose.yml up -d chunker
```

```bash
docker compose -f transcoder/docker-compose.yml up -d \
  --scale transcoder-360p=3 \
  --scale transcoder-2160p=2 \
  transcoder-360p transcoder-2160p
```

This is how the system simulates autoscaling while keeping each service folder self-contained.

## Deployment Story

App host:
- Rust API
- Svelte frontend
- Postgres container

Chunker host:
- Docker engine
- built `chunker` image
- compose-managed chunker containers

Transcoder host:
- Docker engine
- built `transcoder` image
- compose-managed transcoder containers

The deploy scripts are split accordingly:
- [deploy-stg-app-host.sh](/Users/dave/LabBase/vid-archi/scripts/stg/deploy-stg-app-host.sh)
- [start-stg-app-host.sh](/Users/dave/LabBase/vid-archi/scripts/stg/start-stg-app-host.sh)
- [deploy-stg-chunker-host.sh](/Users/dave/LabBase/vid-archi/scripts/stg/deploy-stg-chunker-host.sh)
- [start-stg-chunker-host.sh](/Users/dave/LabBase/vid-archi/scripts/stg/start-stg-chunker-host.sh)
- [deploy-stg-transcoder-host.sh](/Users/dave/LabBase/vid-archi/scripts/stg/deploy-stg-transcoder-host.sh)
- [start-stg-transcoder-host.sh](/Users/dave/LabBase/vid-archi/scripts/stg/start-stg-transcoder-host.sh)

## Current Boundary

What is implemented now:
- baseline `360p` flow through chunker plus transcoder
- master manifest generation and safe expansion as higher renditions finish
- dedicated playback endpoint that exposes the master manifest plus ready rendition playlists
- browser playback page with polling, native HLS support, and HLS.js fallback
- quality switching design:
  - `Auto` loads the master manifest and leaves rendition choice to the HLS player
  - fixed quality loads the selected variant playlist directly so only that resolution is fetched
- source-capped higher-rendition flow from `BASELINE_READY` to `PROCESSING_FULL` to `READY`
