# Worker

This directory will contain the Rust background worker responsible for:
- picking up post-upload processing jobs
- producing the baseline `360p` HLS rendition first
- updating video lifecycle state transitions safely
- extending processing later with `720p` and `1080p` renditions

Planned internal layout:

```text
src/
  application/     job coordination and retry policy
  domain/          processing job and video state transitions
  infrastructure/  SQS polling, Postgres, S3, ffmpeg integration, config
  media/           manifest writing and rendition packaging helpers
scripts/           EC2 deployment and worker bootstrap scripts
```

The worker must treat `BASELINE_READY` as the first streamable state and should only transition to it when baseline manifest and segment artifacts exist.
