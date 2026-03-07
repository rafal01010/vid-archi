# Reference: Video Hosting & Streaming Architecture (YouTube-style)

Purpose of this document: **LLM-optimized reference** distilled from a long-form system design walkthrough.  
It prioritizes **architecture primitives, data contracts, and design decisions** over narrative.

---

## 1) Problem Definition

Design a video platform that supports:
- **Upload** of large videos (up to very large files; example max: **256 GB**).
- **Watch/stream** videos with low startup latency and good experience under varying bandwidth.
- **Scale** to high volumes (example scale targets used in the walkthrough):
  - ~**1M uploads/day**
  - ~**100M daily active users (views/streams)**

This is a starting design for “YouTube/Netflix/Hulu”-like systems focused on **upload + playback**. (Comments, likes, search, recommendations, etc. are extensions.)

---

## 2) Requirements

### 2.1 Functional requirements
- Users can **upload** videos.
- Users can **watch/stream** videos.

### 2.2 Non-functional requirements
- **Availability over consistency** (eventual consistency is acceptable):
  - If a user uploads in region A, users in region B **do not** need to see it instantly.
  - It is acceptable that a video becomes watchable only after async processing completes.
- **Low-latency startup**:
  - Aim for “first pixels” quickly (example target: **< 500 ms**).
  - Must still work reasonably on **low bandwidth**.
- **Scalability**:
  - Handle upload + playback at the given scale.
- **Support very large videos**:
  - Upload and playback must not require single huge requests nor full-file downloads.

---

## 3) Core Entities (Domain Nouns)

### 3.1 User
Minimal placeholder entity:
- `user_id`
- (profile fields omitted)

### 3.2 Video “bytes” vs Video metadata
Treat as **two distinct domains**:
1) **Video bytes**: the actual media content stored in **blob/object storage** (e.g., S3/GCS).
2) **Video metadata**: relational/noSQL record used for listing and playback decisions.

### 3.3 VideoMetadata (example fields)
> Keep metadata small; store heavy media in object storage / CDN.

- `video_id` (primary key)
- `uploader_user_id`
- `title`, `description`
- `created_at`
- `upload_status`: `PENDING | UPLOADED | PROCESSING | READY | FAILED`
- `source_object_url` (object storage URL for the *stitched* source file)
- `manifest_url` (object storage / CDN URL to the manifest)
- `renditions` (structured mapping of **resolution/bitrate** → list of chunk URLs)
- Optional:
  - `duration`, `fps`, `aspect_ratio`
  - `visibility` / access control flags

---

## 4) External API Surface (Contracts)

Use these as “public-facing API primitives”; implementation may evolve.

### 4.1 Upload initiation (metadata first, then direct-to-object-store upload)
**POST** `/videos`
- Request:
  - `metadata` (title/description/etc.)
  - `content_length` (video size)
  - (optionally: file type/codec hints)
- Response:
  - `video_id`
  - `upload_session_id`
  - `presigned_part_urls[]` (or an abstraction like “upload instructions”)

Rationale: large file uploads **cannot** go through typical API gateway POST body limits (example: 10MB).

### 4.2 Upload completion
Option A (preferred): **event-driven** completion via object store notifications  
Option B (fallback): client calls `POST /videos/{video_id}/complete` with upload token

> The walkthrough prefers **not trusting the client** to confirm upload completion.

### 4.3 Get video (metadata + playback entrypoints)
**GET** `/videos/{video_id}`
- Response:
  - `metadata` (title, description, uploader, etc.)
  - `manifest_url` (preferably CDN-backed)
  - (optionally) `playback_policy` / allowed renditions

Client then streams using manifest + chunks.

---

## 5) High-Level Components

- **Client** (web/mobile)
- **API Gateway**
  - routing to services
  - auth, rate limits, request validation
- **Video Service** (stateless)
  - creates VideoMetadata
  - orchestrates upload session creation
- **Video Metadata DB**
  - DynamoDB / Postgres, etc. (small records)
- **Object Storage** (S3/GCS)
  - stores source upload and all derived chunks/renditions
