# Frontend

This directory will contain the Svelte frontend for two core user journeys:
- anonymous upload
- playback by public share link

Planned route shape:

```text
src/routes/
  +page.svelte           upload page
  v/[publicId]/+page.svelte
                        playback page with status polling
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
