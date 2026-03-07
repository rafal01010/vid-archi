# Reference: Netflix/YouTube-Style System Design (LLM-Optimized)

This document is a **machine-usable architecture reference** distilled from a systems design walkthrough.  
It emphasizes **design primitives, data models, flows, and scaling tradeoffs** (not narrative).

---

## 1) Problem Definition

Build a video platform (YouTube/Netflix-like) that supports:
- **Posting/uploading videos**
- **Watching/streaming videos** (buffering, varying bandwidth)
- **Commenting** on videos
- **Searching** videos by title/description

(“View counts” are treated as a standard event-aggregation problem similar to click-counting systems.)

---

## 2) Requirements

### 2.1 Functional
- Upload/post video
- Stream/watch video with buffering
- Comment on video
- Search by name/title/description

### 2.2 Non-functional (implied / emphasized)
- Optimize for **reads** (watching dominates)
- **Scalability** to “billions of users”
- **Streaming-friendly delivery**
  - low barrier to start playback (don’t download whole file)
  - adapt to bandwidth fluctuations
- Eventual consistency is acceptable for many secondary features (e.g., metadata propagation).

---

## 3) Capacity & Back-of-Envelope Estimates (from walkthrough numbers)

Example scale assumptions used:
- **1B users**
- Average video has ~**1,000 views**
- Average video size ~**100 MB**
- Upload volume ~**1,000,000 videos/day**

Storage growth estimate:
- 100 MB/video × 1,000,000 videos/day × ~400 days/year ≈ **40 PB/year**

Implication:
- Raw media storage is huge → **object/blob storage** is mandatory.
- You must design for **high read throughput** and efficient CDN delivery.

---

## 4) Core Streaming Concepts (Primitives)

### 4.1 Why chunking exists
Instead of loading a full video file:
- Load **chunk 1** → start playback immediately
- Meanwhile fetch **chunk 2, 3, 4…** in the background

This reduces:
- startup latency
- wasted transfer for abandoned plays
- sensitivity to bandwidth dips

### 4.2 Multiple renditions (resolution + encoding)
Support:
- multiple **resolutions** (e.g., 240p/720p/1080p/4k)
- multiple **encodings/codecs** (device compatibility, compression efficiency)

Playback can adapt:
- high bandwidth → higher rendition
- low bandwidth → lower rendition

(Practical implementations typically use HLS/DASH, but knowing the primitives is enough.)

---

## 5) High-Level Architecture (Services + Storage)

### 5.1 Control plane vs data plane
- **Control plane**: metadata APIs, permissions, listing, search coordination
- **Data plane**: large media upload/download, chunk delivery

### 5.2 Suggested components
- **API Gateway**
- **Upload Service** (generates upload sessions; writes metadata; triggers pipeline)
- **Comment Service**
- **Search/Indexing pipeline** (inverted index; elastic-like)
- **Processing pipeline** for chunking/transcoding
- **Object storage** (e.g., S3) for raw and processed media
- **CDN** for hot content delivery
- **Message broker** (Kafka suggested) + **stream processor** (Flink suggested) for background workflows / CDC

---

## 6) Data Model (Tables / Collections)

> The walkthrough uses “tables” conceptually; actual DB choice can vary per table.

### 6.1 Users table (baseline)
- `user_id` (partition/shard key)
- `email`, `password_hash`, etc.

Sharding:
- Partition by `user_id`
- Single-leader replication is acceptable (not a primary bottleneck)

---

### 6.2 Subscriptions (followers)
Goal queries:
- “Who am I subscribed to?” (viewer → channels)
- “Who is subscribed to me?” (channel → subscribers) (can be derived)

**Table A: subscriptions**
- `subscriber_user_id` (viewer)
- `subscribed_to_user_id` (channel)

Sharding/index:
- Partition + index by `subscriber_user_id` to serve “who am I subscribed to?” efficiently.

**Derived view via CDC (optional but recommended): channel_subscribers**
- Derived by **Change Data Capture** from subscriptions into Kafka → processed by Flink.
- Partition Kafka topic by `subscribed_to_user_id`
- Maintain a cached/materialized structure listing subscribers per channel.

Why derive?
- Avoid dual-write in the request path.
- Move fanout-related data prep to async pipeline.

---