- **Object Storage Notifications**
  - triggers downstream processing when upload completes
- **Chunker workers** (stateless)
  - segments uploaded video into playback-optimized segments
- **Transcoder workers** (stateless; scalable)
  - generates multiple renditions (resolution/bitrate/codec)
- **CDN**
  - caches **popular chunks** and often the **manifest file**

---

## 6) Upload Flow (Large File-Safe)

### 6.1 Why direct-to-object-store upload
Passing video bytes through API Gateway / Video Service causes:
- request size limits (e.g., API gateway max body)
- expensive network + compute
- avoidable bottlenecks

### 6.2 Pattern: Multipart upload with presigned URLs
1) Client calls **POST `/videos`** with metadata + size.
2) Video Service:
   - writes `VideoMetadata(upload_status=PENDING)`
   - requests multipart upload session from object store
   - returns **presigned part URLs** to client.
3) Client:
   - splits file into **multipart upload parts** (large parts; e.g., 5–10MB+)
   - uploads parts directly to object storage using provided URLs.
4) Object storage:
   - stitches parts into a single object (source video).
   - emits an **upload-complete notification** event.

### 6.3 Upload completion signaling (avoid trusting client)
Object storage emits a completion event to a worker/Lambda:
- update `VideoMetadata` with:
  - `upload_status=UPLOADED`
  - `source_object_url=...`

---

## 7) Processing Pipeline: Chunking + Transcoding (Async)

### 7.1 Why re-chunk after multipart upload
Multipart upload chunks are optimized for **upload efficiency**:
- large parts, arbitrary byte offsets
- not aligned to media boundaries/keyframes

Playback segments must be optimized for **streaming UX**:
- small durations (e.g., **2–10 seconds**)
- usually aligned to **keyframes** for clean switching/decoding

### 7.2 Chunking
Trigger: object store upload-complete event  
Process:
- Chunker reads `source_object_url`
- Outputs:
  - **playback segments** (2–10s)
  - stored back to object storage

### 7.3 Transcoding (multiple renditions)
To support low bandwidth and device diversity, create renditions:
- 240p, 720p, 1080p, 4K (example)
- varying bitrate / codec / resolution / frame rate

Process:
- Chunker → send segments to transcoder workers
- Transcoders produce per-rendition segment sets
- Store all rendition segments back to object storage

Update metadata to a structured mapping:
```json
{
  "renditions": {
    "240p": ["s3://.../seg1.ts", "s3://.../seg2.ts", "..."],
    "720p": ["s3://.../seg1.ts", "s3://.../seg2.ts", "..."],
    "1080p": ["s3://.../seg1.ts", "..."]
  }
}
```

### 7.4 Manifest generation
Create a **manifest file** describing renditions + ordered segment URIs:
- format often XML/JSON depending on protocol
- stored in object storage and **cached in CDN**

Update metadata:
- `manifest_url=...`
- `upload_status=READY` (or `PROCESSING` → `READY`)

---

## 8) Playback Flow (Chunked + Adaptive Streaming)

### 8.1 Baseline playback sequence
1) Client calls **GET `/videos/{id}`** → receives `manifest_url` + metadata.
2) Client fetches **manifest** (prefer CDN).
3) Client streams by requesting segments in order.

### 8.2 Adaptive Bitrate (ABR)
Client continuously evaluates network conditions:
- starts at best viable rendition
- **switches** subsequent segments to lower/higher resolution as conditions change  
Example:
- Start 4K on home Wi‑Fi
- Move to 3G → switch next segments to 720p / 240p, etc.

### 8.3 CDN role
Latency bottleneck often becomes distance to object storage region.
CDN addresses this by caching:
- **popular segments** near users
- **manifest files** (small and high leverage)

Benefits:
- lower RTT
- faster startup
- reduced object store egress load

---

## 9) Streaming Protocol Abstractions (HLS / DASH)

Most systems rely on standard protocols that already define:
- segmenting strategy
- manifest format
- ABR switching rules
- delivery over standard HTTP(S) + CDN friendliness

