# API Contract

All API responses are JSON. Clients may send `x-correlation-id`; the API will echo it back and use it for downstream processing logs.

## Health

`GET /healthz`

Response:

```json
{
  "status": "ok"
}
```

## Create Video

`POST /api/videos`

Request:

```json
{
  "title": "Quarterly update",
  "filename": "quarterly-update.mp4",
  "contentType": "video/mp4",
  "sizeBytes": 734003200,
  "deleteCode": "remove-demo-2026"
}
```

Response:

```json
{
  "videoId": "8ee7b885-c177-4438-94f7-f632d4d64af4",
  "publicId": "quarterly-update-r3t3x2k4r6w34fhx6yzbivskuq",
  "uploadSessionId": "9b0dfc69-8a0a-43d2-8e25-fbd3d31fccd3",
  "s3UploadId": "example-upload-id",
  "sourceObjectKey": "videos/8ee7b885-c177-4438-94f7-f632d4d64af4/source/original",
  "partSizeBytes": 8388608,
  "uploadExpiresAt": "2026-03-10T12:00:00Z",
  "signPartsEndpoint": "/api/videos/8ee7b885-c177-4438-94f7-f632d4d64af4/parts/sign",
  "completeUploadEndpoint": "/api/videos/8ee7b885-c177-4438-94f7-f632d4d64af4/complete",
  "playbackPath": "/v/quarterly-update-r3t3x2k4r6w34fhx6yzbivskuq"
}
```

## Sign Multipart Parts

`POST /api/videos/{videoId}/parts/sign`

Request:

```json
{
  "uploadSessionId": "9b0dfc69-8a0a-43d2-8e25-fbd3d31fccd3",
  "partNumbers": [1, 2, 3]
}
```

Response:

```json
{
  "videoId": "8ee7b885-c177-4438-94f7-f632d4d64af4",
  "uploadSessionId": "9b0dfc69-8a0a-43d2-8e25-fbd3d31fccd3",
  "expiresAt": "2026-03-10T12:00:00Z",
  "parts": [
    {
      "partNumber": 1,
      "url": "https://..."
    },
    {
      "partNumber": 2,
      "url": "https://..."
    }
  ]
}
```

## Complete Upload

`POST /api/videos/{videoId}/complete`

Request:

```json
{
  "uploadSessionId": "9b0dfc69-8a0a-43d2-8e25-fbd3d31fccd3",
  "parts": [
    {
      "partNumber": 1,
      "etag": "\"etag-part-1\""
    },
    {
      "partNumber": 2,
      "etag": "\"etag-part-2\""
    }
  ]
}
```

Response:

```json
{
  "videoId": "8ee7b885-c177-4438-94f7-f632d4d64af4",
  "publicId": "quarterly-update-r3t3x2k4r6w34fhx6yzbivskuq",
  "status": "UPLOADED",
  "processingJobId": "87d79fb7-81c0-4611-96cb-c5bad6bb3229",
  "playbackPath": "/v/quarterly-update-r3t3x2k4r6w34fhx6yzbivskuq"
}
```

## List Recent Videos

`GET /api/videos?page=1&pageSize=10`

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
      "publicId": "quarterly-update-r3t3x2k4r6w34fhx6yzbivskuq",
      "title": "Quarterly update",
      "originalFilename": "quarterly-update.mp4",
      "status": "UPLOADED",
      "isStreamable": false,
      "createdAt": "2026-03-08T14:00:00Z",
      "updatedAt": "2026-03-08T14:03:00Z",
      "playbackPath": "/v/quarterly-update-r3t3x2k4r6w34fhx6yzbivskuq"
    }
  ]
}
```

## Video Details

`GET /api/videos/{publicId}`

Response:

```json
{
  "publicId": "quarterly-update-r3t3x2k4r6w34fhx6yzbivskuq",
  "title": "Quarterly update",
  "originalFilename": "quarterly-update.mp4",
  "status": "BASELINE_READY",
  "isStreamable": true,
  "createdAt": "2026-03-08T14:00:00Z",
  "updatedAt": "2026-03-08T14:05:30Z",
  "playbackPath": "/v/quarterly-update-r3t3x2k4r6w34fhx6yzbivskuq",
  "manifestUrl": "https://d38ixt0cyn1hi6.cloudfront.net/videos/8ee7b885-c177-4438-94f7-f632d4d64af4/hls/master.m3u8",
  "errorCode": null,
  "errorMessage": null
}
```

## Delete Video

`DELETE /api/videos/{publicId}`

Request:

```json
{
  "deleteCode": "remove-demo-2026"
}
```

Response:

```json
{
  "publicId": "quarterly-update-r3t3x2k4r6w34fhx6yzbivskuq",
  "deletedUploadObjectCount": 1,
  "deletedProcessedObjectCount": 18
}
```

## Playback Metadata

`GET /api/videos/{publicId}/playback`

Response:

```json
{
  "publicId": "quarterly-update-r3t3x2k4r6w34fhx6yzbivskuq",
  "title": "Quarterly update",
  "originalFilename": "quarterly-update.mp4",
  "status": "PROCESSING_FULL",
  "isStreamable": true,
  "createdAt": "2026-03-08T14:00:00Z",
  "updatedAt": "2026-03-08T14:07:00Z",
  "playbackPath": "/v/quarterly-update-r3t3x2k4r6w34fhx6yzbivskuq",
  "manifestUrl": "https://d38ixt0cyn1hi6.cloudfront.net/videos/8ee7b885-c177-4438-94f7-f632d4d64af4/hls/master.m3u8",
  "errorCode": null,
  "errorMessage": null,
  "defaultQuality": "auto",
  "pollIntervalMs": 5000,
  "availableQualities": [
    {
      "name": "360p",
      "label": "360p",
      "width": 640,
      "height": 360,
      "codec": "h264",
      "container": "mpegts",
      "playlistUrl": "https://d38ixt0cyn1hi6.cloudfront.net/videos/8ee7b885-c177-4438-94f7-f632d4d64af4/hls/360p/360p.m3u8"
    },
    {
      "name": "720p",
      "label": "720p",
      "width": 1280,
      "height": 720,
      "codec": "h264",
      "container": "mpegts",
      "playlistUrl": "https://d38ixt0cyn1hi6.cloudfront.net/videos/8ee7b885-c177-4438-94f7-f632d4d64af4/hls/720p/720p.m3u8"
    }
  ]
}
```

## Failure Semantics

- `BASELINE_READY`, `PROCESSING_FULL`, and `READY` are playable states.
- `FAILED` may still be playable if the baseline rendition was already published before a later rendition failed.
- `errorCode` and `errorMessage` are safe to surface in the web application.
- `DELETE /api/videos/{publicId}` returns `403` when the delete code does not match the stored hash.
- `DELETE /api/videos/{publicId}` returns `409` while the video is in `PROCESSING_BASELINE` or `PROCESSING_FULL` so workers cannot recreate artifacts after a delete request.
