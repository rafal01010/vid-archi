# Video Streaming Take-Home: End-to-End Implementation Plan

## 1) Purpose

This document is a build blueprint for implementing the take-home exam project: a minimal private video streaming service focused on architecture quality, maintainability, and fast time-to-stream.

It is written to be usable by both a human implementer and coding LLMs.

## 2) Source Of Truth And How To Use References

Primary exam requirements:
- `references/SYSTEM_DESIGN_PROJECT_REQUIREMENTS_GUIDELINES.md`

Supporting architecture references:
- `references/youtube_system_design_reference.md`
- `references/Youtube-problem-writeup.md`

How to apply references:
- Treat `SYSTEM_DESIGN_PROJECT_REQUIREMENTS_GUIDELINES.md` as authoritative for scope and constraints.
- Use `youtube_system_design_reference.md` for implementation primitives (multipart upload, manifest, segmenting, ABR, status gating).
- Use `Youtube-problem-writeup.md` for deeper reasoning on tradeoffs (segmented storage, pipeline orchestration, scaling reads, CDN usage).

Shell-script rule:
- When a service becomes runnable or deployable, add companion `.sh` scripts for:
  - local execution
  - STG deployment/build/startup handoff
  - environment variable loading from `.env` or a passed env file
  - app-host and processing-host deployment/build/startup handoff when services run on separate EC2 instances

## 2.1) Mandatory Post-Generation Verification Rule

After generating or editing code, immediately run the relevant unit tests before moving on.

Current required commands:
- Rust backend changes:
  - `cargo test --manifest-path backend/Cargo.toml`
- Rust chunker changes:
  - `cargo test --manifest-path chunker/Cargo.toml --offline`
- Rust transcoder changes:
  - `cargo test --manifest-path transcoder/Cargo.toml --offline`
- Svelte frontend changes:
  - add and run `npm --prefix frontend run test -- --run` once the frontend test harness is scaffolded

Execution rule:
- Do not treat code generation as complete until the relevant unit tests have been run.
- If tests cannot be run, record the blocker explicitly in notes/README/final handoff.

## 3) Exam Requirement Checklist (Must Pass)

These are non-negotiable outcomes:
- Users can upload video files (up to 1GB).
- Support common formats (at least MP4/MOV/WebM input; current implementation policy also accepts AVI uploads).
- Upload can be anonymous (no auth required).
- System generates a shareable link per video.
- Visiting shareable link streams video in browser.
- Time-to-stream is prioritized over quality.
- Architecture/design documentation is included.

Bonus outcomes to include in design and as much implementation as possible:
- Playback remains consistent across file sizes.
- Horizontal scaling story is explicit and coherent.
- AWS/S3 cost efficiency is discussed with practical choices.

Explicit exclusions:
- Do not implement authentication.
- Do not implement IaC.

## 4) Proposed Project Shape (Pragmatic For 1 Week)

Use a monorepo with clear boundaries:

```text
/backend            # Rust API service (upload sessions, metadata, share links, playback metadata)
/chunker            # Rust processing service that claims baseline jobs and dispatches rendition jobs
/transcoder         # Rust processing service that builds one configured HLS rendition
/frontend           # Svelte app (upload page + playback page)
/docs               # Architecture docs + ADRs + API contracts + operational notes
/references         # Provided requirement/reference docs
```

Recommended runtime stack:
- Backend: Rust + Axum + SQLx (or Diesel) + Tokio.
- Chunker: Rust + Tokio + ffprobe invocation.
- Transcoder: Rust + Tokio + ffmpeg invocation.
- Frontend: SvelteKit + HTML5 video (HLS.js if needed).
- Object storage: AWS S3.
- Metadata DB: Postgres (MVP). The current implementation path uses a Postgres Docker container on the app EC2 host for both local development and STG. Production-like target is managed Postgres (RDS).
- Async trigger: SQS queue (or S3 event -> webhook endpoint for MVP).
- CDN: CloudFront in front of S3 for manifests/segments.

STG deployment profile options:
- `STG-Demo (current target)`:
  - 1 app EC2 host for API + frontend + Postgres container.
  - 1 chunker EC2 host running Dockerized `chunker` containers from the `chunker/` folder.
  - 1 transcoder EC2 host running Dockerized `transcoder-*` containers from the `transcoder/` folder.
  - S3 + CloudFront + SQS.
  - ALB as internet ingress; API Gateway intentionally omitted to stay within interview budget.
  - Docker Compose under `chunker/` and `transcoder/` simulates autoscaling through per-service scaling.
- `STG-Prod-Like (higher confidence, higher cost)`:
  - API on ECS/EC2 behind ALB.
  - API Gateway HTTP API in front of ALB.
  - Chunker split into separate services/instance pools.
  - Transcoder split into per-rendition services/instance pools.
  - Managed Postgres (RDS), S3, CloudFront, SQS.
  - Optional autoscaling group for API.

STG database note:
- For interview-budget environments, prefer a Postgres container on EC2 with a persistent Docker volume.
- Local development follows the same shape: Postgres container plus persistent Docker volume.
- Keep DB private to the host/network; do not expose 5432 publicly.
- Example bootstrap command:
```bash
docker run -d \
  --name stg-postgres \
  --restart unless-stopped \
  -e POSTGRES_DB=video_stg \
  -e POSTGRES_USER=video_app \
  -e POSTGRES_PASSWORD='change_me' \
  -v pgdata:/var/lib/postgresql/data \
  -p 127.0.0.1:5432:5432 \
  postgres:16
```
- Persistence rule:
  - stopping or recreating the container does not remove data if the named volume is preserved
  - data is lost if the volume is deleted or the host disk is lost
- Backup rule:
  - take regular SQL backups with `pg_dump`
  - for EC2 STG, also treat EBS snapshots as an infrastructure-level recovery option
- Ideal future upgrade path: move to RDS Postgres when higher availability/ops isolation is required.

API Gateway decision note:
- Current plan intentionally omits API Gateway in the current STG deployment for cost control and reduced setup complexity.
- Current STG ingress path is `Client -> ALB -> API service`.
- Production-like path can be `Client -> API Gateway -> ALB/API service` when budget allows.
- Benefits of API Gateway (document in final architecture notes):
  - central auth integration points (future auth/JWT/OIDC)
  - request throttling and quotas
  - request/response transformation and validation
  - unified API lifecycle features (stages, versioning, usage plans)
  - tighter integration with WAF/observability and managed edge behavior

Why this shape:
- Fast to implement.
- Clear separation of request path vs processing path.
- Preserves scalability story required by exam.

## 5) High-Level Architecture

Core components:
- `Frontend (Svelte)`
  - Anonymous upload UI.
  - Share link display.
  - Playback page by public link.
- `API Service (Rust)`
  - Create video record + upload session.
  - Issue presigned multipart URLs.
  - Complete upload + verify status.
  - Return playback info/state.
  - Serve share-link resolver.
- `S3`
  - Source uploads.
  - Rendition segments.
  - Manifests.
- `Chunker Service (Rust + ffprobe)`
  - Claims queued baseline jobs after upload completion.
  - Probes source dimensions.
  - Dispatches per-rendition transcoding jobs.
- `Transcoder Service (Rust + ffmpeg)`
  - One container runs one configured rendition.
  - Baseline rendition first (`360p`).
  - Marks streamable only when baseline manifest exists.
  - Additional rendition containers can scale independently.
- `Metadata DB`
  - Source of truth for video lifecycle state.
  - State transitions and manifest pointers.
  - Current STG implementation: Postgres container on the app EC2 host.
  - Production-like implementation: RDS Postgres in private subnets.
- `CDN (CloudFront)`
  - Cache manifests/segments for stream performance and lower egress costs.

Critical design principles:
- Direct-to-S3 upload (no large-file pass-through API servers).
- Separate upload chunks from playback segments.
- State machine with `BASELINE_READY` as stream gate.
- Shared durable metadata store to keep multi-instance behavior consistent.

## 6) Video Lifecycle State Machine

Use explicit states in DB. The goal is to distinguish:
- upload lifecycle
- first streamable moment
- background additional renditions after the video is already playable
- terminal failure

