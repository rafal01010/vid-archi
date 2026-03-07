# Rust Clean Code Guidelines for LAG‑Safe Code Generation

This file is a coding guide for generating **clean Rust code** for a production-minded system design project. The goal is not shortest code, clever tricks, or maximum abstraction. The goal is code that is:

- easy to read
- easy to reason about
- easy to change
- hard to misuse
- aligned with architecture and domain boundaries
- safe under failure and edge cases

## Core Principle

**Clean code is not fewer lines.**

Prefer code that makes the system obvious to a reviewer. For this project, clean Rust means the code structure should help someone quickly understand:

- what the domain objects are
- what states they can be in
- how requests flow through the system
- where business logic lives
- where infrastructure concerns live
- how errors and edge cases are handled

Readable, explicit, boring code is usually better than clever code.

---

# 1. High-Level Standards

## 1.1 Prioritize clarity over terseness

Prefer:

- descriptive names
- small focused functions
- explicit state modeling
- straightforward control flow
- obvious error handling

Avoid:

- dense one-liners when they harm readability
- unnecessarily clever iterator chains
- macros that hide normal business logic
- abstractions that make simple code harder to follow

## 1.2 Optimize for maintainability, not just compilation

Do not generate code that only “works.” Generate code that another engineer can confidently modify.

Every function, struct, enum, module, and trait should have a reason to exist.

## 1.3 Match the code structure to the domain

The codebase should reflect business concepts, not just framework mechanics.

Prefer modules named after domain capabilities such as:

- `video`
- `upload`
- `playback`
- `processing`
- `storage`
- `http`

Avoid generic junk-drawer modules like:

- `utils`
- `helpers`
- `common`
- `misc`

Only introduce such modules when the contents are truly cohesive.

---

# 2. Naming Rules

## 2.1 Names must reveal intent

Choose names that explain what something is or does without requiring the reader to inspect implementation details.

Good examples:

- `VideoStatus`
- `UploadSessionId`
- `CreateUploadSession`
- `GetPlaybackInfo`
- `mark_baseline_streamable`
- `persist_video_metadata`

Bad examples:

- `Data`
- `Item`
- `Thing`
- `handle_video`
- `process_data`
- `do_work`

## 2.2 Use domain vocabulary consistently

When the domain has a concept, use one canonical term everywhere.

For example, if the system uses `baseline rendition ready` or `streamable`, do not mix this with vague alternatives like:

- `done`
- `prepared`
- `active`
- `available-ish`

Pick one term and use it in:

- types
- field names
- API DTOs
- logs
- comments
- function names

## 2.3 Avoid vague booleans in names

Do not model rich state with multiple booleans like:

- `is_uploaded`
- `is_processed`
- `is_ready`
- `has_failed`

This creates ambiguous and invalid combinations.

Prefer enums that represent the actual lifecycle.

---

# 3. Model the Domain Explicitly

## 3.1 Prefer enums over boolean soup

Whenever the system has a real lifecycle, use an enum.

Bad:

```rust
struct Video {
    uploaded: bool,
    processing: bool,
    ready: bool,
    failed: bool,
}
```

Good:

```rust
enum VideoStatus {
    PendingUpload,
    Uploaded,
    Processing,
    StreamableBaselineReady,
    Ready,
    Failed { reason: String },
}
```

This is cleaner because it:

- matches the domain
- makes invalid combinations harder
- simplifies downstream logic
- improves API clarity

## 3.2 Use types to prevent misuse

When possible, use domain types instead of raw primitives everywhere.

Prefer:

- `VideoId`
- `UploadSessionId`
- `ManifestUrl`
- `ShareToken`

Avoid using raw `String` and `Uuid` values throughout the codebase without semantic wrappers when intent matters.

Use newtypes when they improve clarity and reduce accidental misuse.

## 3.3 Make illegal states harder to represent

Design structs and enums so that impossible states are not easy to construct.

Examples:

- do not allow a “ready” video without a manifest URL
- do not allow playback metadata to exist without a valid video identity
- do not permit “failed and ready” to coexist in the same state representation

---

# 4. Function Design

## 4.1 Functions should do one clear job

A function should have one main responsibility.

Bad signs:

- parses request input
- validates business rules
- writes to database
- calls object storage
- triggers background processing
- transforms response DTO
- logs extensively
- maps HTTP errors

all inside one function.

Instead, separate responsibilities.

Example layering:

- handler: HTTP concerns only
- service/use case: orchestration and business flow
- repository/client: DB or external service interaction
- domain methods: invariant enforcement and state transitions

## 4.2 Keep handlers thin

HTTP handlers should mainly:

- extract inputs
- call one application service or use case
- map results to HTTP responses

Handlers should not contain core business logic.