Common examples:
- **HLS** (HTTP Live Streaming)
- **MPEG-DASH** (Dynamic Adaptive Streaming over HTTP)

In interview settings, naming them is optional; understanding the primitives is the key:
- segment/chunk
- manifest
- multiple renditions
- ABR on client

---

## 10) Data Storage & Scaling Notes

### 10.1 Video metadata DB
Metadata per video is small (often KB-level).
At the scale in the walkthrough (1M/day), storage is manageable; database choice is flexible:
- Postgres can work for long periods for metadata-only scope
- DynamoDB also fits; shard/partition by `video_id`

Common access patterns:
- get by `video_id`
- list videos by `user_id` (use a secondary index)

### 10.2 Stateless services scale horizontally
- API Gateway + Video Service: add instances; load-balance
- Chunker + Transcoders: add workers; autoscale based on CPU/memory/queue depth

### 10.3 Object storage “infinite scale”
Object storage is used because it handles:
- huge blobs cheaply
- large throughput
- high durability

---

## 11) Consistency & Availability

**Availability > Consistency**:
- Video becomes visible/streamable only after processing finishes.
- Users can still use the system (watch existing videos) during processing.

Implementation implications:
- `upload_status` gates watchability:
  - `PENDING/UPLOADED/PROCESSING`: show “processing” UI
  - `READY`: allow playback
  - `FAILED`: retry/notify

---

## 12) Failure Handling (Recommended Additions)

The walkthrough hints at deeper staff-level discussion:
- What if **chunker/transcoder fails**?

Suggested primitives:
- **Job orchestration**: DAG/workflow engine or queue-based pipeline
- **Retry policy**:
  - retry transient failures with backoff
  - cap retries; send to DLQ
- **Idempotency**:
  - rerunning chunk/transcode should not corrupt state
  - write output to deterministic paths (e.g., `/videos/{id}/renditions/{r}/seg_{n}`)
- **Progress tracking**:
  - percent complete, last successful segment, etc.
- **Partial availability** (optional):
  - allow playback once first N segments are READY (more complex)

---

## 13) Security / Abuse Controls (Baseline)

- Upload authorization: only authenticated users can initiate upload sessions.
- Presigned URL constraints:
  - short expiration
  - bound to object key prefix, content length bounds, checksum if possible
- Rate limiting:
  - per user/IP upload sessions
  - API Gateway throttles
- Malware/abuse scanning (extension):
  - scanning stage before publishing/READY

---

## 14) Observability (Operational Essentials)

- Metrics:
  - upload success rate, multipart failures
  - processing latency (upload→READY)
  - transcoder queue depth, worker CPU/memory
  - CDN hit rate, startup latency
- Logs/traces:
  - correlate by `video_id` / `upload_session_id`
- Alerts:
  - processing backlog grows
  - READY latency SLO violations
  - CDN hit rate drop (unexpected origin load)

---

## 15) Extension Checklist (Beyond Scope but Common Next Steps)

If later requests expand the system, these plug in naturally:
- Search & indexing (title/description, captions)
- Recommendations / feed
- Comments, likes, subscriptions
- Monetization, ads, analytics
- DRM / signed playback URLs
- Multi-region replication & geo-routing
- Live streaming (different pipeline)

---

## 16) Quick Reference: End-to-End Sequence

### Upload
- `POST /videos` → metadata + size → returns presigned URLs
- client multipart upload → object store
- object store notification → update metadata → start processing

### Processing
- chunker → playback segments (2–10s)
- transcoders → multiple renditions
- generate manifest → store manifest + segments → CDN caches popular

### Playback
- `GET /videos/{id}` → returns manifest URL
- client fetch manifest → fetch segments from CDN/object store
- ABR switching based on network conditions

---

## 17) Design “Why” Summary (Most Reusable Ideas)

- **Direct-to-object-store upload** avoids gateway limits and bottlenecks.
- **Two chunking layers** are normal:
  - upload parts ≠ playback segments
- **Async processing** enables availability-first behavior.
- **Transcoding + ABR** is essential for low bandwidth & device diversity.
- **Manifest + CDN** is the core of scalable streaming delivery.
