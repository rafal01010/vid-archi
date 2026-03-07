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

## 3) Exam Requirement Checklist (Must Pass)

These are non-negotiable outcomes:
- Users can upload video files (up to 1GB).
- Support common formats (at least MP4/MOV/WebM input).
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
/worker             # Rust background processor (ffmpeg pipeline, manifest generation, status transitions)
/frontend           # Svelte app (upload page + playback page)
/docs               # Architecture docs + ADRs + API contracts + operational notes
/references         # Provided requirement/reference docs
```

Recommended runtime stack:
- Backend: Rust + Axum + SQLx (or Diesel) + Tokio.
- Worker: Rust + Tokio + ffmpeg invocation.
- Frontend: SvelteKit + HTML5 video (HLS.js if needed).
- Object storage: AWS S3.
- Metadata DB: Postgres (MVP). Cost-optimized STG-Lite can run Postgres in a Docker container on EC2; production-like target is managed Postgres (RDS).
- Async trigger: SQS queue (or S3 event -> webhook endpoint for MVP).
- CDN: CloudFront in front of S3 for manifests/segments.

STG deployment profile options:
- `STG-Lite (lowest cost, recommended first)`:
  - 1 small VM/container host for API + worker.
  - Postgres as a Docker container on the same host (cost-first setup).
  - S3 + CloudFront + SQS.
  - ALB as internet ingress; API Gateway intentionally omitted in STG-Lite to stay within interview budget.
  - No multi-AZ and minimal autoscaling.
- `STG-Prod-Like (higher confidence, higher cost)`:
  - API on ECS/EC2 behind ALB.
  - API Gateway HTTP API in front of ALB.
  - Worker split into separate services/instance pools:
    - `Chunker` pool (extracts streamable clips / packaging steps).
    - `Transcoder` pool (per-rendition jobs such as 360p/720p/1080p).
  - Managed Postgres (RDS), S3, CloudFront, SQS.
  - Optional autoscaling group for API.

STG-Lite database note:
- For interview-budget environments, prefer local Postgres container on EC2.
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
- Ideal future upgrade path: move to RDS Postgres when higher availability/ops isolation is required.

API Gateway decision note:
- Current plan intentionally omits API Gateway in STG-Lite for cost control and reduced setup complexity.
- STG-Lite ingress path is `Client -> ALB -> API service`.
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
- `Processing Worker (Rust + ffmpeg)`
  - Triggered after upload completion.
  - Baseline rendition first (360p).
  - Mark streamable when baseline manifest exists.
  - Continue higher renditions.
- `Metadata DB`
  - Source of truth for video lifecycle state.
  - State transitions and manifest pointers.
  - STG-Lite implementation: Postgres container on EC2 host.
  - Production-like implementation: RDS Postgres in private subnets.
- `CDN (CloudFront)`
  - Cache manifests/segments for stream performance and lower egress costs.

Critical design principles:
- Direct-to-S3 upload (no large-file pass-through API servers).
- Separate upload chunks from playback segments.
- State machine with `BASELINE_READY` as stream gate.
- Shared durable metadata store to keep multi-instance behavior consistent.

## 6) Video Lifecycle State Machine

Use explicit states in DB:
- `INITIATED` (video record created, session open)
- `UPLOADING` (multipart in progress)
- `UPLOADED` (S3 object completed)
- `PROCESSING_BASELINE` (360p rendition in progress)
- `BASELINE_READY` (streamable; primary exam success condition)
- `PROCESSING_FULL` (additional renditions ongoing)
- `READY` (all planned renditions complete)
- `FAILED` (terminal error after retries)

State transition rules:
- Do not return `streamable=true` until manifest exists and `BASELINE_READY` or `READY`.
- Transition writes must be atomic and idempotent.
- Worker should tolerate duplicate events.

## 7) Data Model (Minimal But Complete)

### 7.1 `videos`

Fields:
- `id` (UUID internal id)
- `public_id` (short share slug, unique)
- `title` (nullable)
- `original_filename`
- `content_type`
- `size_bytes`
- `source_s3_key`
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
- `rendition` (`360p|720p|1080p`)
- `codec` (e.g. h264)
- `container` (e.g. fmp4/ts)
- `playlist_key`
- `status` (`PROCESSING|READY|FAILED`)
- `segment_count`
- `created_at`, `updated_at`

Constraint:
- unique(`video_id`, `rendition`)

### 7.5 `processing_jobs`

Fields:
- `id`
- `video_id`
- `job_type` (`BASELINE|ENHANCE`)
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
  - `videos/{video_id}/hls/720p/...`
  - `videos/{video_id}/hls/1080p/...`
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
- `contentType`
- `sizeBytes` (validate `<= 1GB`)
- optional `title`

Response:
- `videoId`
- `publicId`
- `uploadSessionId`
- `s3UploadId`
- `partSizeBytes`
- `presignedPartUrlTemplate` or first batch URLs
- `completeUploadEndpoint`

### 9.2 Sign parts (if not pre-generated)

`POST /api/videos/{videoId}/parts/sign`

Request:
- list of `partNumbers`

Response:
- `[{ partNumber, url }]`

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
- Enqueue baseline processing.

### 9.5 Get video status/details

`GET /api/videos/{publicId}`

Response:
- `publicId`
- `status`
- `isStreamable`
- `playbackUrl` (frontend route)
- `manifestUrl` (if streamable)

### 9.6 Playback entrypoint (for player page)

`GET /api/videos/{publicId}/playback`

Response:
- `status`
- `manifestCdnUrl` when streamable
- optional `posterUrl`

### 9.7 Health/readiness

`GET /healthz`
- Includes DB, S3 reachability checks.

## 10) Upload And Stream Flows

### 10.1 Upload flow (anonymous + large-file-safe)

1. Frontend requests create-upload session.
2. Backend validates size/type and inserts `videos + upload_sessions`.
3. Backend creates multipart upload in S3 and returns signing data.
4. Frontend uploads parts directly to S3.
5. Frontend calls complete endpoint with ETags.
6. Backend completes multipart, transitions to `UPLOADED`, enqueues worker job.

### 10.2 Processing flow (time-to-stream first)

1. Worker picks baseline job.
2. ffmpeg transcodes source into 360p segmented HLS (baseline ABR variant) and creates variant playlist.
3. Upload manifest/segments to S3.
4. Create/update master manifest with 360p entry only.
5. DB transition to `BASELINE_READY` and set `is_streamable=true` (this is the streamability gate).
6. Worker enqueues enhancement job for 720p/1080p ABR variants.
7. On completion, update master manifest with higher variants and set `READY`.

### 10.3 Playback flow

1. User opens `/v/{publicId}` in browser.
2. Frontend calls playback endpoint.
3. If `BASELINE_READY/READY`, player fetches master manifest and begins streaming with 360p available immediately.
4. Player performs ABR variant switching as network/device conditions change (via HLS client behavior).
5. If not ready, show processing state and poll.

## 11) Frontend Plan (Svelte)

Pages:
- `/` upload page
- `/v/[publicId]` playback page

Upload page features:
- file picker + drag/drop
- client-side size/type pre-validation
- multipart progress bar
- error/retry states
- show generated share link on completion

Playback page features:
- status polling until streamable
- video player using HLS manifest URL
- fallback for unsupported browsers
- clear state labels: `Uploading`, `Processing`, `Ready`, `Failed`

UX rules aligned to exam:
- Functional UI is enough; avoid over-investing in styling.
- Ensure "time-to-first-play" visibly prioritized once baseline ready.

## 12) Worker/Transcoding + ABR Plan

ffmpeg strategy for MVP:
- Input: source S3 object (download to local temp file).
- Output baseline quickly:
  - 360p, moderate bitrate, short segment duration (2-4s), H.264 + AAC.
- Generate HLS artifacts:
  - variant playlists (`360p.m3u8`, then `720p.m3u8`, `1080p.m3u8`)
  - master playlist (`master.m3u8`) for ABR.
- Generate master manifest immediately with baseline entry only.
- Later add 720p and 1080p variants and update master manifest.

ABR ladder for MVP:
- 360p baseline (first streamable target)
- 720p standard
- 1080p higher quality

Why this matters:
- ABR is essential for low-bandwidth and device diversity: players can switch rendition without restarting playback.
- Time-to-stream remains prioritized because 360p is produced first and gates streamability.

Worker execution model:
- Poll queue (SQS) or DB job table.
- Lock job atomically (`RUNNING`) to avoid duplicate workers.
- Retry transient failures with capped attempts + backoff.
- Move permanent failures to `FAILED` with error details.

Processing deployment modes:
- `STG-Lite` (budget): one worker process/instance performs chunking + transcoding stages.
- `Scale-out mode` (diagram-aligned): separate chunker and transcoder pools.
  - Ingest queue/event triggers chunker.
  - Chunker emits per-rendition transcode jobs.
  - Multiple transcoder instances consume jobs in parallel (horizontal scale).
  - This can be deployed as EC2 instance groups or ECS services.

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
- Keep rendition set small for MVP (360p, 720p, 1080p only).
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
- Start with `STG-Lite` and scale only after demo stability.
- Add billing alarms/budgets on day zero.
- Keep managed services at smallest tiers; disable idle resources when not testing.

## 15) Security And Abuse Baseline

Even without auth:
- Validate file size and allowed content types both client and server side.
- Limit presigned URL expiration (short TTL).
- Scope presigned permissions to exact object key/session.
- Rate limit session creation by IP.
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
- repeated worker failures

## 17) Testing Strategy

### 17.1 Unit tests
- status transition validator
- share-link ID generation uniqueness
- upload size/type validators
- manifest URL generation logic

### 17.2 Integration tests
- create upload session -> complete multipart -> status changes
- worker baseline process updates DB + artifacts
- playback endpoint gates by status correctly

### 17.3 End-to-end smoke tests
- upload sample MP4 under 1GB -> receive share link -> stream starts
- upload large file close to limit -> still process correctly
- concurrent uploads (3-10 files) -> no API starvation

### 17.4 Failure tests
- worker crash during transcode -> retry path works
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
- Worker baseline processing (360p HLS) and streamability transition.

Day 4:
- Svelte upload + playback pages integrated with backend.

Day 5:
- Additional ABR renditions (720p/1080p), manifest updates, CDN behavior, retry/error handling.

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
| Security Group | Worker SG | `sg-worker` |  |  | No public inbound |
| Security Group | DB SG | `sg-db` |  |  | 5432 only from `sg-api`/`sg-worker` |
| Security Group | Admin SSH SG | `sg-admin-ssh` |  |  | SSH from your IP only |
| EC2 Instance | Primary app/worker host |  |  |  | Fill instance ID, private/public IP |
| EC2 Instance | Chunker host/service (if separate) |  |  |  | Fill if provisioned separately |
| EC2 Instance | Transcoder host/service (if separate) |  |  |  | Fill if provisioned separately |
| IAM Role | EC2 app/worker role |  |  |  | Fill role name/ARN |

### 20A.2 Runtime Config Values (For App/Worker Integration)

| Config Key | Current Value | Notes |
|---|---|---|
| `STG_ALB_DNS` | `stg-api-alb-816249004.ap-northeast-1.elb.amazonaws.com` | App/API temporary public endpoint |
| `CDN_BASE_URL` | `https://d38ixt0cyn1hi6.cloudfront.net` | Build `manifestCdnUrl` from this |
| `AWS_REGION` | `ap-northeast-1` | |
| `UPLOAD_BUCKET` | `stg-video-upload-1a` | |
| `PROCESSED_BUCKET` |  | Fill value |
| `SQS_CHUNKER_QUEUE_URL` |  | Fill value |
| `SQS_TRANSCODER_QUEUE_URL` |  | Fill value |
| `DATABASE_URL` |  | Fill value when app wiring starts |

