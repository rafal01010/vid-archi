# Transcoder

This crate is the second processing stage for the split media pipeline.

Responsibilities:
- claim queued rows from `transcoding_jobs`
- run only for one configured `TRANSCODER_RENDITION`
- download the source object
- build HLS artifacts for that rendition with `ffmpeg`
- rebuild and upload `master.m3u8` so `Auto` playback stays aligned with ready renditions
- upload playlists and segments to the processed bucket
- mark baseline streamability when the `360p` rendition finishes
- move additional-rendition work through `PROCESSING_FULL` and finish at `READY`

Container model:
- one Docker service definition exists per rendition
- you can scale `transcoder-360p`, `transcoder-720p`, or `transcoder-2160p` independently
- that is how the service scales packaging pressure by resolution

Logging behavior:
- logs are emitted as structured JSON
- default level is `INFO`
- each claimed job runs inside a span that includes `correlation_id`, `processing_job_id`, `transcoding_job_id`, `video_id`, and `rendition`

Useful commands:

```bash
cargo test --manifest-path transcoder/Cargo.toml --offline
./transcoder/scripts/deploy-transcoder.sh --run-tests
./transcoder/scripts/run-stg-transcoder.sh --env-file .env.local --rendition 360p
docker compose -f transcoder/docker-compose.yml up -d transcoder-360p transcoder-480p transcoder-720p transcoder-1080p transcoder-1440p transcoder-2160p
```
