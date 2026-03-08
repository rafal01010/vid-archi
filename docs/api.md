# API Contract

This document reflects the upload API currently implemented in the Rust backend.

## Upload Session Flow

1. `POST /api/videos`
2. `POST /api/videos/{videoId}/parts/sign`
3. client uploads file parts directly to S3 or MinIO with the returned presigned `PUT` URLs
4. `POST /api/videos/{videoId}/complete`

The API keeps video bytes off the backend request path. The backend only creates metadata, upload sessions, and presigned instructions.

## `POST /api/videos`

Creates the video metadata row, reserves the deterministic share link, opens the multipart upload session in object storage, and returns the upload instructions.

Request:

```json
{
  "filename": "demo.mp4",
  "contentType": "video/mp4",
  "sizeBytes": 52428800,
  "title": "Demo upload"
}
```

Response:

```json
{
  "videoId": "8ee7b885-c177-4438-94f7-f632d4d64af4",
  "publicId": "demo-upload-r3t3x2k4r6w34fhx6yzbivskuq",
  "uploadSessionId": "17dc6b6c-b371-4f81-b341-262d56d6312b",
  "s3UploadId": "example-upload-id",
  "sourceObjectKey": "videos/8ee7b885-c177-4438-94f7-f632d4d64af4/source/original",
  "partSizeBytes": 8388608,
  "uploadExpiresAt": "2026-03-08T14:00:00Z",
  "signPartsEndpoint": "/api/videos/8ee7b885-c177-4438-94f7-f632d4d64af4/parts/sign",
  "completeUploadEndpoint": "/api/videos/8ee7b885-c177-4438-94f7-f632d4d64af4/complete",
  "playbackPath": "/v/demo-upload-r3t3x2k4r6w34fhx6yzbivskuq"
}
```

## `POST /api/videos/{videoId}/parts/sign`

Returns presigned `PUT` URLs for the requested multipart part numbers.

Request:

```json
{
  "uploadSessionId": "17dc6b6c-b371-4f81-b341-262d56d6312b",
  "partNumbers": [1, 2, 3]
}
```

Response:

```json
{
  "videoId": "8ee7b885-c177-4438-94f7-f632d4d64af4",
  "uploadSessionId": "17dc6b6c-b371-4f81-b341-262d56d6312b",
  "expiresAt": "2026-03-08T14:00:00Z",
  "parts": [
    {
      "partNumber": 1,
      "url": "https://...",
      "method": "PUT"
    }
  ]
}
```

## `POST /api/videos/{videoId}/complete`

Completes the multipart upload in object storage and transitions the metadata row to `UPLOADED`.

Request:

```json
{
  "uploadSessionId": "17dc6b6c-b371-4f81-b341-262d56d6312b",
  "parts": [
    {
      "partNumber": 1,
      "etag": "\"8d777f385d3dfec8815d20f7496026dc\""
    }
  ]
}
```

Response:

```json
{
  "videoId": "8ee7b885-c177-4438-94f7-f632d4d64af4",
  "publicId": "demo-upload-r3t3x2k4r6w34fhx6yzbivskuq",
  "status": "UPLOADED",
  "processingJobId": "22cf9b14-4d77-4a5a-a390-f49e7f7f6935",
  "playbackPath": "/v/demo-upload-r3t3x2k4r6w34fhx6yzbivskuq"
}
```

## Status Transition In This Step

- Create upload: `INITIATED`
- First successful part-sign request: `UPLOADING`
- Complete upload: `UPLOADED`

The next checklist step will consume the queued `BASELINE` job and move the video into the processing states.