## 21) Ordered What-To-Do-Next Checklist

This list is intentionally execution-ordered; completing all items should produce a functioning exam-compliant project.
Checked items (`[x]`) are done items.

- [ ] 1a. Create monorepo folders (`backend`, `worker`, `frontend`, `docs`) and base READMEs.
- [ ] 1b. Add local dev config (`.env.example`, docker-compose for Postgres/local S3 emulator if used).
- [ ] 1c. Define shared constants (max upload size 1GB, allowed MIME types, baseline rendition profile).

- [ ] 2. Implement DB schema + migrations for `videos`, `upload_sessions`, `upload_parts`, `video_renditions`, `processing_jobs`.

- [ ] 3a. Implement `POST /api/videos` (create video + upload session + S3 multipart init).
- [ ] 3b. Implement part-signing endpoint and complete-upload endpoint.
- [ ] 3c. Implement deterministic `public_id` share-link generation and uniqueness checks.

- [ ] 4a. Build frontend upload page with file validation and progress UI.
- [ ] 4b. Wire frontend multipart part uploads directly to S3.
- [ ] 4c. On completion, display share URL (`/v/{publicId}`).

- [ ] 5a. Build worker job pickup logic and status transition guardrails.
- [ ] 5b. Implement baseline transcoding pipeline (360p HLS segments + 360p variant playlist).
- [ ] 5c. Generate/update master manifest with baseline entry and upload artifacts to S3.
- [ ] 5d. Set status `BASELINE_READY` only when baseline manifest + segments are available.

