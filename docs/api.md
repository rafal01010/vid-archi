# API Contract

This document reflects the API currently implemented in the Rust backend and the split baseline-processing pipeline formed by `chunker` plus `transcoder`.

## Homepage And Share-Page Read Flow

1. `GET /api/videos?page=1&pageSize=10`
2. `GET /api/videos/{publicId}`

The frontend currently uses these endpoints to:
- upload a new video from `/`
- refresh the recent-video library after upload completion
- open `/v/{publicId}` for share-page status

## `GET /api/videos?page=1&pageSize=10`

Returns the most recent videos ordered by `created_at DESC`.

Response:

```json
{
  "page": 1,
  "pageSize": 10,
  "totalCount": 37,
  "totalPages": 4,
  "hasPreviousPage": false,
  "hasNextPage": true,
  "videos": [
    {
      "publicId": "demo-upload-r3t3x2k4r6w34fhx6yzbivskuq",
      "title": "Demo upload",
      "originalFilename": "demo.mp4",
      "status": "UPLOADED",
      "isStreamable": false,
      "createdAt": "2026-03-08T14:00:00Z",
      "updatedAt": "2026-03-08T14:03:00Z",
      "playbackPath": "/v/demo-upload-r3t3x2k4r6w34fhx6yzbivskuq"
    }
  ]
}
```

## `GET /api/videos/{publicId}`

Returns the current lifecycle state and share-page metadata for a public video.

The endpoint behavior is now:
- resolve the stable `publicId`
- read `status`, `is_streamable`, and `manifest_s3_key` from Postgres
- build `manifestUrl` from `PROCESSED_ASSET_BASE_URL`, `CDN_BASE_URL`, or the local processed-bucket base URL
- expose `manifestUrl` only after the baseline transcoder has published the full `360p` VOD-style HLS package and marked the video `BASELINE_READY`

Response before baseline is ready:

```json
{
  "publicId": "demo-upload-r3t3x2k4r6w34fhx6yzbivskuq",
  "title": "Demo upload",
  "originalFilename": "demo.mp4",
  "status": "UPLOADED",
  "isStreamable": false,
  "createdAt": "2026-03-08T14:00:00Z",
  "updatedAt": "2026-03-08T14:03:00Z",
  "playbackPath": "/v/demo-upload-r3t3x2k4r6w34fhx6yzbivskuq",
  "manifestUrl": null
}
```

Response after baseline is ready:

```json
{
  "publicId": "demo-upload-r3t3x2k4r6w34fhx6yzbivskuq",
  "title": "Demo upload",
  "originalFilename": "demo.mp4",
  "status": "BASELINE_READY",
  "isStreamable": true,
  "createdAt": "2026-03-08T14:00:00Z",
  "updatedAt": "2026-03-08T14:05:30Z",
  "playbackPath": "/v/demo-upload-r3t3x2k4r6w34fhx6yzbivskuq",
  "manifestUrl": "https://d38ixt0cyn1hi6.cloudfront.net/videos/8ee7b885-c177-4438-94f7-f632d4d64af4/hls/master.m3u8"
}
```

## Upload Session Flow

1. `POST /api/videos`
2. `POST /api/videos/{videoId}/parts/sign`
3. client uploads file parts directly to S3 or MinIO
4. `POST /api/videos/{videoId}/complete`

The API only creates metadata, upload sessions, and presigned instructions. Video bytes do not pass through the backend.

## `POST /api/videos`

Creates the video metadata row, reserves the deterministic share link, opens the multipart upload in object storage, and returns the upload instructions.

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

Completes the multipart upload and queues the baseline processing flow.

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

## Status Transition In The Current Implementation

- create upload: `INITIATED`
- first part-sign request: `UPLOADING`
- complete upload: `UPLOADED`
- chunker claim: `PROCESSING_BASELINE`
- baseline transcoder publishes the full `360p` playlist and its segments: `BASELINE_READY`

The existing share-page endpoint is the current playback metadata source. Once `manifest_s3_key` is present, it returns `manifestUrl` for the baseline stream.
