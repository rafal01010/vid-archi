# Chunker

This crate is the first processing stage for the split media pipeline.

Responsibilities:
- receive baseline work from the chunker SQS queue and claim the parent `BASELINE` row in `processing_jobs`
- move videos from `UPLOADED` to `PROCESSING_BASELINE`
- download the source object from the upload bucket
- probe source dimensions with `ffprobe`
- persist `source_width` and `source_height`
- enqueue per-rendition rows in `transcoding_jobs`
- publish the baseline transcoder message to the transcoder SQS queue

Current implemented behavior:
- the chunker dispatches the baseline `360p` transcoding job first
- it also queues an `ADDITIONAL_RENDITIONS` parent job plus any source-eligible higher renditions in Postgres so baseline completion can release them onto SQS later

Logging behavior:
- logs are emitted as structured JSON
- default level is `INFO`
- each claimed job runs inside a span that includes `correlation_id`, `job_id`, `video_id`, and `attempt`

Useful commands:

```bash
cargo test --manifest-path chunker/Cargo.toml --offline
./chunker/scripts/deploy-chunker.sh --run-tests
./chunker/scripts/run-stg-chunker.sh --env-file .env.local
docker compose -f chunker/docker-compose.yml up -d chunker
```
