# Architecture

VidArchi separates request handling from media processing so uploads stay lightweight while video packaging scales independently.

## Component Diagram

```mermaid
flowchart LR
    Browser[Browser]
    Frontend[Frontend]
    API[Backend API]
    DB[(Postgres)]
    Upload[(S3 Upload Bucket)]
    Processed[(S3 Processed Bucket)]
    Chunker[Chunker]
    Transcoder[Transcoder]
    CDN[CloudFront]

    Browser --> Frontend
    Browser --> API
    Browser --> Upload
    API --> DB
    API --> Upload
    Chunker --> DB
    Chunker --> Upload
    Chunker --> DB
    Transcoder --> DB
    Transcoder --> Upload
    Transcoder --> Processed
    API --> DB
    API --> CDN
    Browser --> CDN
    CDN --> Processed
```

## Upload To First Play

```mermaid
sequenceDiagram
    participant U as Browser
    participant A as API
    participant S as Upload Bucket
    participant D as Postgres
    participant C as Chunker
    participant T as Transcoder
    participant P as Processed Bucket/CDN

    U->>A: POST /api/videos
    A->>D: create video + upload session
    A->>S: create multipart upload
    A-->>U: upload session + share link
    U->>A: POST /api/videos/{videoId}/parts/sign
    A-->>U: presigned part URLs
    U->>S: multipart PUT parts
    U->>A: POST /api/videos/{videoId}/complete
    A->>D: mark UPLOADED + queue BASELINE job
    C->>D: claim BASELINE job
    C->>S: download source
    C->>D: persist source dimensions + queue rendition jobs
    T->>D: claim 360p job
    T->>S: download source
    T->>P: upload 360p playlist, segments, master manifest
    T->>D: mark BASELINE_READY
    U->>A: GET /api/videos/{publicId}/playback
    A-->>U: manifestUrl + available qualities
```

## Runtime Responsibilities

- `backend`
  - owns upload session lifecycle, public video lookup, and playback metadata
  - never proxies video bytes
- `chunker`
  - claims parent baseline jobs
  - probes the source file with `ffprobe`
  - creates the baseline transcode job and any eligible higher renditions
- `transcoder`
  - runs one configured rendition per process
  - packages HLS artifacts with `ffmpeg`
  - refreshes `master.m3u8` as renditions finish

## Scaling And Consistency

- Uploads scale with object storage rather than API CPU or memory.
- API nodes remain stateless because streamability is derived from Postgres, not local memory.
- Chunkers scale on queued baseline jobs.
- Transcoders scale by rendition, which allows heavier qualities such as `1080p` or `2160p` to receive more capacity without affecting baseline throughput.
- `BASELINE_READY` is the first playback milestone. Higher renditions continue in the background under `PROCESSING_FULL`.

## Structured Logging

- All Rust services emit JSON logs through `tracing`.
- The API accepts or generates `x-correlation-id` and echoes it in every response.
- `processing_jobs` and `transcoding_jobs` persist `correlation_id`, so chunker and transcoder spans carry the same identifier as the API request that queued the baseline pipeline.
- Default log level is `INFO`. Set `RUST_LOG=debug` on any service for deeper traces.

## STG Topology

Current staging layout:

- App host
  - backend API
  - frontend
  - Postgres container
- Chunker host
  - `chunker` container replicas
- Transcoder host
  - one container definition per rendition
- Shared AWS services
  - ALB: `stg-api-alb`
  - CloudFront: `d38ixt0cyn1hi6.cloudfront.net`
  - Upload bucket: `stg-video-upload-1a`
  - Processed bucket: `stg-video-processed-1a`
  - Chunker queue: `stg-chunker-queue`
  - Transcoder queue: `stg-transcoder-queue`

## Ingress Decision

API Gateway is intentionally omitted from the current staging path. The active ingress path is:

```text
Client -> ALB -> backend API
```

That keeps the environment smaller and cheaper while the service shape is still evolving. The upgrade path is straightforward:

```text
Client -> API Gateway -> ALB -> backend API
```

API Gateway becomes useful when the service needs centralized auth, throttling, request validation, or a stricter public API management layer.