Recommended happy-path transition:

```text
INITIATED
  -> UPLOADING
  -> UPLOADED
  -> PROCESSING_BASELINE
  -> BASELINE_READY
  -> PROCESSING_FULL
  -> READY
```

State definitions:
- `INITIATED`
  - Meaning: the `videos` row exists and an upload session can now be created or has just been created.
  - Playback: not streamable.
  - Typical entry condition: API accepted video metadata and reserved a public share ID.
  - Typical exit: client begins multipart upload and state moves to `UPLOADING`.
- `UPLOADING`
  - Meaning: multipart upload is in progress.
  - Playback: not streamable.
  - Typical entry condition: at least one upload part has started or upload session is actively being used.
  - Typical exit: S3 multipart completion succeeds and the source object is now durable.
- `UPLOADED`
  - Meaning: the original source file exists in object storage, but no streamable rendition is available yet.
  - Playback: not streamable.
  - Typical entry condition: S3 upload completed event or trusted completion flow finalized the source object.
  - Typical exit: chunker claims a baseline processing job and dispatches the `360p` transcoder job.
- `PROCESSING_BASELINE`
  - Meaning: the system is producing the first low-quality streamable rendition, which is the exam-critical path.
  - Playback: not streamable yet.
  - Typical entry condition: chunker claimed the baseline job and the baseline transcoder started packaging.
  - Typical exit: baseline playlist, segments, and master manifest entry exist in storage.
- `BASELINE_READY`
  - Meaning: the first streamable version is available. This is the first success condition for the project.
  - Playback: streamable now.
  - Typical artifact expectation: `360p` HLS output exists and the master manifest points to it.
  - Why this state exists separately: it marks the exact time-to-stream milestone before additional renditions begin.
  - Typical exit: additional transcoder containers continue with higher renditions and move the video to `PROCESSING_FULL`.
- `PROCESSING_FULL`
  - Meaning: the video is already streamable, but higher-quality renditions such as `720p` and `1080p` are still being generated.
  - Playback: still streamable.
  - Why this state matters: it distinguishes "playable at baseline quality" from "playable while quality improvements are still in progress."
  - Typical exit: all planned renditions are finished and the final master manifest is complete.
- `READY`
  - Meaning: all planned processing is complete.
  - Playback: streamable.
  - Typical artifact expectation: baseline and higher renditions are present, rendition metadata is finalized, and no more processing work is pending for the video.
  - This is the final steady state for successful processing.
- `FAILED`
  - Meaning: processing or upload-related recovery has exhausted retries or hit a terminal validation/media error.
  - Playback: not streamable unless failure happened after a previously streamable state and the system explicitly chooses to preserve that behavior.
  - Typical artifact expectation: error code/message are captured for UI, logs, and debugging.
  - This is a terminal state unless the system later adds an explicit manual retry/reset flow.

State transition rules:
- Do not return `streamable=true` until manifest exists and status is `BASELINE_READY`, `PROCESSING_FULL`, or `READY`, except for the explicit preserved-playback case where a later non-baseline failure moves the video to `FAILED` after it was already streamable.
- Treat `BASELINE_READY` as the first streamable milestone and the primary exam success condition.
- Use `PROCESSING_FULL` only after the video is already playable and background transcoders are continuing additional-rendition work.
- `READY` means all planned renditions for the current MVP ladder are complete.
- Transition writes must be atomic and idempotent.
- Chunker/transcoder containers should tolerate duplicate events and repeated queue deliveries.
- Shared DB state is the source of truth; never rely on per-instance memory to decide streamability.

## 7) Data Model (Minimal But Complete)

### 7.1 `videos`

Fields:
- `id` (UUID internal id)
- `public_id` (short share slug, unique)
- `delete_code_salt`
- `delete_code_hash`
- `title` (nullable)
- `original_filename`
- `content_type`
- `size_bytes`
- `source_s3_key`
- `source_width` (nullable; original width from media probe)
- `source_height` (nullable; original height from media probe)
- `manifest_s3_key`
- `status` (enum from lifecycle)
- `is_streamable` (bool, derived but useful for quick filters)
- `baseline_ready_at` (timestamp nullable)
- `ready_at` (timestamp nullable)
- `error_code` (nullable)
- `error_message` (nullable)
- `created_at`, `updated_at`

Indexes:
- unique(`public_id`)
- index(`status`)
- index(`created_at`)

### 7.2 `upload_sessions`

Fields:
- `id` (UUID)
- `video_id` (FK)
- `s3_upload_id`
- `part_size_bytes`
- `expires_at`
- `status` (`OPEN|COMPLETED|ABORTED|EXPIRED`)
- `created_at`, `updated_at`

### 7.3 `upload_parts`

Fields:
- `session_id` (FK)
- `part_number`
- `etag`
- `size_bytes`
- `uploaded_at`

Index/constraints:
- unique(`session_id`, `part_number`)

### 7.4 `video_renditions`

Fields:
- `video_id` (FK)
- `rendition` (`360p|480p|720p|1080p|1440p|2160p`)
- `codec` (e.g. h264)
- `container` (e.g. fmp4/ts)
- `playlist_key`
- `output_width`
- `output_height`
- `target_video_bitrate_kbps`
- `target_audio_bitrate_kbps`
- `status` (`PROCESSING|READY|FAILED`)
- `segment_count`
- `created_at`, `updated_at`

Constraint:
- unique(`video_id`, `rendition`)

### 7.5 `processing_jobs`

Fields:
- `id`
- `video_id`
- `job_type` (`BASELINE|ADDITIONAL_RENDITIONS`)
- `attempt`
- `status` (`QUEUED|RUNNING|SUCCEEDED|FAILED`)
- `worker_id`
- `started_at`, `finished_at`
- `error`

Purpose:
- Retries, observability, idempotency guardrails.

## 8) S3 Object Key Strategy

Use deterministic keys:
- Source upload:
  - `videos/{video_id}/source/original`
- Baseline output:
  - `videos/{video_id}/hls/360p/segment_{n}.ts`
  - `videos/{video_id}/hls/360p/index.m3u8`
- Higher outputs:
  - `videos/{video_id}/hls/480p/...`
  - `videos/{video_id}/hls/720p/...`
  - `videos/{video_id}/hls/1080p/...`
  - `videos/{video_id}/hls/1440p/...`
  - `videos/{video_id}/hls/2160p/...`
- Master manifest:
  - `videos/{video_id}/hls/master.m3u8`

Benefits:
- Easy cleanup/lifecycle policies.
- Idempotent overwrite behavior on retry.
- No runtime S3 LIST dependency.

## 9) API Contract (MVP + Scale-Ready)

### 9.1 Create upload

`POST /api/videos`

Request:
- `filename`
- `contentType` (allow `video/mp4`, `video/quicktime`, `video/webm`, `video/x-msvideo`, `video/vnd.avi`)
- `sizeBytes` (validate `<= 1GB`)
- optional `title`
- required `deleteCode` (plaintext accepted once, persisted only as a salted hash)

Response:
- `videoId`
- `publicId`
- `uploadSessionId`
- `s3UploadId`
- `sourceObjectKey`
- `partSizeBytes`
- `uploadExpiresAt`
- `signPartsEndpoint`
- `completeUploadEndpoint`
- `playbackPath`

### 9.2 Sign parts (if not pre-generated)

`POST /api/videos/{videoId}/parts/sign`

Request:
- `uploadSessionId`
- list of `partNumbers`

Response:
- `expiresAt`
- `[{ partNumber, url, method }]`

### 9.3 Record part uploaded (optional metadata tracking)

`PATCH /api/videos/{videoId}/parts`

Request:
- `partNumber`
- `etag`
- `sizeBytes`

### 9.4 Complete multipart upload

`POST /api/videos/{videoId}/complete`

Request:
- `uploadSessionId`
- `parts: [{partNumber, etag}]`

Behavior:
- Complete S3 multipart.
- Mark video `UPLOADED`.
- Insert baseline processing job in `processing_jobs` (`QUEUED`, attempt `1`).

