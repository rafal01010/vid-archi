# Frontend

This directory now contains the Svelte frontend for two core user journeys:
- anonymous upload from the homepage
- share-page resolution by public link

Planned route shape:

```text
src/routes/
  +page.svelte           homepage with upload + recent-video library
  v/[publicId]/+page.svelte
                        share page with status lookup
```

Frontend state should align directly with backend lifecycle terms such as:
- `INITIATED`
- `UPLOADING`
- `UPLOADED`
- `PROCESSING_BASELINE`
- `BASELINE_READY`
- `READY`
- `FAILED`

Avoid inventing alternate UI-only status names when the backend already has a precise domain term.

## Implemented Frontend Flow

- `/` renders the upload form and the recent-upload library in one page.
- The homepage defaults to 10 videos per page and exposes previous/next controls when older uploads exist.
- The upload form performs:
  - file size validation against the 1GB project limit
  - file type validation for MP4, MOV, WebM, and AVI
  - direct multipart uploads from the browser to S3/MinIO using presigned `PUT` URLs
  - progress reporting during part uploads
  - share-link display after upload completion
- The homepage library calls `GET /api/videos` and renders videos newest first.
- Clicking a library card opens `/v/[publicId]`, which calls `GET /api/videos/{publicId}` and shows the current lifecycle state.
- The UI theme is monochrome only: black, white, and gray.

## Local Run

Preferred local command:

```bash
./scripts/local/deploy-local-lite.sh
```

That local wrapper installs frontend dependencies when needed, builds the app, and starts the packaged frontend on `http://127.0.0.1:4173`.

The frontend expects `PUBLIC_API_BASE_URL` to point at the Rust API. The root [.env.example](/Users/dave/LabBase/vid-archi/.env.example) now includes a local default:

```bash
PUBLIC_API_BASE_URL=http://127.0.0.1:8080
```

Script helpers:

```bash
./frontend/scripts/deploy-frontend.sh --run-check
./frontend/scripts/run-stg-frontend.sh
./scripts/local/deploy-local-lite.sh
./scripts/local/start-local-lite.sh
./scripts/local/stop-local-lite.sh
```

The scripts:
- load env vars from `.env` by default or from `--env-file`
- export `PUBLIC_API_BASE_URL` before running npm commands
- support local development and STG build/start handoff on EC2
- default to `http://${STG_ALB_DNS}` in STG when that value exists
- fall back to same-origin relative `/api` only when you intentionally deploy a reverse proxy that fronts both the frontend and API on one origin
