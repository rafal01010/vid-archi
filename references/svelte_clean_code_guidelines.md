# Svelte Clean Code Guidelines for LLM‑Safe Code Generation

This file is a coding guide for generating **clean Svelte code** for a production-minded system design project. The goal is not the fewest lines, the most reactive magic, or the most compact component. The goal is UI code that is:

- easy to read
- easy to reason about
- easy to change
- aligned with backend and domain states
- predictable under async conditions
- maintainable as the product grows

## Core Principle

**Clean code is not shorter code.**

A component with fewer lines can still be harder to understand than a slightly longer component with clearer structure.

For this project, clean Svelte means the frontend should clearly express:

- what the page or component is responsible for
- what state exists
- which state comes from the backend
- which actions trigger network requests
- how async lifecycle states appear in the UI
- where reusable UI logic belongs

Readable, predictable, and explicit code is preferred over cleverness.

---

# 1. High-Level Standards

## 1.1 Optimize for understandability

Generate code that another engineer can scan and understand quickly.

Prefer:

- descriptive component and variable names
- simple data flow
- minimal hidden behavior
- explicit async handling
- focused components

Avoid:

- giant components doing everything
- deeply chained reactive statements
- unnecessary state duplication
- inline business logic mixed into markup-heavy files
- “smart” patterns that obscure how the UI works

## 1.2 Let the UI reflect the domain

The frontend should not invent confusing local concepts that differ from backend concepts.

If the backend exposes meaningful states such as:

- `PendingUpload`
- `Uploading`
- `Processing`
- `StreamableBaselineReady`
- `Ready`
- `Failed`

then the frontend should use and present those states clearly rather than flattening everything into vague booleans like:

- `isLoading`
- `isReady`
- `isDone`
- `showPlayer`

## 1.3 Prefer maintainable simplicity over framework cleverness

Svelte makes concise reactive code easy, but brevity is not the goal. Use Svelte features in a way that improves readability.

---

# 2. Component Design

## 2.1 Each component should have a clear purpose

A component should do one UI job well.

Good examples:

- `UploadForm.svelte`
- `UploadProgress.svelte`
- `ProcessingStatus.svelte`
- `VideoPlayer.svelte`
- `ShareLinkBox.svelte`

Bad pattern:

One giant page component that:

- handles file selection
- performs network requests
- tracks upload progress
- polls processing state
- renders playback
- builds share URLs
- formats error messages
- contains complex conditional markup

all in one place.

## 2.2 Do not split components too early

Do not create tiny components for every small element just for the sake of reuse. Split when it improves clarity, reuse, or separation of concerns.

## 2.3 Keep presentational concerns and data-fetching concerns reasonably separated

A component may still perform its own fetch when appropriate, especially in small apps, but avoid mixing heavy network orchestration with large markup sections when the component becomes hard to read.

---

# 3. Naming Rules

## 3.1 Names must reveal intent

Use names that explain what the state or function represents.

Good examples:

- `videoStatus`
- `uploadProgress`
- `playbackInfo`
- `createUploadSession`
- `refreshProcessingState`
- `copyShareLink`

Bad examples:

- `data`
- `item`
- `temp`
- `stuff`
- `handleThing`
- `fetchData` when several kinds of data exist

## 3.2 Use one canonical name per concept

If the backend uses `streamable` or `baselineReady`, use the same term throughout the frontend.

Do not mix inconsistent synonyms such as:

- `ready`
- `live`
- `playable`
- `done`
- `processed`

unless they truly mean different things.

## 3.3 Event handlers should describe the action

Good:

- `handleFileSelected`
- `handleUploadStarted`
- `handleRetryClicked`
- `handleCopyLink`

Avoid generic handler names unless the context is extremely obvious.

---

# 4. State Management

## 4.1 Prefer a single source of truth

Do not store many overlapping booleans or derived values independently.

Bad:

- `isUploading`
- `isProcessing`
- `isReady`
- `showSpinner`
- `canPlay`
- `statusMessage`

when most of these can be derived from one authoritative `videoStatus` and a few related fields.

## 4.2 Derive state instead of duplicating it

If one value can be computed from another, prefer deriving it rather than storing both.

Examples:

- derive `showPlayer` from playback availability
- derive status message from `videoStatus`
- derive action button availability from status and request activity

