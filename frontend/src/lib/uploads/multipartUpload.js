import { completeVideoUpload, signUploadParts } from '$lib/api';

export async function uploadMultipartVideo({ file, createResponse, onProgress, onCompleting }) {
	const partSizeBytes = createResponse.partSizeBytes;
	const totalPartCount = Math.ceil(file.size / partSizeBytes);
	const partNumbers = Array.from({ length: totalPartCount }, (_, index) => index + 1);
	const signResponse = await signUploadParts(createResponse.videoId, {
		uploadSessionId: createResponse.uploadSessionId,
		partNumbers
	});

	const completedParts = [];
	let uploadedBytes = 0;

	for (const part of signResponse.parts) {
		const start = (part.partNumber - 1) * partSizeBytes;
		const end = Math.min(start + partSizeBytes, file.size);
		const blob = file.slice(start, end);
		const etag = await uploadSinglePart({
			url: part.url,
			blob,
			contentType: file.type || 'application/octet-stream',
			onProgress: (loadedBytes) => {
				onProgress?.(uploadedBytes + loadedBytes, file.size);
			}
		});

		uploadedBytes += blob.size;
		onProgress?.(uploadedBytes, file.size);
		completedParts.push({
			partNumber: part.partNumber,
			etag
		});
	}

	onCompleting?.();

	return completeVideoUpload(createResponse.videoId, {
		uploadSessionId: createResponse.uploadSessionId,
		parts: completedParts
	});
}

function uploadSinglePart({ url, blob, contentType, onProgress }) {
	return new Promise((resolve, reject) => {
		const request = new XMLHttpRequest();

		request.open('PUT', url);
		request.setRequestHeader('Content-Type', contentType);

		request.upload.addEventListener('progress', (event) => {
			if (event.lengthComputable) {
				onProgress?.(event.loaded);
			}
		});

		request.addEventListener('load', () => {
			if (request.status < 200 || request.status >= 300) {
				reject(new Error(`Part upload failed with status ${request.status}`));
				return;
			}

			const etag = request.getResponseHeader('etag') ?? request.getResponseHeader('ETag');

			if (!etag) {
				reject(
					new Error(
						'Part upload succeeded but the storage response did not expose an ETag header'
					)
				);
				return;
			}

			resolve(etag);
		});

		request.addEventListener('error', () => {
			reject(new Error('Network failure while uploading a multipart part'));
		});

		request.send(blob);
	});
}