Implementation note:
- Current backend implementation lives under `backend/src`.
- `public_id` is deterministic: `sanitized-title-or-filename + "-" + lowercase-base32(video_id-bytes)`.
- The playback page path returned by upload endpoints is `/v/{publicId}`.

### 9.5 List recent videos for homepage

`GET /api/videos?page=1&pageSize=10`

Response:
- `videos: [{ publicId, title, originalFilename, status, isStreamable, createdAt, updatedAt, playbackPath }]`
- `page`, `pageSize`, `totalCount`, `totalPages`, `hasPreviousPage`, `hasNextPage`

Behavior:
- Return newest videos first using `created_at DESC`.
- Support the homepage/frontpage feed that shows the most recent 10 uploaded videos by default.
- Allow pagination so older videos remain browseable as uploads grow.
- Keep the original upload-to-share-link flow intact; the homepage is an additional discovery surface, not a replacement.

### 9.6 Get video status/details

`GET /api/videos/{publicId}`

Response:
- `publicId`
- `title`
- `originalFilename`
- `status`
- `isStreamable`
- `createdAt`
- `updatedAt`
- `playbackPath` (frontend route)
- `manifestUrl` (if streamable)

### 9.6A Delete video

`DELETE /api/videos/{publicId}`

Request:
- `deleteCode`

Behavior:
- Verify the salted delete-code hash in `videos`.
- Abort any still-open multipart upload if one exists.
- Reject deletes while status is `PROCESSING_BASELINE` or `PROCESSING_FULL` so in-flight workers cannot recreate artifacts after cleanup starts.
- Delete `videos/{video_id}/...` objects from the upload bucket.
- Delete `videos/{video_id}/...` HLS artifacts from the processed bucket.
- Delete the `videos` row so related metadata rows cascade away.

### 9.7 Playback entrypoint (for player page)

`GET /api/videos/{publicId}/playback`

Response:
- `publicId`
- `title`
- `originalFilename`
- `status`
- `isStreamable`
- `playbackPath`
- `manifestUrl` when streamable
- `defaultQuality` (`auto`)
- `pollIntervalMs`
- optional `availableQualities[]` for fixed-resolution playback
  - `name`
  - `label`
  - `width`
  - `height`
  - `codec`
  - `container`
  - `playlistUrl`

API impact note for the expanded ladder:
- No upload API change is required for source-capped multi-resolution support.
- The HLS master manifest remains the source of truth for `Auto` ABR playback.
- `availableQualities[]` can be served from ready `video_renditions` metadata so the frontend can lock playback to a specific variant playlist such as `1080p`.
- Step `7` must keep `video_renditions` readiness and master-manifest updates logically aligned so `Auto` playback and fixed-quality playback converge on the same set of usable renditions.

### 9.8 Health/readiness

`GET /healthz`
- Includes DB, S3 reachability checks.

## 10) Upload And Stream Flows

### 10.1 Upload flow (anonymous + large-file-safe)

1. Frontend requests create-upload session.
2. Backend validates size/type, hashes the delete code, and inserts `videos + upload_sessions`.
3. Backend creates multipart upload in S3 and returns signing data.
4. Frontend uploads parts directly to S3.
5. Frontend calls complete endpoint with ETags.
6. Backend completes multipart, transitions to `UPLOADED`, and enqueues the parent baseline processing job.

### 10.2 Processing flow (time-to-stream first)

1. Chunker picks the parent baseline job.
2. Chunker probes the original file dimensions and enqueues the baseline `360p` transcoder job.
3. The baseline transcoder builds segmented `360p` HLS output and creates the variant playlist.
4. Upload manifest/segments to S3.
5. Create/update master manifest with the `360p` entry only.
6. DB transition to `BASELINE_READY` and set `is_streamable=true` only after the full `360p` VOD artifacts exist.
7. Later, enqueue and complete higher renditions that do not exceed the source resolution, then update the master manifest and set `READY`.

### 10.3 Playback flow

1. User opens `/v/{publicId}` in browser.
2. Frontend calls playback endpoint.
3. If `BASELINE_READY/READY`, player fetches master manifest and begins streaming with 360p available immediately.
4. Player performs ABR variant switching as network/device conditions change (via HLS client behavior).
5. If the user selects a fixed quality such as `720p`, the player swaps from the master manifest URL to that rendition's variant playlist URL.
6. If not ready, show processing state and poll.

## 11) Frontend Plan (Svelte)

Pages:
- `/` homepage with upload flow + recent-video library
- `/v/[publicId]` playback page

Homepage features:
- file picker + drag/drop
- client-side size/type pre-validation
- delete-code + confirmation fields during upload
- multipart progress bar
- cancel-upload action while the current browser upload is still in progress
- error/retry states
- show generated share link on completion
- show recent uploads sorted newest-first
- show 10 videos per page by default
- previous/next pagination controls for older uploads
- every library entry links to `/v/[publicId]`
- every library entry exposes delete-by-code as an anonymous ownership substitute
- monochrome styling only (`black/white/gray`)

Playback page features:
- resolve the public share ID and show lifecycle state immediately
- status polling until streamable
- video player using HLS manifest URL
- fallback for unsupported browsers
- delete action using the same delete code as the homepage flow

Current browser limitation:
- Full-page refresh does not resume an in-progress multipart upload because the active upload session, presigned part URLs, and completed-part ETags are kept in browser memory only.
- clear state labels: `Uploading`, `Processing`, `Ready`, `Failed`

UX rules aligned to exam:
- Functional UI is enough; avoid over-investing in styling.
- Ensure "time-to-first-play" visibly prioritized once baseline ready.
- Write visible UI copy for end users, not for developers, reviewers, or the take-home checklist.
- Do not expose implementation language in default UI text, including references to checklist steps, project phases, backend architecture, S3, multipart upload internals, `publicId`, route templates, or exam wording.
- Prefer concise product language such as `File size limit`, `Accepted files`, `Status`, `Share link`, and `Recent videos`.
- Hide internal identifiers and infrastructure details unless they are required for a real user task or an explicit admin/debug view.

## 12) Chunker/Transcoding + ABR Plan

ffmpeg strategy for MVP:
- Input: source S3 object (download to local temp file).
- Output baseline quickly:
  - 360p, moderate bitrate, short segment duration (2-4s), H.264 + AAC.
- Generate HLS artifacts:
  - variant playlists (`360p.m3u8`, then source-eligible higher variants such as `480p.m3u8`, `720p.m3u8`, `1080p.m3u8`, `1440p.m3u8`, `2160p.m3u8`)
  - master playlist (`master.m3u8`) for ABR.
- Generate master manifest immediately with baseline entry only.
- Later add the remaining source-eligible variants and update master manifest.

ABR ladder for MVP:
- 360p baseline (first streamable target)
- 480p
- 720p
- 1080p
- 1440p
- 2160p

No-upscaling rule:
- Never generate a rendition above the source file resolution.
- Example:
  - a `2160p` source can expose `360p`, `480p`, `720p`, `1080p`, `1440p`, and `2160p`
  - a `1080p` source can expose only `360p`, `480p`, `720p`, and `1080p`
- Persist `source_width` and `source_height` in `videos` so chunker/transcoder decisions and architecture documentation share the same source of truth.

Why this matters:
- ABR is essential for low-bandwidth and device diversity: players can switch rendition without restarting playback.
- Time-to-stream remains prioritized because 360p is produced first and gates streamability.

Processing execution model:
- Backend inserts durable job rows in Postgres and publishes queue messages to SQS.
- Chunker consumes the chunker SQS queue, then atomically claims the referenced parent baseline job row in Postgres.
- Transcoder consumes the transcoder SQS queue, then atomically claims the referenced rendition job row in Postgres.
- Lock job atomically (`RUNNING`) to avoid duplicate claims.
- Retry transient failures with capped attempts + backoff.
- Move permanent failures to `FAILED` with error details.

Processing deployment modes:
- `Current STG`: separate app host, chunker host, and transcoder host, with runtime YAML stored inside each service folder.
- `Scale-out mode` (diagram-aligned): separate chunker and transcoder pools.
  - Ingest queue/event triggers chunker.
  - Chunker emits per-rendition transcode jobs.
  - Multiple transcoder instances consume jobs in parallel.
  - This can be deployed as EC2 instance groups, ECS services, or Docker Compose service replicas for the take-home demo.