## 4.3 Prefer straightforward control flow

Avoid deeply nested conditionals when early returns make logic easier to follow.

Prefer code where the happy path and failure paths are both readable.

## 4.4 Keep parameters cohesive

If a function takes many related arguments, group them into a meaningful struct.

Bad:

```rust
async fn create_video(
    title: String,
    owner_id: String,
    mime_type: String,
    size_bytes: u64,
    checksum: String,
    visibility: String,
) -> Result<Video, Error>
```

Better:

```rust
struct CreateVideoCommand {
    title: String,
    owner_id: OwnerId,
    mime_type: String,
    size_bytes: u64,
    checksum: String,
    visibility: Visibility,
}
```

Only do this when the arguments form a real conceptual unit.

---

# 5. Error Handling

## 5.1 Be explicit and meaningful

Errors should preserve business meaning.

Different failures should remain distinguishable when the distinction matters:

- invalid input
- unsupported file type
- video not found
- upload session expired
- manifest not ready yet
- storage service unavailable
- processing pipeline failed

Do not collapse everything into a generic internal error too early.

## 5.2 Avoid `unwrap()` and `expect()` in normal application flow

Use `unwrap()` only when failure is truly impossible or in tests/prototyping where justified.

Production code should handle fallible paths explicitly.

## 5.3 Add context when propagating errors

If using `anyhow`, `thiserror`, or custom error types, include useful context.

Bad:

```rust
let row = repo.get(id).await?;
```

Better:

```rust
let row = repo
    .get(id)
    .await
    .context("failed to load video record from repository")?;
```

## 5.4 Map errors at the correct boundary

Keep infrastructure details out of public HTTP/API responses.

For example:

- storage timeout may map to 503
- missing resource may map to 404
- invalid request may map to 400
- processing not complete yet may map to 409 or a domain-specific response

The HTTP layer should translate domain/application errors clearly.

---

# 6. Module and Architecture Boundaries

## 6.1 Separate layers clearly

Use clear boundaries between:

- HTTP transport
- application/use-case orchestration
- domain logic
- infrastructure integrations
- persistence

A reviewer should be able to see where to go for each responsibility.

## 6.2 Do not leak framework details into domain logic

Avoid contaminating core business logic with:

- framework request/response types
- database row structs used as domain models
- storage SDK-specific types
- web-layer serialization annotations everywhere

Keep the core model as framework-agnostic as practical.

## 6.3 Group by feature when possible

For medium-sized projects, prefer feature-oriented structure over layer-only structure.

Example:

```text
src/
  video/
    mod.rs
    domain.rs
    service.rs
    repository.rs
    dto.rs
  upload/
    mod.rs
    service.rs
    storage.rs
  playback/
    mod.rs
    service.rs
    dto.rs
  http/
    mod.rs
    routes.rs
    error.rs
```

This is usually cleaner than giant global files for all handlers, models, and services.

---

# 7. Traits and Abstractions

## 7.1 Do not abstract before there is real pressure

Avoid introducing traits, generics, or indirection just to appear architectural.

Bad reasons for a trait:

- “maybe we will need another implementation later”
- “this looks more enterprise”
- “the LLM generated an interface automatically”

Good reasons for a trait:

- multiple real implementations exist now
- the abstraction enables testing without distorting the design
- the boundary is naturally stable and conceptually meaningful

## 7.2 Keep traits small and focused

Prefer narrow traits expressing capability.

Good:

- `VideoRepository`
- `ObjectStore`
- `ManifestLocator`

Avoid giant traits with unrelated methods.

## 7.3 Prefer explicit concrete types when abstraction adds no value

A simple concrete dependency is often clearer than a trait object or generic parameter.

---

# 8. Data Structures and DTOs

## 8.1 Separate domain models from transport DTOs when useful

HTTP request/response structs often serve a different purpose from domain entities.

Do not force one struct to do all jobs if it harms clarity.

Examples of separate concerns:

- HTTP request payload
- application command
- domain entity
- persistence record
- HTTP response DTO

## 8.2 Keep serialization concerns localized

Use `serde` derives appropriately, but do not let transport-specific field naming conventions distort the domain model unnecessarily.

## 8.3 Avoid `serde_json::Value` unless flexibility is genuinely required

Prefer typed structures over unstructured JSON blobs when the shape is known.

---

# 9. Async and Concurrency

## 9.1 Keep async flows understandable

Use async because the system needs it, not because every helper must become async.

Do not make synchronous pure logic async.

## 9.2 Be explicit about orchestration boundaries

Where background processing or async workflows exist, keep responsibilities separated:

- command accepted
- upload completed
- processing triggered
- baseline rendition ready
- full readiness achieved

Do not bury lifecycle transitions inside unrelated code.