## 4.3 Keep local state local

Use component-local state by default.

Use Svelte stores only when state is truly shared across multiple components or routes.

Do not move everything into stores automatically.

## 4.4 Stores should have a clear reason to exist

Good reasons for stores:

- shared playback state across multiple components
- app-wide notification system
- session-level preferences or shared route data

Bad reasons:

- avoiding prop passing when the component tree is still simple
- making local state global without benefit
- following habit rather than need

---

# 5. Reactivity Guidelines

## 5.1 Use reactivity to clarify, not to hide behavior

Svelte reactive declarations should make derived logic easier to understand.

Good use cases:

- derived display labels
- computed disabled states
- lightweight formatting decisions

## 5.2 Avoid complex reactive chains

Do not create multiple reactive statements that mutate one another in a hard-to-follow sequence.

Bad pattern:

- reactive block updates `statusText`
- another reactive block triggers fetch based on `statusText`
- another reactive block flips `canPlay`
- another reactive block resets errors

This makes behavior hard to trace.

## 5.3 Keep side effects explicit

Network requests, timers, polling, and subscriptions should be visibly initiated in obvious lifecycle or event code rather than hidden inside complicated reactive chains.

---

# 6. Async and Network Logic

## 6.1 Make async flows obvious

The reader should easily understand:

- which user action triggers the request
- what loading state is shown
- what success state is expected
- what happens on failure
- when a retry is possible

## 6.2 Do not bury fetch logic inside large markup components

When network logic grows beyond trivial size, move it into:

- an API client module
- a composable helper
- a route-level load mechanism
- a focused utility function

This keeps components readable.

## 6.3 Keep API calls typed and predictable

Prefer explicit request and response shapes instead of unstructured ad hoc access.

Avoid code like:

```ts
const data = await response.json();
status = data?.x?.y?.z;
```

when a known response interface can be defined.

## 6.4 Handle failure states explicitly

Do not only code the happy path.

The UI should have a clear approach for:

- invalid input
- upload failure
- processing failure
- playback not ready yet
- network timeouts or transient errors

---

# 7. Alignment with Backend Domain States

## 7.1 Reflect backend lifecycle clearly in the UI

If the backend has staged readiness, the frontend should present staged readiness clearly.

Examples:

- upload started
- upload in progress
- upload complete
- processing in progress
- baseline quality streamable now
- more renditions still processing
- fully ready
- failed

Do not reduce all of these to a vague generic loader.

## 7.2 Avoid inventing conflicting frontend-only lifecycle terminology

Frontend language should align with backend vocabulary unless there is a deliberate UX reason to present a more user-friendly label.

Even then, keep internal naming consistent with the domain.

---

# 8. Markup and Template Cleanliness

## 8.1 Keep template logic readable

Avoid very large inline expressions in markup.

If conditional logic becomes complex, move it into named derived values or helper functions.

## 8.2 Use conditional rendering intentionally

Conditionals should correspond to meaningful UI states.

Prefer clean, state-driven blocks over scattered conditionals across the template.

## 8.3 Avoid repetition when it truly obscures intent

If the same UI pattern appears many times, extract a component or helper. Do not extract only to reduce line count; extract when it improves clarity.

## 8.4 Keep DOM structure understandable

Do not generate deeply nested markup unless necessary. A reviewer should be able to scan the template and quickly see the structure of the UI.

---

# 9. Forms and User Actions

## 9.1 Form handling should be explicit

For upload or input flows, make it clear:

- what the user can input
- what validation exists
- when submission is allowed
- what happens after submission

## 9.2 Disable or guard actions appropriately

Prevent duplicate submission or conflicting actions when requests are in progress.

## 9.3 Show useful feedback

User feedback should distinguish:

- upload progress
- processing status
- success
- recoverable failure
- non-recoverable failure

---

# 10. Error and Loading UX

## 10.1 Loading states should be specific

Do not overuse one generic “Loading...” label for everything.

Prefer meaningful labels such as:

- `Uploading video...`
- `Processing video...`
- `Preparing stream...`
- `Retrying upload...`

## 10.2 Error messages should be actionable when possible

Errors shown to users should help them understand what happened and what to do next.

Examples:

