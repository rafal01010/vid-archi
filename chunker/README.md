# Chunker

This crate is the first processing stage for the split media pipeline.

Responsibilities:
- claim queued parent `BASELINE` jobs from `processing_jobs`
- move videos from `UPLOADED` to `PROCESSING_BASELINE`
- download the source object from the upload bucket
- probe source dimensions with `ffprobe`
- persist `source_width` and `source_height`
- enqueue per-rendition rows in `transcoding_jobs`

Current implemented behavior:
- step `5a` through `5d` uses the chunker to dispatch the baseline `360p` transcoding job
- step `7a` now also uses the chunker to queue an `ADDITIONAL_RENDITIONS` parent job plus source-eligible higher renditions

Useful commands:

```bash
cargo test --manifest-path chunker/Cargo.toml --offline
./chunker/scripts/deploy-chunker.sh --run-tests
./chunker/scripts/run-stg-chunker.sh --env-file .env.local
docker compose -f chunker/docker-compose.yml up -d chunker
```