Where horizontal scaling happens for processing:
- Increase `Chunker` instances when upload completion events back up.
- Increase `Transcoder` instances when rendition queue depth grows.
- Keep API scaling independent from processing scaling.

Idempotency:
- Derive output keys from `video_id` and rendition.
- Safe overwrite of partial artifacts.
- Check existing ready files before reprocessing.

## 13) Horizontal Scaling And Race Condition Handling

### 13.1 No head-of-line blocking on uploads

Design decisions:
- Upload bytes go directly client -> S3.
- API instances only handle small metadata requests.
- Large upload from one user does not monopolize server CPU/memory.

### 13.2 Multi-instance consistency

Rules:
- `videos` table is source of truth for readiness.
- Do not rely on per-instance memory/cache for status decisions.
- If using cache, use short TTL and invalidate on status transitions.

### 13.3 Read-your-writes expectations

Pragmatic behavior:
- Uploader can immediately see upload status from DB-backed endpoint.
- Global viewers may briefly see processing state until transition propagation is complete.
- Never return manifest URL if DB status is below `BASELINE_READY`.

## 14) Cost-Efficiency Plan (AWS + S3)

Must be documented even if partially implemented:
- Put CloudFront before S3 for playback artifacts.
- Keep the configured ladder explicit (`360p`, `480p`, `720p`, `1080p`, `1440p`, `2160p`) but only generate renditions up to the source resolution.
- Use lifecycle policies:
  - Abort incomplete multipart uploads.
  - Optionally transition old originals to cheaper storage class.
- Avoid runtime S3 LIST by storing explicit keys in DB.
- Keep segment duration balanced (2-6s) to avoid too many GETs.

CDN content and policy:
- Put CloudFront in front of S3 for:
  - `master.m3u8` and variant playlists (`*.m3u8`)
  - segment files (`*.ts` or `*.m4s`)
- Cache policy guidance:
  - manifests: shorter TTL (faster freshness while processing completes)
  - segments: longer TTL (high cache-hit potential)
- Prioritize first-play experience:
  - ensure manifest + first few segments are cacheable immediately
  - optional: prefetch/warm first 2-3 segments for newly ready videos in STG tests
- Current STG CloudFront distribution domain (use this for playback URL construction):
  - `d38ixt0cyn1hi6.cloudfront.net`

Low-cost STG guidance:
- Start with the current two-host STG shape and scale replicas only after demo stability.
- Add billing alarms/budgets on day zero.
- Keep managed services at smallest tiers; disable idle resources when not testing.

## 15) Security And Abuse Baseline

Even without auth:
- Validate file size and allowed content types both client and server side.
- Limit presigned URL expiration (short TTL).
- Scope presigned permissions to exact object key/session.
- Rate limit session creation by IP.
- Store only a salted hash of the upload-time delete code; never persist the plaintext delete secret.
- Sanitize file names and never trust client metadata blindly.

## 16) Observability And Operations

Structured logs (JSON) with correlation IDs:
- `video_id`
- `public_id`
- `upload_session_id`
- `job_id`

Metrics:
- upload success/failure rate
- median and p95 time `upload_start -> BASELINE_READY`
- processing queue depth
- transcode error rates
- playback start failures

Alerts:
- baseline processing latency above threshold
- queue backlog growth
- repeated chunker/transcoder failures

## 17) Testing Strategy

### 17.1 Unit tests
- status transition validator
- share-link ID generation uniqueness
- upload size/type validators
- manifest URL generation logic

Current implementation note:
- Minimal unit tests now exist for the generated upload-path logic:
  - deterministic `public_id` generation
  - upload policy validation
  - filename sanitizing and multipart normalization helpers
- After any backend Rust code generation, run:
  - `cargo test --manifest-path backend/Cargo.toml`
- After future frontend Svelte code generation, run:
  - `npm --prefix frontend run test -- --run` once Vitest or the chosen test harness is added

### 17.2 Integration tests
- create upload session -> complete multipart -> status changes
- chunker + transcoder baseline process updates DB + artifacts
- playback endpoint gates by status correctly

### 17.3 End-to-end smoke tests
- upload sample MP4 under 1GB -> receive share link -> stream starts
- upload large file close to limit -> still process correctly
- concurrent uploads (3-10 files) -> no API starvation

### 17.4 Failure tests
- transcoder crash during transcode -> retry path works
- invalid format upload -> fails gracefully
- incomplete multipart upload -> cleanup/expiry behavior

## 18) Documentation Deliverables (Required For Exam)

Create under `/docs`:
- `architecture.md`
  - component diagram
  - sequence diagrams (upload, processing, playback)
  - scalability and race-condition notes
- `adr/` (Architecture Decision Records), at least:
  - direct-to-S3 multipart uploads
  - baseline-first processing strategy
  - DB choice and scaling plan
- `api.md`
  - endpoint contracts + example payloads
- `operational-costs.md`
  - cost centers and chosen mitigations

Include "Refer to" citations in docs:
- `references/SYSTEM_DESIGN_PROJECT_REQUIREMENTS_GUIDELINES.md` for mandatory constraints.
- `references/youtube_system_design_reference.md` for chunk/manifest/ABR primitives.
- `references/Youtube-problem-writeup.md` for scaling and pipeline deep dive rationale.

## 19) Suggested 1-Week Execution Cadence

Day 0:
- AWS account, budget alarms, IAM users/roles, initial STG infrastructure.

Day 1:
- Repo scaffolding, DB schema, core API skeleton, upload session endpoint.

Day 2:
- Multipart upload flow end-to-end with S3; shareable link generation.

Day 3:
- Chunker + baseline transcoder processing (360p HLS) and streamability transition.

Day 4:
- Svelte homepage upload flow integrated with backend, plus recent-video frontpage and share-link status page.

Day 5:
- Additional ABR renditions (`480p/720p/1080p/1440p/2160p` when source-eligible), manifest updates, CDN behavior, retry/error handling.

Day 6:
- Tests, observability hooks, concurrency checks, cost/scaling docs.

Day 7:
- Final polish, architecture doc finalization, demo script, exam submission package.

## 20) Definition Of Done

Project is done when all are true:
- A user can upload an anonymous video up to 1GB.
- A shareable link is generated and persisted.
- Visiting link eventually streams video in browser.
- Video becomes streamable at baseline quality before full processing finishes.
- System handles multiple simultaneous uploads without obvious blocking.
- Architecture document explains scaling, consistency, and cost decisions.
- Basic automated tests pass for critical paths.

## 20A) STG AWS Resource Inventory (Keep Updated)

Purpose:
- This section is the runtime source of truth for already-created AWS resources.
- Update this table whenever resource names/IDs/endpoints change.
- LLMs should read this section first before executing deployment/checklist steps.

Region baseline:
- Primary region: `ap-northeast-1`

### 20A.1 Resource Table

