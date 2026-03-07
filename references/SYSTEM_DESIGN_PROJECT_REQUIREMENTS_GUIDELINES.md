# Take-Home Exam Guidelines — LLM Reference (Augmented)

This file is intended to be used as a **reference/source for LLMs** while designing and implementing the take-home system.
It includes (a) the original PDF guideline content **verbatim where possible**, with the **Submission** section removed as requested, and
(b) **explicit augmentations** describing additional requirements, assumptions, and scenarios.

---

## 1) Original guideline content (Submission removed)

### Software X Discover Engineer

#### Overview
This exercise evaluates architectural thinking, code quality, and communication skills.  
You have **1 week** to complete the task.

#### Goal
Design and implement a minimal private video streaming service. The main focus is on **strong architecture design** and a **maintainable codebase**.  
A functional UI is sufficient.

#### Core requirements
1. Users can upload video files  
   - The upload size is limited to **1GB**  
   - Support common video formats  
   - Videos can be uploaded anonymously  
   - The system generates a **shareable link** per video  
2. Users can stream videos by accessing the shareable link via the browser.  
3. **Time-to-stream** (the time from starting a video upload until it can be streamed to users) is more important than high video quality.  
4. Provide a short document explaining the core architecture decisions and overall design.

#### Bonus
1. Playback performance should feel consistent, regardless of file size.  
2. The system should be able to scale horizontally.  
3. The architecture needs to be cost efficient (cheap to run).

#### Guidelines
1. The preferred stack is **Rust & Svelte**. You’re welcome to introduce additional tools you find appropriate.  
2. We welcome the use of AI tools, but please don’t collaborate with other people.  
3. It’s understandable if you don’t manage to implement everything in code, but completing the architecture and system design is a hard requirement.  
4. We value thoughtful architectural decisions, and clean code.  
5. You do not need to implement authentication, or write IaC.

---

## 2) Augmentations (must be considered in architecture + documentation)

### 2.1 Stack constraints and allowed deviations
- **Primary/required stack**:
  - Backend: **Rust**
  - Frontend: **Svelte**
- Additional languages/tools (e.g., Python) should be introduced **only if they provide a significant improvement** (e.g., best-in-class media tooling or a clearly simpler/safer processing pipeline).
  - If introduced, it must be explicitly justified in architecture docs: *why it is necessary, where it runs, how it is deployed/operated, and why Rust alone was insufficient.*

### 2.2 Core emphasis: Time-to-stream
Time-to-stream is a **first-class** requirement.

**Interpretation / target behavior**:
- A video should become streamable **as soon as the first low-quality rendition is available**, rather than waiting for all renditions.
- Working assumption: the system should allow streaming once **~360p** (or equivalent low-bitrate baseline) has finished processing.

**Implications to document (not solutions, but required considerations)**:
- The pipeline should support a **“minimum viable streamable state”** and a **“fully processed state.”**
- Metadata/state must encode:
  - upload started / upload completed
  - processing in progress
  - **baseline rendition ready (streamable)**
  - additional renditions ready (quality improves over time)
  - failed states / retries

### 2.3 Multi-user upload concurrency (no head-of-line blocking)
The system must handle **multiple users uploading concurrently**.

Scenario that must be supported:
- If User A uploads a very large file (near the limit), **User B and others must still be able to upload** without severe degradation or blocking.

Implications to consider in design:
- Avoid single shared bottlenecks on the upload path (e.g., avoid routing all bytes through one app server instance).
- The upload path should be designed for **horizontal scalability** and isolation of slow uploads.

### 2.4 Horizontal scaling and race-condition scenarios
The system should remain correct under **horizontal scaling** (multiple backend instances).

Scenarios to explicitly handle (in requirements/docs):
- **Read-your-writes vs global visibility**:
  - A video might appear available to the uploader (because they hit one server / cache path) but **not yet available** to other users (hitting another server / region / cache).