- [ ] 6a. Implement playback metadata endpoint (`GET /api/videos/{publicId}/playback`).
- [ ] 6b. Build frontend playback page (`/v/[publicId]`) with status polling.
- [ ] 6c. Integrate browser playback (native HLS or HLS.js fallback) from manifest URL.
- [ ] 6d. Enable ABR behavior in player (auto quality selection and variant switching).

- [ ] 7a. Implement enhancement transcoding pipeline for 720p/1080p ABR variants.
- [ ] 7b. Update master manifest safely and transition final status to `READY`.
- [ ] 7c. Persist rendition metadata in `video_renditions`.
- [ ] 7d. Configure CDN cache behavior for manifests and segments; validate first-play path via CloudFront URLs.

- [ ] 8a. Add idempotent retry policy for worker failures.
- [ ] 8b. Add terminal failure handling (`FAILED` state, error message surfaces in UI/API).
- [ ] 8c. Add abandoned multipart cleanup path (scheduled task or lifecycle policy docs + config).

- [ ] 9a. Add structured logging and correlation IDs across API and worker.
- [ ] 9b. Add metrics for time-to-stream, queue depth, processing success/failure.
- [ ] 9c. Add health/readiness endpoints for API and worker process.

- [ ] 10a. Add integration tests for upload->process->playback happy path.
- [ ] 10b. Add concurrency test for multiple simultaneous uploads.
- [ ] 10c. Add failure-path tests (bad format, worker retry, incomplete upload).

