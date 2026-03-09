# Operational Costs

This document captures the current staging footprint, the main cost drivers, and the guardrails used to keep the environment predictable.

## Current STG Resources

Public entry:

- ALB: `stg-api-alb`
- CloudFront distribution: `d38ixt0cyn1hi6.cloudfront.net`

Storage and queues:

- Upload bucket: `stg-video-upload-1a`
- Processed bucket: `stg-video-processed-1a`
- Chunker queue: `stg-chunker-queue`
- Transcoder queues: `stg-transcoder-{360p,480p,720p,1080p,1440p,2160p}-queue`

Compute layout:

- app host for backend, frontend, and Postgres
- dedicated chunker host
- dedicated transcoder host

## Primary Cost Drivers

- EC2 compute for the app, chunker, and transcoder hosts
- S3 storage for uploaded source files and HLS outputs
- S3 request volume during multipart upload and segment delivery
- CloudFront egress and request volume for manifests and segments
- Postgres storage on the app host volume

## Cost-Control Decisions

- Direct-to-S3 multipart upload keeps large files off the API host.
- `360p` is packaged first so the service can stop spending time on startup-critical work as soon as playback is available.
- Higher renditions are limited by the shared ladder and never upscale past the source dimensions.
- CloudFront serves playback assets so repeat segment reads do not go back to S3 unnecessarily.
- Incomplete multipart uploads are aborted after `1` day through a bucket lifecycle rule.
- Postgres runs on the app host in staging to keep the environment smaller than an RDS-based layout.

## STG Monthly Guardrails

These are practical operating targets rather than billing guarantees:

- keep the environment to one app host, one chunker host, and one transcoder host unless queue depth proves otherwise
- keep the rendition ladder capped at the current shared policy
- keep CloudFront enabled for playback traffic
- keep abandoned multipart cleanup enabled
- remove large unused source files or move them to a colder storage strategy if retention requirements allow it

## API Gateway Note

The current staging environment does not place API Gateway in front of the ALB. That avoids an extra managed hop and reduces fixed overhead while the service remains small. If the public API surface needs centralized auth, throttling, or stricter request governance, API Gateway is the next ingress layer to add.