| Resource Type | Logical Role | Name / ID | Domain / Endpoint / URL | ARN | Notes |
|---|---|---|---|---|---|
| ALB | Public ingress for app/API | `stg-api-alb` | `stg-api-alb-816249004.ap-northeast-1.elb.amazonaws.com` |  | Temporary STG endpoint |
| CloudFront Distribution | CDN for playback artifacts |  | `d38ixt0cyn1hi6.cloudfront.net` |  | Use for playback manifest/segment URLs |
| S3 Bucket | Upload source bucket | `stg-video-upload-1a` |  | `arn:aws:s3:::stg-video-upload-1a` | Has upload-complete event notifications |
| S3 Bucket | Processed artifacts bucket |  |  |  | Fill bucket name/ARN |
| SQS Queue | Legacy/general processing queue (if retained) | `stg-video-processing` |  |  | Keep only if still used |
| SQS Queue | Chunker input queue |  |  |  | Fill exact queue name/URL/ARN |
| SQS Queue | Transcoder job queue |  |  |  | Fill exact queue name/URL/ARN |
| SQS Queue | Chunker DLQ |  |  |  | Optional |
| SQS Queue | Transcoder DLQ |  |  |  | Optional |
| VPC | STG network |  | CIDR: `10.0.0.0/16` |  | Fill VPC ID |
| Subnets | Public subnets (2 AZ) |  |  |  | Fill subnet IDs |
| Subnets | Private subnets (2 AZ) |  |  |  | Fill subnet IDs |
| Security Group | Public ingress SG | `sg-public-entry` |  |  | ALB-facing rules |
| Security Group | API SG | `sg-api` |  |  | App port from `sg-public-entry` only |
| Security Group | Processing SG | `sg-processing` |  |  | No public inbound; use separate admin SSH SG or SSM for host access |
| Security Group | DB SG | `sg-db` |  |  | 5432 only from `sg-api`/`sg-processing` |
| Security Group | Admin SSH SG | `sg-admin-ssh` |  |  | SSH from your IP only |
| EC2 Instance | Primary app host |  |  |  | Fill instance ID, private/public IP |
| EC2 Instance | Chunker host |  |  |  | Fill instance ID, private/public IP |
| EC2 Instance | Transcoder host |  |  |  | Fill instance ID, private/public IP |
| IAM Role | EC2 app host role |  |  |  | Fill role name/ARN |
| IAM Role | EC2 chunker host role |  |  |  | Fill role name/ARN |
| IAM Role | EC2 transcoder host role |  |  |  | Fill role name/ARN |

### 20A.2 Runtime Config Values (For App/Processing Integration)

| Config Key | Current Value | Notes |
|---|---|---|
| `STG_ALB_DNS` | `stg-api-alb-816249004.ap-northeast-1.elb.amazonaws.com` | App/API temporary public endpoint |
| `CDN_BASE_URL` | `https://d38ixt0cyn1hi6.cloudfront.net` | Build `manifestCdnUrl` from this |
| `PROCESSED_ASSET_BASE_URL` | `https://d38ixt0cyn1hi6.cloudfront.net` | Optional explicit playback-asset base URL; backend falls back to `CDN_BASE_URL` |
| `AWS_REGION` | `ap-northeast-1` | |
| `UPLOAD_BUCKET` | `stg-video-upload-1a` | |
| `PROCESSED_BUCKET` | `stg-video-processed-1a` | |
| `SQS_CHUNKER_QUEUE_URL` | `https://sqs.ap-northeast-1.amazonaws.com/799168734365/stg-chunker-queue` | |
| `SQS_TRANSCODER_360P_QUEUE_URL` | `https://sqs.ap-northeast-1.amazonaws.com/799168734365/stg-transcoder-360p-queue` | |
| `SQS_TRANSCODER_480P_QUEUE_URL` | `https://sqs.ap-northeast-1.amazonaws.com/799168734365/stg-transcoder-480p-queue` | |
| `SQS_TRANSCODER_720P_QUEUE_URL` | `https://sqs.ap-northeast-1.amazonaws.com/799168734365/stg-transcoder-720p-queue` | |
| `SQS_TRANSCODER_1080P_QUEUE_URL` | `https://sqs.ap-northeast-1.amazonaws.com/799168734365/stg-transcoder-1080p-queue` | |
| `SQS_TRANSCODER_1440P_QUEUE_URL` | `https://sqs.ap-northeast-1.amazonaws.com/799168734365/stg-transcoder-1440p-queue` | |
| `SQS_TRANSCODER_2160P_QUEUE_URL` | `https://sqs.ap-northeast-1.amazonaws.com/799168734365/stg-transcoder-2160p-queue` | |
| `STG_POSTGRES_DB` | `vid-archi-db` | Container bootstrap DB name |
| `STG_POSTGRES_USER` | `video-db-access` | Container bootstrap DB user |
| `STG_POSTGRES_PASSWORD` | `video-db-password` | Container bootstrap DB password |
| `STG_POSTGRES_BIND_ADDRESS` | `10.0.2.16` | Primary app EC2 private IP; publish Postgres here for chunker/transcoder host access |
| `DATABASE_URL` | `postgres://video-db-access:video-db-password@10.0.2.16:5432/vid-archi-db` | Use as `STG_DATABASE_URL`; primary app host private IP because chunker/transcoder run on a separate EC2 instance |
| `RUST_LOG` | `info` | Default structured log level; set to `debug` for verbose Rust service logs |

### 20A.3 Missing AWS Inventory Values To Collect

Still missing from the current STG inventory:
- Processed artifacts bucket:
  - bucket ARN
- Queue resources:
  - chunker queue ARN
  - transcoder 360p queue ARN
  - transcoder 480p queue ARN
  - transcoder 720p queue ARN
  - transcoder 1080p queue ARN
  - transcoder 1440p queue ARN
  - transcoder 2160p queue ARN
  - chunker DLQ name/URL/ARN if created
  - transcoder DLQ name/URL/ARN if created
- Network resources:
  - VPC ID
  - public subnet IDs
  - private subnet IDs
- Security groups:
  - `sg-public-entry` actual SG ID
  - `sg-api` actual SG ID
  - `sg-processing` actual SG ID
  - `sg-db` actual SG ID
  - `sg-admin-ssh` actual SG ID
- Compute:
  - primary EC2 instance ID
  - primary EC2 public IP
  - chunker EC2 instance ID
  - chunker EC2 private IP
  - chunker EC2 public IP if assigned
  - transcoder EC2 instance ID
  - transcoder EC2 private IP
  - transcoder EC2 public IP if assigned
- IAM:
  - EC2 app host role name
  - EC2 app host role ARN
  - EC2 chunker host role name
  - EC2 chunker host role ARN
  - EC2 transcoder host role name
  - EC2 transcoder host role ARN
- Runtime config values:
  - none remaining for app/processing DB connectivity; current STG DB URL is known

STG database access note:
- If backend and processing containers run on the same primary EC2 host as the Postgres container, use `127.0.0.1:5432` in the STG DB URL.
- If chunker/transcoder run on a separate EC2 instance, the Postgres container must listen on the primary host private interface and only allow VPC-private access from the API/processing security groups.
- In that split-host case, use the primary EC2 private IP or private DNS name in `STG_DATABASE_URL`, not `localhost`.
- Current confirmed STG shape: backend/API runs on the primary EC2 host with Postgres, while chunker and transcoder run on separate EC2 instances. Use `postgres://video-db-access:video-db-password@10.0.2.16:5432/vid-archi-db` as the STG DB URL.
- Deployment implication: the STG Postgres container can no longer bind only to `127.0.0.1`; it must listen on the primary host private interface so the chunker and transcoder EC2 instances can connect over the VPC.
- STG deploy-script rule: when bootstrapping the Postgres container, initialize it with `STG_POSTGRES_DB`, `STG_POSTGRES_USER`, and `STG_POSTGRES_PASSWORD`, publish `5432` on `STG_POSTGRES_BIND_ADDRESS`, and run Postgres with `listen_addresses='*'`.

## 21) Ordered What-To-Do-Next Checklist

This list is intentionally execution-ordered; completing all items should produce a functioning exam-compliant project.
Checked items (`[x]`) are done items.

- [x] 1a. Create monorepo folders (`backend`, `chunker`, `transcoder`, `frontend`, `docs`) and base READMEs.
- [x] 1b. Add local dev config (`.env.example`, docker-compose for Postgres/local S3 emulator if used).
- [x] 1c. Define shared constants (max upload size 1GB, allowed MIME types, baseline rendition profile).
- [x] 1d. Create shell scripts for local run and STG deployment handoff for backend/frontend services as they become runnable.
- [x] 1e. Create app-host, chunker-host, and transcoder-host deployment/start scripts for the split EC2 topology.

Current script scope:
- Host-specific deployment scripts should package artifacts and start services by default.
- Database bootstrap and schema migration execution must be explicit flags, not default deploy behavior.
- First-time DB setup should use `--bootstrap-db`.
- Full database reset should use `--reset-db` and must remain an explicit destructive action.
- Later schema changes should use `--migrate-db`.
- Local packaged-service scripts should wrap the same flow with local defaults, including MinIO startup and `.env.local` as the default env file.
- The single preferred local command remains `./scripts/local/deploy-local-lite.sh`; for real STG deployment, run app-host, chunker-host, and transcoder-host scripts on their respective EC2 instances.