### 6.3 Video metadata table
Minimal set:
- `video_id` (often unique within channel; can be `(channel_id, video_id)` compound identity)
- `channel_id` (or uploader id)
- `title`, `description`
- `created_at`
- `status`: `UPLOADING | PROCESSING | READY | FAILED`
- references to chunk sets / manifest / thumbnails

> “User videos table” is mentioned as a listing structure (see below).

---

### 6.4 User videos table (channel → list of videos)
Purpose:
- List videos for a channel/user quickly (channel page)
- Gate availability: only show/watch when processing is sufficiently complete

Typical fields:
- `channel_id` (partition key)
- `video_id` (sort key; time-based ordering)
- `created_at`
- `status` / `is_ready`
- (optional) pointers to preview/thumbnail

---

### 6.5 Video comments table
Fields:
- `(channel_id, video_id)` as partition key (or `video_id` alone if globally unique)
- `timestamp` as clustering/sort key
- `commenter_user_id`
- `content`

Reasoning:
- Keep all comments for a video on the same partition for fast reads.

DB choice guidance from walkthrough:
- MySQL is fine for many tables, **but** comment write load for viral videos may be high.
- **Cassandra** is suggested for comment storage due to:
  - LSM-tree write friendliness (writes go to memory first)
  - leaderless replication (high write throughput)
  - natural modeling: partition by `(channel_id, video_id)` and cluster by `timestamp`

Cassandra caveat:
- Editing comments / child-threaded comments increases complexity (treated as stretch goal).

---

### 6.6 Video chunks table (control pointers)
Tracks where to fetch the actual chunk bytes for playback.

Conceptually includes:
- `(channel_id, video_id)` (partition)
- `encoding`
- `resolution`
- `chunk_index` or `time_range`
- `chunk_storage_url` (e.g., `s3://...` or CDN URL)
- (optional) `checksum`, `size_bytes`

This table is key to “watch” reads:
- Watch service reads metadata + chunk pointers quickly
- Client pulls chunk bytes via CDN/object storage

---

## 7) Upload + Processing Pipeline

### 7.1 Why background processing is required
Chunking + transcoding are heavy and must be asynchronous:
- CPU intensive
- multi-rendition generation
- cannot be done inline in an upload request

Hence: **message broker + background workers**.

### 7.2 What to put on the broker (important)
Do **NOT** put chunk bytes on Kafka.
- Chunk payloads can be large.
- Kafka is for **events/metadata**, not blob transfer.

Put on Kafka:
- `video_id`, `channel_id`, uploader id
- object storage pointers (keys/URLs)
- processing state transitions
- chunk completion notifications

### 7.3 Storage for bytes
Options mentioned:
- **S3** (object storage) as the canonical store for chunk bytes
- (Alternative discussed) **HDFS** clusters (more ops heavy; may colocate compute with storage)
Practical takeaway:
- Store bytes in S3 (or similar); keep pipelines stateless; use pointers in events.

---

## 8) CDN Strategy (Hot Content)

CDN is used for **fast chunk delivery**:
- When a channel/user is “popular” (many subscribers / high demand), push/cache content in CDN.

Important ordering warning from walkthrough:
- Be careful about **sequential upload/publish ordering**:
  - Don’t push to CDN too early if the video is not yet watchable (wastes CDN capacity, bad UX).
  - Gate CDN push and “READY” states appropriately.

---

## 9) Search Index Design (Distributed Inverted Index)

### 9.1 Index structure
Goal:
- Search by video title/description.

Core technique:
- Build an **inverted index**:
  - term → list of `(channel_id, video_id)` that contain the term

Tokenization:
- Split titles/descriptions into tokens/terms (exact algorithm depends on search engine).

### 9.2 Partitioning problem (scale)
Desired ideal:
- For a term, store all IDs on one node (fast, no fan-in aggregation)

Reality at scale:
- Popular terms can explode in size.
- Example estimate from walkthrough:
  - 1M videos/day × 10 years ≈ **4B videos**
  - `(user_id + video_id)` as 8 bytes total
  - 8 bytes × 4B ≈ **32 GB of IDs** for a single ubiquitous term (in theory feasible)
But:
- You often need more than IDs (title, description, thumbnails, ranking signals)
- That implies **denormalization** inside the index, making popular terms far larger.

### 9.3 Practical partitioning strategies
1) **Round-robin documents into partitions**
   - Each partition has a local index.
   - Query requires aggregation across partitions (expensive if many partitions).