- unsupported file type
- upload failed, try again
- video is still processing, check again shortly

## 10.3 Keep internal error details out of the UI unless useful

Do not dump raw stack traces or backend internals into the interface.

---

# 11. TypeScript Use in Svelte

## 11.1 Prefer typed data shapes

When using TypeScript, define clear types or interfaces for:

- API responses
- component props
- shared state objects
- action payloads

## 11.2 Avoid `any` unless absolutely necessary

Prefer precise types, unions, and optional fields that reflect reality.

## 11.3 Use unions for meaningful states when helpful

When frontend state is more than a simple flag, use structured typed state rather than loosely connected booleans.

---

# 12. File and Project Organization

## 12.1 Group files coherently

Use structure that makes responsibilities easy to locate.

Example:

```text
src/
  lib/
    api/
      videos.ts
      uploads.ts
    components/
      UploadForm.svelte
      UploadProgress.svelte
      ProcessingStatus.svelte
      VideoPlayer.svelte
      ShareLinkBox.svelte
    types/
      video.ts
    utils/
      format.ts
  routes/
    upload/
      +page.svelte
    watch/[id]/
      +page.svelte
```

## 12.2 Avoid junk-drawer files

Do not create files like:

- `utils.ts`
- `helpers.ts`
- `common.ts`

that accumulate unrelated logic.

Utility files should remain cohesive.

---

# 13. Comments and Documentation

## 13.1 Comment the “why,” not the obvious “what”

Bad comment:

```ts
// set loading to true
isLoading = true;
```

Useful comment:

```ts
// We allow playback as soon as the baseline rendition is ready,
// even while higher-quality versions are still processing.
```

## 13.2 Keep comments accurate and rare

Comments should add context, not clutter.

Avoid filler comments that restate markup or simple assignments.

---

# 14. Anti-Patterns to Avoid

Do not generate code with these traits unless explicitly requested:

## 14.1 Giant all-in-one page components

A single component containing form logic, progress logic, polling, playback, and many layers of conditionals.

## 14.2 State duplication

Multiple booleans and strings representing the same lifecycle state.

## 14.3 Reactive spaghetti

Too many `$:` blocks with side effects and implicit dependencies.

## 14.4 Inline network logic everywhere

Repeating raw fetch and response parsing across many components.

## 14.5 Vague naming

Names that do not explain state, actions, or domain concepts.

## 14.6 Hidden coupling to backend quirks

Frontend code depending on fragile undocumented response details without clear typing or abstraction.

## 14.7 Happy-path-only UI

No meaningful behavior for in-progress, failed, or partially-ready states.

## 14.8 Overuse of global stores

Globalizing state that should remain local.

---

# 15. LLM-Specific Code Generation Instructions

When generating Svelte code, follow these rules:

## 15.1 Start by identifying the component responsibility

Before writing code, determine:

- what this component owns
- what inputs it receives
- what events it emits or handles
- what network calls it triggers, if any
- what states it must represent

## 15.2 Keep backend integration explicit

Use clear API modules or typed fetch helpers when requests are non-trivial.

## 15.3 Prefer one source of truth for lifecycle state

Do not invent many overlapping UI flags when a structured status exists.

## 15.4 Generate readable templates

Avoid long inline conditions and giant expressions inside markup.

## 15.5 Keep side effects obvious

Timers, polling, retries, and fetches should be easy to locate and reason about.

## 15.6 Code all meaningful UI states

Include:

- idle
- in progress
- partial success or staged readiness
- full success
- failure

whenever the domain supports them.

## 15.7 Avoid overengineering

Do not generate elaborate state architectures or abstraction layers unless there is clear benefit.

## 15.8 Favor reviewer-friendly code

Assume a human evaluator will read the repository and judge:

- readability
- maintainability
- structure
- clarity of async state handling
- alignment with system design

---

# 16. Preferred Output Characteristics

Generated Svelte code should generally have these characteristics:

- focused components
- clear names
- minimal state duplication
- explicit async handling
- readable templates
- typed API interaction when possible
- domain-aligned lifecycle states
- limited and purposeful reactivity
- minimal but useful comments
- no unnecessary cleverness

---

# 17. Final Rule

When in doubt, generate the version that a teammate can understand, review, and safely modify after reading it once.

That is clean Svelte code.