First-time deployment commands:
- Local:
  - `./scripts/local/deploy-local-lite.sh --bootstrap-db`
- STG app host:
  - `./scripts/stg/deploy-stg-app-host.sh --bootstrap-db`
- STG chunker host:
  - `./scripts/stg/deploy-stg-chunker-host.sh`
- STG transcoder host:
  - `./scripts/stg/deploy-stg-transcoder-host.sh`

Operational note:
- On STG hosts, install Docker Engine plus Compose v2, then run `sudo systemctl enable --now docker` before invoking the host deploy scripts.
- If Docker's `docker-compose-plugin` conflicts with Ubuntu's `docker-compose-v2` package on STG, remove `docker-compose-v2`, run `sudo apt --fix-broken install -y`, then install `docker-compose-plugin` and re-check `docker compose version`.
- On the STG app host, backend/frontend are started as background processes rather than Docker containers; verify them with `ps -fp "$(cat logs/backend.pid)"`, `ps -fp "$(cat logs/frontend.pid)"`, and the `logs/*.log` files.
- The app-host start scripts now reclaim ports `4173` and `8080` automatically from stale same-user listeners before starting the new frontend/backend process.
- App-host deploy/start must fail fast if backend does not bind `8080` or frontend does not bind `4173`; print recent log output instead of reporting a false-success deploy.
- App-host startup should preserve prior logs while clearly marking each new attempt; append a timestamped start marker before launching backend/frontend.
- Rust deploy scripts should refresh crate metadata explicitly with `cargo fetch` before build so fresh hosts do not fail on stale or missing registry index state.
- Browser access uses `http://<STG_ALB_DNS>` when the ALB rules are ready, or `http://<APP_EC2_PUBLIC_IP>:4173` for direct frontend access before the ALB is wired.
- The Vite preview server must allow the ALB/browser `Host` header in STG; derive `preview.allowedHosts` from `STG_ALB_DNS`, `PUBLIC_API_BASE_URL`, the Vite fallback env var `__VITE_ADDITIONAL_SERVER_ALLOWED_HOSTS`, and keep the current STG ALB hostname hard-coded until deployment proves the dynamic path is fully reliable.
- App-host partial redeploys should avoid unnecessary Rust rebuilds: use the dedicated frontend deploy/run scripts for Svelte-only changes and the dedicated backend deploy/run scripts for Rust-only changes.
- Worker-host redeploys should replace previous containers deterministically; the chunker/transcoder start scripts now use Docker Compose `up -d --force-recreate --remove-orphans`.
- The STG frontend preview server must stay pinned to port `4173`; do not allow automatic port fallback because the ALB target group remains configured for `4173`.

Normal redeploy commands:
- Local:
  - `./scripts/local/deploy-local-lite.sh`
- STG app host:
  - `./scripts/stg/deploy-stg-app-host.sh`
- STG chunker host:
  - `./scripts/stg/deploy-stg-chunker-host.sh`
- STG transcoder host:
  - `./scripts/stg/deploy-stg-transcoder-host.sh`

Schema migration commands after first deploy:
- Local:
  - `./scripts/local/deploy-local-lite.sh --migrate-db`
- STG app host:
  - `./scripts/stg/deploy-stg-app-host.sh --migrate-db`

Destructive DB reset commands:
- Local:
  - `./scripts/local/deploy-local-lite.sh --reset-db`
- STG app host:
  - `./scripts/stg/deploy-stg-app-host.sh --reset-db`

- [x] 2. Implement DB schema + migrations for `videos`, `upload_sessions`, `upload_parts`, `video_renditions`, `processing_jobs`.

- [x] 3a. Implement `POST /api/videos` (create video + upload session + S3 multipart init).
- [x] 3b. Implement part-signing endpoint and complete-upload endpoint.
- [x] 3c. Implement deterministic `public_id` share-link generation and uniqueness checks.

- [x] 4a. Build frontend upload page with file validation and progress UI.
- [x] 4b. Wire frontend multipart part uploads directly to S3.
- [x] 4c. On completion, display share URL (`/v/{publicId}`).
- [x] 4d. Add anonymous delete flow with upload-time delete code and delete actions on the homepage/share page.
- [x] 4e. Add client-side cancel-upload control for in-progress multipart uploads.

Implementation note for `4a/4b/4c/4d/4e`:
- The homepage now combines both user flows: direct upload to shareable link, plus browsing recent uploads from newest to oldest before clicking into the share page.
- The homepage feed now defaults to 10 videos per page and supports previous/next pagination.
- The upload form now requires a delete code plus confirmation; the backend stores only a salted hash in `videos`.
- The upload form now exposes a cancel action during the browser-side preparation and multipart-part transfer phases; cancelling stops client requests before final completion and relies on the S3 lifecycle rule to clean up any incomplete multipart parts left behind.
- Browser refresh still does not resume an in-progress upload because multipart session state is not persisted across reloads.
- The homepage library and `/v/{publicId}` share page now both expose delete-by-code, which removes the upload-bucket prefix, processed HLS prefix, and cascaded metadata rows.
- Supporting backend read endpoints added: `GET /api/videos?page=1&pageSize=10` and `GET /api/videos/{publicId}`.
- Supporting backend delete endpoint added: `DELETE /api/videos/{publicId}`.

- [x] 5a. Build chunker/transcoder job pickup logic and status transition guardrails.
- [x] 5b. Implement baseline transcoding pipeline (360p HLS segments + 360p variant playlist).
- [x] 5c. Generate/update master manifest with baseline entry and upload artifacts to S3.
- [x] 5d. Set status `BASELINE_READY` only when baseline manifest + segments are available.

Implementation note for `5a/5b/5c/5d`:
- `chunker/` now contains the Rust service that claims parent `BASELINE` jobs and dispatches child rendition jobs.
- `transcoder/` now contains the Rust service that processes one configured rendition per container.
- Current executable shape uses a separate processing EC2 host plus Docker Compose services: `chunker` and `transcoder-<resolution>`.
- The backend now inserts the durable `BASELINE` job row and immediately publishes a chunker SQS message after multipart completion, so upload completion triggers processing through the queue path instead of worker-side DB polling.
- The chunker now consumes the chunker SQS queue, claims the referenced `BASELINE` job atomically, moves the video from `UPLOADED` to `PROCESSING_BASELINE`, downloads the source object, persists source dimensions via `ffprobe`, inserts the baseline `360p` child job into `transcoding_jobs`, and publishes the baseline transcoder SQS message.
- The chunker now commits source dimensions, the baseline child job, and any source-eligible additional-rendition jobs in one database transaction so a chunker failure cannot leave a partially dispatched baseline pipeline behind.
- The baseline transcoder now consumes the dedicated `360p` SQS queue, claims the referenced `360p` row atomically, downloads `videos/{video_id}/source/original`, generates `360p` HLS artifacts via `ffmpeg`, writes `videos/{video_id}/hls/master.m3u8`, uploads artifacts to the processed bucket, and then releases queued higher-rendition jobs onto their own rendition-specific transcoder queues once baseline playback is available.
- `BASELINE_READY` is written only after the transcoder has finished the full `360p` VOD package, uploaded the baseline playlist, uploaded the master manifest, uploaded all baseline segments, and persisted `manifest_s3_key`.
- `GET /api/videos/{publicId}` now exposes `manifestUrl` from shared metadata using `PROCESSED_ASSET_BASE_URL` or `CDN_BASE_URL`.
- The runtime assets now live inside `chunker/` and `transcoder/` so each service folder can be transferred to its own EC2 instance. Scale `chunker` replicas for dispatch pressure and scale `transcoder-360p`, `transcoder-720p`, or `transcoder-2160p` independently for rendition-specific load.