## 9.3 Protect readability around concurrency

Concurrency code must remain clear about:

- what runs concurrently
- what failures mean
- what retries happen
- what state transitions occur

If concurrency makes the code hard to understand, simplify it.

---

# 10. Comments and Documentation

## 10.1 Comment the “why,” not the obvious “what”

Avoid comments that restate code.

Bad:

```rust
// increment retry count
retry_count += 1;
```

Useful:

```rust
// We expose playback as soon as the baseline rendition is ready,
// because time-to-stream is prioritized over higher-quality completion.
```

## 10.2 Keep comments accurate

Outdated comments are worse than no comments.

If a comment exists, it must remain aligned with the code.

## 10.3 Use doc comments for public behavior when helpful

Public modules, traits, and functions benefit from short docs when their purpose is not obvious.

---

# 11. Logging and Observability

## 11.1 Write logs for operations, not noise

Logs should help answer:

- what request happened
- which entity it affected
- what state transition occurred
- why it failed
- what external dependency was involved

## 11.2 Include useful identifiers

Prefer structured logs containing contextual keys such as:

- `video_id`
- `upload_session_id`
- `request_id`
- `user_id` if applicable
- `status`

## 11.3 Do not log sensitive or excessively noisy data

Do not dump entire payloads or giant objects unless truly necessary.

---

# 12. Testing Mindset

## 12.1 Write code that is testable because it is well-structured

Do not contort the design just to make tests possible. Well-factored code is naturally easier to test.

## 12.2 Test behavior, not implementation trivia

Useful test targets include:

- state transitions
- error cases
- boundary behavior
- DTO mapping where important
- service orchestration rules

## 12.3 Prefer focused tests with clear names

A good test name should explain the scenario and expected result.

Example:

- `returns_streamable_status_when_baseline_manifest_exists`

---

# 13. Formatting and Style Expectations

## 13.1 Follow idiomatic Rust formatting

Use standard formatting tools and conventions.

Expect generated code to be compatible with:

- `rustfmt`
- `clippy`

## 13.2 Prefer readable line breaks over compressed code

Do not compress complex expressions into one line just because it is possible.

## 13.3 Keep import organization tidy

Avoid unused imports and noisy wildcard imports.

---

# 14. Anti-Patterns to Avoid

Do not generate code with these traits unless explicitly requested:

## 14.1 God handlers or god services

Huge functions that parse inputs, perform business logic, call external systems, persist data, and build responses all together.

## 14.2 Boolean explosion

Multiple flags used to simulate lifecycle state.

## 14.3 Premature generic abstraction

Complex traits, generics, and layers without real need.

## 14.4 Junk drawer modules

`utils.rs`, `helpers.rs`, `common.rs` containing unrelated code.

## 14.5 Overuse of dynamic/untyped structures

Too much reliance on:

- `HashMap<String, String>`
- `serde_json::Value`
- raw strings for domain semantics

## 14.6 Hidden side effects

Functions that look simple but perform network calls, persistence, or state mutation unexpectedly.

## 14.7 Error swallowing

Losing cause and context behind generic messages.

## 14.8 Cleverness over clarity

Using advanced Rust features when straightforward code is easier to understand.

---

# 15. LLM-Specific Code Generation Instructions

When generating Rust code, follow these rules:

## 15.1 Start from structure, not from random implementation

Before generating code, infer or define:

- domain entities
- state enums
- service boundaries
- repository boundaries
- DTOs
- error types

## 15.2 Prefer feature-oriented modules

Organize code around business capabilities instead of generic technical buckets.

## 15.3 Keep handlers thin and services focused

Never put all logic inside the route handler.

## 15.4 Use enums for lifecycle states

Whenever the domain has meaningful stages, model them explicitly.

## 15.5 Use meaningful names consistently

Do not invent multiple synonyms for the same concept.

## 15.6 Generate explicit error paths

Do not only code the happy path.

## 15.7 Avoid overengineering

Prefer a clean concrete solution over a “flexible” but harder-to-understand design.

## 15.8 Generate comments only when they add design context

Do not add filler comments.

## 15.9 Make code reviewer-friendly

Assume a human evaluator will read the repository and judge:

- readability
- maintainability
- architecture alignment
- soundness of decisions

---

# 16. Preferred Output Characteristics

Generated Rust code should generally have these characteristics:

- explicit domain types
- clear state modeling
- thin HTTP handlers
- focused services
- small cohesive modules
- meaningful error enums or wrapped errors with context
- structured logging
- straightforward async boundaries
- minimal but useful comments
- no unnecessary cleverness

---

# 17. Final Rule

When in doubt, generate the version that a teammate could understand and safely modify after reading it once.

That is clean Rust.