- [ ] 11a. Write `docs/architecture.md` with component and sequence diagrams.
- [ ] 11b. Write `docs/api.md` and include request/response examples.
- [ ] 11c. Write `docs/operational-costs.md` with S3/CDN/lifecycle decisions and tradeoffs.
- [ ] 11d. Add explicit citations to files under `references/` for requirement traceability.
- [ ] 11e. Document STG deployment topology, resource list, and monthly cost guardrails.
- [ ] 11f. Add ADR/note: API Gateway omitted in STG-Lite for cost; include benefits and upgrade path to production-like ingress.

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
- [x] 0c.3 Create 2 private subnets (different AZs) for API app, worker, and DB.
- [x] 0c.4 Attach Internet Gateway to VPC and create public route table: `0.0.0.0/0 -> IGW`.
- [x] 0c.5 Associate public subnets to public route table; associate private subnets to private route table.
- [ ] 0c.6 Ensure only ingress layer is public:
- [ ] 0c.6a Public path components: `CloudFront` + (`API Gateway` and/or `ALB`).
- [ ] 0c.6b Private components: API service tasks/instances, worker, Postgres, internal queues.
- [x] 0c.7 Create security groups with least privilege:
- [x] 0c.7a `sg-public-entry`: inbound `443` from `0.0.0.0/0` (and optional `80` redirect).
- [x] 0c.7b `sg-api`: inbound app port only from `sg-public-entry`.
- [x] 0c.7c `sg-worker`: no inbound from internet.
- [x] 0c.7d `sg-db`: inbound `5432` only from `sg-api` and `sg-worker`.
- [ ] 0c.8 Set public accessibility rules for review:
- [ ] 0c.8a Public: website domain, upload/status/playback API routes, CloudFront video paths (`*.m3u8`, `*.ts`/`*.m4s`).
- [ ] 0c.8b Private: DB endpoints, worker endpoints, S3 buckets (no public bucket/object access).
- [x] 0c.8c If company provides fixed egress CIDRs, add optional IP allowlist via WAF for review paths.
- [ ] 0c.9 Validate networking before app deploy:
- [ ] 0c.9a Confirm public URL reachability from external network.
- [ ] 0c.9b Confirm private resources are unreachable from internet.
- [ ] 0c.9c Confirm API can reach DB and worker can reach S3/SQS.
- [x] 0d. Provision storage and queue primitives (S3 upload bucket, S3 processed bucket, SQS queue, optional DLQ).
- [x] 0e. Provision compute + database for STG:
  - `STG-Lite`: single API/worker host + Postgres container (cost-first).
  - `STG-Prod-Like`: API service + worker service + managed Postgres (RDS).
- [x] 0e.1 Launch primary STG EC2 instance for app services (API + worker).
- [x] 0e.2 Install Docker and run Postgres container on EC2 (`postgres:16`) with persistent volume.
- [x] 0e.3 Keep Postgres access private (bind `127.0.0.1:5432` or private subnet only).
- [ ] 0e.4 Configure API/worker env vars to use containerized Postgres connection string.
- [ ] 0e.5 Document that RDS Postgres is the production-like replacement when budget/ops requirements increase.
- [x] 0e.6 (Scale-out option) Provision separate compute for processing pools:
  - `chunker` instance/service
  - `transcoder` instance/service(s)
- [x] 0e.7 (Scale-out option) Use separate queue stages for processing:
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