- **Partial availability across servers**:
  - Some servers may have cached metadata or manifest pointers while others do not.
- **State propagation**:
  - When processing transitions from “uploading → baseline ready → fully ready,” all servers must converge to the same state eventually.

This implies architecture documentation must address:
- How state is stored (shared, durable, not per-instance)
- How servers observe transitions (polling vs eventing)
- How to prevent inconsistent “READY” responses across instances

> Note: This section is for *requirements and scenarios*; solutions will be designed later.

### 2.5 Scalability target
The system should be designed with an explicit scalability goal:
- Must be able to handle **millions of users**.

Even if the MVP implementation is smaller, the design doc must show:
- which components scale horizontally
- which components are bottlenecks
- how storage and delivery scale (uploads, streaming, processing)

### 2.6 Storage and cloud environment
- Storage: **Amazon S3** is the canonical object store for uploaded videos and derived artifacts.
- Infrastructure: **AWS in general** is assumed.

You do **not** need to write IaC, but the design should still:
- name AWS services used (at least at a component level)
- explain why each service is chosen
- outline minimal deployment topology (regions, networking boundaries, etc.)

---

## 3) Cost-efficiency considerations for AWS + S3 (requirements-level guidance)

This section is **guidance to inform design decisions**. It should influence architecture and tradeoffs.

### 3.1 S3 cost drivers (what matters)
- **Storage class** (Standard vs IA vs Glacier)
- **Requests** (PUT/GET/LIST costs)
- **Data transfer out** (especially to the internet)
- **Lifecycle duration** (how long originals and renditions are kept)
- **Replication** (cross-region replication increases cost)

### 3.2 Practical cost-saving levers to consider
- **CDN in front of S3** for streaming:
  - improves latency and can reduce origin egress by caching hot segments/manifests.
- **Lifecycle policies**:
  - keep *original uploads* only as long as needed (or keep but transition to cheaper storage if not frequently accessed).
  - transition older videos/renditions to cheaper classes if the product allows.
- **Minimize redundant artifacts**:
  - be intentional about how many renditions you generate (each rendition multiplies storage and processing).
  - prefer a small set of renditions aligned with “time-to-stream first” goals.
- **Avoid excessive LIST operations**:
  - store explicit pointers/keys in metadata DB rather than listing S3 prefixes at runtime.
- **Right-size segment/chunk strategy**:
  - too-small segments increase request volume (GETs), too-large segments increase buffering/startup.
  - pick a segment length that balances UX and request costs.
- **Use multipart upload**:
  - reduces retry cost for large uploads and is standard practice for large files.
- **Delete/expire abandoned uploads**:
  - clean up incomplete multipart uploads.
- **Compress and choose efficient codecs** (balanced against processing time):
  - codecs affect storage/egress and time-to-stream (tradeoff).

### 3.3 Cost-efficiency is a “bonus requirement” but should be visible in the design
Even if the MVP is minimal, documentation should explicitly mention:
- expected major cost centers
- decisions taken to reduce them (or why not)

---

## 4) Deliverables (what the design must contain)
The guidelines require “architecture and system design” even if code is incomplete.
This implies the design artifact(s) must include at minimum:

- **High-level architecture diagram** (components + data flow)
- **Key APIs** (upload initiation, upload completion signal, playback)
- **Core data model/state machine** (video lifecycle with baseline-ready)
- **Streaming approach** (chunking/manifest/renditions conceptually)
- **Background processing approach** (workers/queues/events; failure handling)
- **Horizontal scaling story** (stateless services, shared metadata, CDN)
- **Time-to-stream strategy** (baseline rendition gating)
- **Cost notes** (how the design avoids obvious waste on AWS/S3)

---

## 5) Explicit exclusions (from original guidelines)
- **Authentication is not required to implement**.
- **Infrastructure-as-code is not required to implement**.

(You may still describe how auth/IaC would be added later, but it is not required in the MVP.)