- [x] 6a. Implement playback metadata endpoint (`GET /api/videos/{publicId}/playback`).
- [x] 6b. Build frontend playback page (`/v/[publicId]`) with status polling.
- [x] 6c. Integrate browser playback (native HLS or HLS.js fallback) from manifest URL.
- [x] 6d. Enable ABR behavior in player (auto quality selection and variant switching).

Implementation note for `6a/6b/6c/6d`:
- Backend now exposes `GET /api/videos/{publicId}/playback` as the dedicated share-page/player contract.
- The playback endpoint returns:
  - `manifestUrl` for `Auto` ABR playback against the master manifest
  - `availableQualities[]` built from ready `video_renditions` rows, each with its own fixed-quality `playlistUrl`
  - `pollIntervalMs` so the frontend can keep polling while more renditions are still processing
- The endpoint contract is derived only from shared Postgres metadata, so the player never races ahead of chunker/transcoder progress held in worker-local memory.
- Playback quality `width` and `height` now come from persisted transcoder output metadata in `video_renditions`, not from assumed ladder dimensions.
- The schema now also persists target video/audio bitrate metadata for each rendition so step `7` can continue from a more precise source of truth.
- The frontend playback page now polls that endpoint until the video reaches `READY` or `FAILED`.
- The browser player now supports:
  - native HLS where available
  - HLS.js fallback loaded from CDN for browsers that need JavaScript HLS playback
  - `Auto` quality by loading the master manifest URL
  - fixed-resolution quality selection by rebuilding the player against the selected rendition playlist URL so the entire stream follows the locked quality after the swap
- This design is intentionally aligned with step `7`: once new rendition rows such as `720p` or `1080p` are persisted and the master manifest is expanded, the playback endpoint and player do not need a redesign; they will surface the new options automatically.

- [x] 7a. Implement additional-renditions transcoding pipeline for source-eligible `480p/720p/1080p/1440p/2160p` ABR variants without upscaling.
- [x] 7b. Update master manifest safely and transition final status to `READY`.
- [x] 7c. Persist additional-rendition metadata in `video_renditions` and keep actual output dimensions/bitrates aligned with manifest updates.
- [x] 7d. Configure CDN cache behavior for manifests and segments; validate first-play path via CloudFront URLs.

Implementation note for `7a/7b/7c/7d`:
- `chunker/` now reads the shared adaptive ladder, keeps the baseline `360p` dispatch, and also inserts one `ADDITIONAL_RENDITIONS` parent job plus source-eligible higher-rendition rows into `transcoding_jobs`.
- Non-baseline transcoders do not start until the baseline path has already made the video streamable. The first additional-renditions claim moves the video from `BASELINE_READY` to `PROCESSING_FULL`.
- Every successful rendition rebuilds `videos/{video_id}/hls/master.m3u8` from the current set of ready renditions, uploads the refreshed master manifest, and keeps `Auto` playback aligned with fixed-quality playlist URLs.
- Additional renditions now persist `codec`, `container`, `playlist_key`, actual encoded `output_width/output_height`, target video/audio bitrate metadata, and `segment_count` in `video_renditions` before their final `READY` transition.
- When the last planned source-eligible rendition finishes, the video moves to `READY` and the `ADDITIONAL_RENDITIONS` parent job is marked `SUCCEEDED`.
- For low-resolution source files where no additional rendition is eligible, baseline completion now moves the video directly to `READY`.
- `7d` is satisfied in code by constructing playback URLs from `CDN_BASE_URL` / `PROCESSED_ASSET_BASE_URL` and by keeping HLS playlist and segment paths relative under `videos/{video_id}/hls/...`, which allows CloudFront to serve both `*.m3u8` and `*.ts` artifacts correctly.
- If your CloudFront distribution already has a short-TTL behavior for `*.m3u8` and a longer-cache behavior for segment files such as `*.ts` (and optionally `*.m4a` / `*.m4s` for future packaging changes), there is nothing else mandatory for `7d`.

- [x] 8a. Add idempotent retry policy for chunker/transcoder failures.
- [x] 8b. Add terminal failure handling (`FAILED` state, error message surfaces in UI/API).
- [x] 8c. Add abandoned multipart cleanup path (scheduled task or lifecycle policy docs + config).

Implementation note for `8a/8b/8c`:
- `chunker` now retries failed baseline dispatch work by inserting a new `processing_jobs` attempt up to `CHUNKER_MAX_PROCESSING_ATTEMPTS`; retries are idempotent because each attempt uses a unique `(video_id, job_type, attempt)` key.
- Baseline dispatch is now also atomic inside a single DB transaction for source dimensions plus child-job fanout, which closes the partial-dispatch race between chunker failure handling and transcoder job pickup.
- Baseline retry dispatch also inserts the child baseline `transcoding_jobs` row using the same retry attempt number, so later attempts do not collide with the original `360p` job row.
- `transcoder` now retries failed rendition work by inserting a new `transcoding_jobs` attempt up to `TRANSCODER_MAX_ATTEMPTS`; this keeps finished attempts immutable and avoids reusing a partially failed row.
- `GET /api/videos/{publicId}` and `GET /api/videos/{publicId}/playback` now expose `errorCode` and `errorMessage`.
- `errorMessage` is a user-facing summary intended for the default share page; raw worker/ffmpeg/ffprobe error text remains in `processing_jobs.error` and `transcoding_jobs.error` for debugging.
- The share page at `/v/{publicId}` now renders terminal failure details and, when the baseline rendition already exists, continues to allow playback even though the video reports `status=FAILED`.
- Abandoned multipart uploads are now covered operationally by an S3 lifecycle rule on the upload bucket that aborts incomplete multipart uploads after `1` day.

- [x] 9a. Add structured logging and correlation IDs across API and processing services.
- [ ] 9b. Add metrics for time-to-stream, queue depth, processing success/failure.
- [ ] 9c. Add health/readiness endpoints for API, chunker, and transcoder.

Implementation note for `9a`:
- `backend`, `chunker`, and `transcoder` now emit structured JSON logs through `tracing`.
- The API now accepts an optional `x-correlation-id` header, generates one when absent, and echoes it in the response.
- `processing_jobs` and `transcoding_jobs` now persist `correlation_id`, allowing the upload request, chunker work, and transcoder work to be followed through one shared identifier.
- `chunker` and `transcoder` now run claimed jobs inside spans that include `correlation_id` plus the relevant job/video fields.
- The queue transport is now the live execution trigger: backend publishes baseline jobs to `SQS_CHUNKER_QUEUE_URL`, chunker publishes rendition jobs to the matching rendition-specific queue (`SQS_TRANSCODER_360P_QUEUE_URL` through `SQS_TRANSCODER_2160P_QUEUE_URL`), and both workers use Postgres claims as the duplicate-delivery guardrail for standard SQS.
- Worker queue consumers now delete invalid queue payloads instead of retry-looping them. This keeps the runtime resilient to stale or malformed messages without carrying special-case parsing branches for specific foreign schemas.
- Each transcoder service now receives the full set of rendition queue URLs so a completed baseline worker can publish follow-up rendition jobs to their correct queues instead of reusing only its own consumer queue URL.
- Transcoder workers now self-heal any misrouted rendition message by re-enqueueing it onto the correct rendition-specific queue and deleting the stale copy from the wrong queue. This prevents old queue state from causing infinite release/retry loops after the queue topology change.
- The playback page now hides raw internal status terms from end users. The title appears above the player, the video is visible immediately without scrolling, and metadata such as upload time, baseline-ready time, full-processing time, share link, and delete controls live below the player in user-facing language.
- Manual playback quality selection now forces a real rendition reload for both playback engines: choosing `720p` or `1080p` rebuilds just the player against that rendition playlist, preserving playback position when possible and ensuring rewinds or seeks back to the beginning stay on the locked quality until the user returns to `Auto`.
- The app-host backend deploy script now stages artifacts into temporary files inside `backend/dist` and atomically renames them into place. This avoids Linux `Text file busy` failures when packaging a new release while the previous backend binary is still running from the same path.
- The transcoder S3 publish path now logs each processed artifact upload with bucket, key, local file path, and file size so baseline stalls can be distinguished between ffmpeg packaging and processed-bucket publication.
- The transcoder now uploads rendition segments before the variant playlist and retries/times out individual processed-bucket uploads so transient S3 publish failures do not immediately strand a baseline rendition in `PROCESSING_BASELINE` after ffmpeg has already finished.
- The transcoder now uploads HLS artifacts from in-memory bytes with explicit `content_length` instead of path-streamed request bodies; this is a pragmatic STG hardening step for small segment uploads that were hanging on the Rust S3 client path even though host-side `aws s3 cp` to the same bucket succeeded.
- The STG transcoder image now includes `awscli`, and the artifact uploader falls back to `aws s3 cp` when the Rust S3 client times out or returns an upload error. This keeps the application state transitions in Rust while using the same EC2 instance role credentials for a more reliable publish path under take-home time constraints.
- STG worker Compose definitions now use `network_mode: host` for `chunker` and `transcoder-*` so S3/DB traffic uses the EC2 host network path directly instead of Docker bridge/NAT.
- The runtime scripts now default `RUST_LOG` to `info`; set `RUST_LOG=debug` when deeper troubleshooting is needed.
- The schema remains consolidated into the single bootstrap migration `backend/migrations/0001_initial_schema.up.sql` so first-time deployment still requires only one SQL file.

