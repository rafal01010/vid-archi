import { completeVideoUpload, signUploadParts } from '$lib/api';

export class UploadCancelledError extends Error {
	constructor() {
		super('Upload cancelled');
		this.name = 'UploadCancelledError';
	}
}

export function isUploadCancelledError(error) {
	return error instanceof UploadCancelledError || error?.name === 'AbortError';
}

export async function uploadMultipartVideo({
	file,
	createResponse,
	onProgress,
	onCompleting,
	signal
}) {
	const partSizeBytes = createResponse.partSizeBytes;
	const totalPartCount = Math.ceil(file.size / partSizeBytes);
	const partNumbers = Array.from({ length: totalPartCount }, (_, index) => index + 1);
	throwIfCancelled(signal);
	const signResponse = await signUploadParts(createResponse.videoId, {
		uploadSessionId: createResponse.uploadSessionId,
		partNumbers
	}, { signal });

	const completedParts = [];
	let uploadedBytes = 0;

	for (const part of signResponse.parts) {
		throwIfCancelled(signal);
		const start = (part.partNumber - 1) * partSizeBytes;
		const end = Math.min(start + partSizeBytes, file.size);
		const blob = file.slice(start, end);
		const etag = await uploadSinglePart({
			url: part.url,
			blob,
			contentType: file.type || 'application/octet-stream',
			signal,
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
	throwIfCancelled(signal);

	return completeVideoUpload(createResponse.videoId, {
		uploadSessionId: createResponse.uploadSessionId,
		parts: completedParts
	}, { signal });
}

function throwIfCancelled(signal) {
	if (signal?.aborted) {
		throw new UploadCancelledError();
	}
}

function uploadSinglePart({ url, blob, contentType, signal, onProgress }) {
	return new Promise((resolve, reject) => {
		throwIfCancelled(signal);

		const request = new XMLHttpRequest();
		let settled = false;

		const settleResolve = (value) => {
			if (settled) {
				return;
			}

			settled = true;
			signal?.removeEventListener('abort', handleAbortSignal);
			resolve(value);
		};

		const settleReject = (error) => {
			if (settled) {
				return;
			}

			settled = true;
			signal?.removeEventListener('abort', handleAbortSignal);
			reject(error);
		};

		const handleAbortSignal = () => {
			request.abort();
		};

		request.open('PUT', url);
		request.setRequestHeader('Content-Type', contentType);

		request.upload.addEventListener('progress', (event) => {
			if (event.lengthComputable) {
				onProgress?.(event.loaded);
			}
		});

		request.addEventListener('load', () => {
			if (request.status < 200 || request.status >= 300) {
				settleReject(new Error(`Part upload failed with status ${request.status}`));
				return;
			}

			const etag = request.getResponseHeader('etag') ?? request.getResponseHeader('ETag');

			if (!etag) {
				settleReject(
					new Error(
						'Part upload succeeded but the storage response did not expose an ETag header'
					)
				);
				return;
			}

			settleResolve(etag);
		});

		request.addEventListener('error', () => {
			settleReject(new Error('Network failure while uploading a multipart part'));
		});

		request.addEventListener('abort', () => {
			settleReject(new UploadCancelledError());
		});

		signal?.addEventListener('abort', handleAbortSignal, { once: true });
		request.send(blob);
	});
}