2) **Term-based partitioning**
   - Keep each term’s postings primarily on one node.
   - For very popular terms (e.g., “Fortnite”), add **inner-sharding**:
     - `term$1`, `term$2`, `term$3`, … across multiple nodes
     - Query aggregates only those sub-shards.

### 9.4 Denormalization tradeoff
To avoid expensive distributed fetches from video metadata after search:
- Store (denormalize) fields like `title` and `description` in the search index.

Cost:
- Editing titles/descriptions becomes expensive because you must update index entries.

Conclusion:
- Accept slower edit propagation because reads dominate; eventual consistency is fine.

### 9.5 Handling metadata edits (CDC-driven index updates)
Proposed approach:
- Use CDC on video metadata/user-videos changes into Kafka.
- Use Flink to:
  - maintain old title/description
  - compute required removals/additions in the search index
  - issue update operations to Elasticsearch-like cluster

This can touch multiple partitions and be slow, but is acceptable.

---

## 10) Event-Driven Orchestration (Kafka + Flink Pattern)

### 10.1 Processing events
Key idea:
- Workers emit **chunk completed** events as processing runs.

Events contain:
- `channel_id`, `video_id`
- `uploader_user_id`
- `chunk_index` / rendition info
- completion markers

Sharding strategy:
- Partition chunk events by `uploader_user_id` to concentrate per-uploader state in one Flink task.

### 10.2 What Flink maintains
For a given uploader:
- number of subscribers (from derived cache/table)
- number of expected chunks
- which chunks are completed

When all required chunks are done:
- perform external writes (must be idempotent):
  1) write/update **video chunks table**
  2) update **user videos table** to mark watchable
  3) optionally push/cache in **CDN** if uploader is popular
  4) emit metadata/index update events for search (Kafka → Flink → search cluster)

### 10.3 Ordering matters
Suggested ordering (from walkthrough logic):
1) Ensure chunk pointers exist (video chunks table)
2) Mark video as visible/watchable (user videos table / metadata)
3) CDN push for hot channels
4) Search index update pipeline

---

## 11) Database Choices (As Suggested)

- **MySQL**:
  - Users
  - Subscriptions (write volume is low; correctness emphasized)
  - Video metadata / user videos (often manageable)
  - Single-leader replication OK for many tables

- **Cassandra**:
  - Video comments (high write throughput, viral spikes)
  - Partition by `(channel_id, video_id)`, cluster by `timestamp`

- **Search engine**:
  - Elasticsearch-like distributed inverted index

- **Blob storage**:
  - S3 for video bytes and chunk bytes

---

## 12) Reliability & Correctness Primitives (Implied Essentials)

Even if not deeply expanded in the walkthrough, the architecture relies on:

- **Idempotent consumers**:
  - Kafka/Flink replays are normal; writes must be safe to repeat.
- **Exactly-once is optional**; at-least-once + idempotency is common.
- **CDC** avoids dual-write complexity in request path.
- **Eventual consistency** acceptable for:
  - subscription-derived views
  - search index updates
  - title/description edits propagation

---

## 13) Minimal APIs (Interface-Level)

### 13.1 Upload
- `POST /videos` → create video record + begin upload session (return upload instructions)
- `POST /videos/{id}/complete` (optional if object-store events are used)
- Upload bytes should go **direct to object storage** (presigned multipart)

### 13.2 Watch
- `GET /videos/{id}` → return metadata + pointers to chunk sets (or a manifest URL)
- Client streams by fetching chunk bytes via CDN/object storage.

### 13.3 Comments
- `POST /videos/{id}/comments`
- `GET /videos/{id}/comments?cursor=...`

### 13.4 Search
- `GET /search?q=...` → query distributed inverted index and return ranked results.

---

## 14) “Most Reusable” Design Decisions (Why)

- **Chunked streaming** enables fast startup + buffering.
- **Multiple renditions** (resolution/encoding) enable device support + bandwidth adaptation.
- **Kafka + Flink** provide:
  - async processing
  - CDC-based denormalized views
  - scalable orchestration and fanout
- **Do not put blobs on Kafka**; store bytes in S3 and pass pointers in events.
- **Cassandra for comments** when write spikes dominate.
- **Search index denormalization** makes reads fast; accept slow edits.

---

## 15) Extensions Commonly Added Later
- View count aggregation (streaming counters)
- Personalized feed (fanout-on-write/read tradeoffs)
- Recommendations, ranking, analytics
- Thumbnails, captions, DRM, regional replication, AB testing