- [ ] 10a. Add integration tests for upload->process->playback happy path.
- [ ] 10b. Add concurrency test for multiple simultaneous uploads.
- [ ] 10c. Add failure-path tests (bad format, transcoder retry, incomplete upload).

Schedule note:
- `9b`, `9c`, `10a`, `10b`, and `10c` are intentionally deferred for the current delivery window and remain open follow-up work.

- [x] 11a. Write `docs/architecture.md` with component and sequence diagrams.
- [x] 11b. Write `docs/api.md` and include request/response examples.
- [x] 11c. Write `docs/operational-costs.md` with S3/CDN/lifecycle decisions and tradeoffs.
- [x] 11d. Add explicit citations to files under `references/` for requirement traceability.
- [x] 11e. Document STG deployment topology, resource list, and monthly cost guardrails.
- [x] 11f. Add ADR/note: API Gateway omitted in current STG for cost; include benefits and upgrade path to production-like ingress.

Implementation note for `11a/11b/11c/11d/11e/11f`:
- `docs/architecture.md` now includes a component diagram, an upload-to-first-play sequence diagram, the delete-by-code cleanup path, the structured logging model, the current STG topology, and the ingress note for the current ALB-only path.
- `docs/api.md` now documents the implemented endpoints with concise request/response examples, including the `x-correlation-id` behavior and the delete-video contract.
- `docs/operational-costs.md` now captures the current STG footprint, cost-control decisions, and monthly guardrails.
- `docs/stg-deployment.md` now consolidates the first-time STG deployment steps, ALB listener/target-group setup, environment templates, and redeploy commands for the current app-host plus worker-host layout.
- Root and service READMEs were rewritten in a production-style tone so the repository reads like an operating application rather than an internal exercise bundle.
- `docs/architecture.md` and `docs/api.md` now both include explicit "Refer to" links back to the requirement and system-design references under `references/`.

- [ ] 12a. Run final end-to-end demo scenario and capture expected outputs.
- [ ] 12b. Verify the requirement checklist in Section 3 item-by-item.
- [ ] 12c. Prepare final submission notes describing what is implemented vs documented tradeoffs.

## 22) AWS Checklist (0-Step Items)

Checked items (`[x]`) are done items.

- [x] 0a. Create AWS account and secure it (root MFA, IAM admin user, least-privilege app roles).
- [x] 0b. Configure AWS Budgets + billing alerts before provisioning resources.
- [ ] 0c. Create STG networking baseline (complete all `0c.x` items below).
- [x] 0c.1 Create 1 VPC for STG (example CIDR: `10.0.0.0/16`).
- [x] 0c.2 Create 2 public subnets (different AZs) for internet-facing ingress.
- [x] 0c.3 Create 2 private subnets (different AZs) for API app, processing, and DB.
- [x] 0c.4 Attach Internet Gateway to VPC and create public route table: `0.0.0.0/0 -> IGW`.
- [x] 0c.5 Associate public subnets to public route table; associate private subnets to private route table.
- [ ] 0c.6 Ensure only ingress layer is public:
- [ ] 0c.6a Public path components: `CloudFront` + (`API Gateway` and/or `ALB`).
- [ ] 0c.6b Private components: API service tasks/instances, chunker/transcoder containers, Postgres, internal queues.
- [x] 0c.7 Create security groups with least privilege:
- [x] 0c.7a `sg-public-entry`: inbound `443` from `0.0.0.0/0` (and optional `80` redirect).
- [x] 0c.7b `sg-api`: inbound app port only from `sg-public-entry`.
- [x] 0c.7c `sg-processing`: no inbound from internet for application traffic; use separate admin SSH SG or SSM if host access is needed.
- [x] 0c.7d `sg-db`: inbound `5432` only from `sg-api` and `sg-processing`.
- [ ] 0c.8 Set public accessibility rules for review:
- [ ] 0c.8a Public: website domain, upload/status/playback API routes, CloudFront video paths (`*.m3u8`, `*.ts`/`*.m4s`).
- [ ] 0c.8b Private: DB endpoints, processing endpoints, S3 buckets (no public bucket/object access).
- [x] 0c.8c If company provides fixed egress CIDRs, add optional IP allowlist via WAF for review paths.
- [ ] 0c.9 Validate networking before app deploy:
- [ ] 0c.9a Confirm public URL reachability from external network.
- [ ] 0c.9b Confirm private resources are unreachable from internet.
- [ ] 0c.9c Confirm API can reach DB and processing containers can reach S3/SQS.
- [x] 0d. Provision storage and queue primitives (S3 upload bucket, S3 processed bucket, SQS queue, optional DLQ).
- [x] 0d.1 Add an S3 lifecycle rule on the upload bucket to abort incomplete multipart uploads after `1` day.
- [x] 0e. Provision compute + database for STG:
  - `Current STG`: app host + chunker host + transcoder host + Postgres container on app host.
  - `STG-Prod-Like`: API service + processing services + managed Postgres (RDS).
- [x] 0e.1 Launch primary STG EC2 instance for app services (API + frontend + Postgres container).
- [x] 0e.2 Install Docker and run Postgres container on EC2 (`postgres:16`) with persistent volume.
- [x] 0e.3 Keep Postgres access private (bind `127.0.0.1:5432` or private subnet only).
- [ ] 0e.4 Configure app-host and processing-host env vars to use the shared Postgres connection string.
- [ ] 0e.5 Document DB backup/recovery plan for containerized Postgres (`pg_dump` + restore workflow, optional EBS snapshots).
- [ ] 0e.6 Document that RDS Postgres is the production-like replacement when budget/ops requirements increase.
- [x] 0e.7 (Scale-out option) Provision separate compute for processing pools:
  - `chunker` instance/service
  - `transcoder` instance/service(s)
- [x] 0e.8 (Scale-out option) Use separate queue stages for processing:
  - upload-complete -> chunker queue
  - chunker output -> transcoder queue (per rendition jobs)
- [ ] 0e.8 (Scale-out option) Configure autoscaling triggers for processing pools based on queue depth.
- [x] 0f. Provision ingress components for STG (API Gateway HTTP API and/or ALB) and route API traffic.
- [x] 0f.1 Current STG ALB DNS endpoint (temporary public endpoint for app/API access):
  - `stg-api-alb-816249004.ap-northeast-1.elb.amazonaws.com`
- [x] 0g. Provision CloudFront distribution in front of processed-video bucket.
- [x] 0g.1 Current STG CloudFront distribution domain (temporary CDN endpoint for playback):
  - `d38ixt0cyn1hi6.cloudfront.net`

## 23) Optional Upgrades (Only If Time Remains)

- S3 event-driven auto-triggering (instead of API-triggered enqueue only).
- Queue service hardening (SQS visibility timeout tuning, dead-letter queues).
- Thumbnail extraction and poster image.
- Signed playback URLs for private mode.
- Multi-region read strategy and cache invalidation refinement.
