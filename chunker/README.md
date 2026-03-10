# Chunker

This crate is the first processing stage for the split media pipeline.

Responsibilities:
- receive baseline work from the chunker SQS queue and claim the parent `BASELINE` row in `processing_jobs`
- move videos from `UPLOADED` to `PROCESSING_BASELINE`
- download the source object from the upload bucket
- probe source dimensions with `ffprobe`
- split the uploaded source into reusable source segments
- upload those source segments back into the upload bucket
- persist `source_width` and `source_height`
- enqueue per-segment, per-rendition rows in `transcoding_jobs`
- publish the baseline segment jobs to the transcoder SQS queues

Current implemented behavior:
- the chunker splits the uploaded source into segment files before it dispatches transcoding work
- it publishes all baseline `360p` segment jobs first
- it also queues an `ADDITIONAL_RENDITIONS` parent job plus any source-eligible higher-rendition segment jobs in Postgres so baseline completion can release them onto SQS later

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
