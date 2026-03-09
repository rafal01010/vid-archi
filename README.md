# VidArchi

VidArchi is a private video publishing service built around direct uploads, baseline-first HLS packaging, and a split processing pipeline that keeps the request path independent from media work.

Uploads now require a delete code. The backend stores only a salted hash in Postgres, and the homepage or share page can later use that code to delete the source upload, processed HLS artifacts, and the public listing.

## Services

```text
backend/      Upload, catalog, and playback API
chunker/      Source probe and job fan-out service
transcoder/   Per-rendition HLS packaging service
frontend/     Svelte web application
docs/         Architecture, API, and operations notes
config/       Shared upload and rendition policy
```

## Runtime Overview

- Browsers upload directly to the source bucket with multipart presigned URLs.
- The upload flow also persists a hashed delete code so anonymous users can remove videos later without full account ownership.
- The browser upload flow now supports manual cancel while a multipart transfer is in progress.
- Refreshing the page does not resume an in-progress upload because the active multipart session state is kept client-side only.
- The API writes lifecycle state to Postgres and queues the baseline processing job.
- `chunker` probes the source file and creates one baseline transcoding job plus any source-eligible higher renditions.
- `transcoder` packages HLS output, publishes manifests and segments, and advances the video from `PROCESSING_BASELINE` to `BASELINE_READY`, then to `PROCESSING_FULL` and `READY`.
- Playback URLs are always derived from shared metadata, so every API instance sees the same streamability state.

## Shared Policy

[video_policy.json](/Users/dave/LabBase/vid-archi/config/video_policy.json) is the source of truth for:

- maximum upload size
- accepted video types
- baseline rendition settings
- adaptive ladder order
- no-upscaling rules

## Logging

All Rust services emit structured JSON logs. `INFO` is the default level and `DEBUG` can be enabled with `RUST_LOG=debug`.

Every API request accepts an optional `x-correlation-id` header. If the client does not supply one, the API generates it and echoes it back in the response. The same correlation ID is persisted into processing jobs so the upload, chunker, and transcoder logs can be traced end-to-end.

## Deployment Scripts

- Local bootstrap: `./scripts/local/deploy-local-lite.sh --bootstrap-db`
- Local redeploy: `./scripts/local/deploy-local-lite.sh`
- App host deploy: `./scripts/stg/deploy-stg-app-host.sh`
- Chunker host deploy: `./scripts/stg/deploy-stg-chunker-host.sh`
- Transcoder host deploy: `./scripts/stg/deploy-stg-transcoder-host.sh`

The staging host scripts package binaries, build images where needed, and start the services with environment-driven configuration.

## STG Access

- Through the ALB, open `http://<STG_ALB_DNS>`. In the current staging docs example, that is `http://stg-api-alb-816249004.ap-northeast-1.elb.amazonaws.com`.
- If the ALB is not wired yet, you can open the frontend directly on the app host at `http://<APP_EC2_PUBLIC_IP>:4173`.
- To verify the API directly on the app host, open `http://<APP_EC2_PUBLIC_IP>:8080/healthz`.
- If only the app host is deployed and the worker is still down, the site should load in the browser, but uploads will not finish processing into playable HLS output.

On the app host, the STG start scripts write PID files and logs into `logs/`:
- backend process check: `ps -fp "$(cat logs/backend.pid)"`
- frontend process check: `ps -fp "$(cat logs/frontend.pid)"`
- backend log tail: `tail -f logs/backend.log`
- frontend log tail: `tail -f logs/frontend.log`

## Documentation

- [architecture.md](/Users/dave/LabBase/vid-archi/docs/architecture.md)
- [api.md](/Users/dave/LabBase/vid-archi/docs/api.md)
- [operational-costs.md](/Users/dave/LabBase/vid-archi/docs/operational-costs.md)
- [stg-deployment.md](/Users/dave/LabBase/vid-archi/docs/stg-deployment.md)
