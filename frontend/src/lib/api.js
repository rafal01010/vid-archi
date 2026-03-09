import { env } from '$env/dynamic/public';

const normalizedApiBaseUrl = env.PUBLIC_API_BASE_URL
	? env.PUBLIC_API_BASE_URL.replace(/\/$/, '')
	: '';

async function sendJsonRequest(path, options = {}) {
	const response = await fetch(`${normalizedApiBaseUrl}${path}`, {
		...options,
		headers: {
			'Content-Type': 'application/json',
			...(options.headers ?? {})
		}
	});

	if (!response.ok) {
		const errorBody = await readErrorBody(response);
		throw new Error(errorBody);
	}

	return response.json();
}

async function readErrorBody(response) {
	try {
		const data = await response.json();
		return data?.error?.message ?? `Request failed with status ${response.status}`;
	} catch {
		return `Request failed with status ${response.status}`;
	}
}

export function listRecentVideos({ page = 1, pageSize = 10 } = {}) {
	return sendJsonRequest(`/api/videos?page=${page}&pageSize=${pageSize}`);
}

export function getVideoDetails(publicId) {
	return sendJsonRequest(`/api/videos/${encodeURIComponent(publicId)}`);
}

export function getVideoPlayback(publicId) {
	return sendJsonRequest(`/api/videos/${encodeURIComponent(publicId)}/playback`);
}

export function createVideoUpload(payload, options = {}) {
	return sendJsonRequest('/api/videos', {
		...options,
		method: 'POST',
		body: JSON.stringify(payload)
	});
}

export function deleteVideo(publicId, payload, options = {}) {
	return sendJsonRequest(`/api/videos/${encodeURIComponent(publicId)}`, {
		...options,
		method: 'DELETE',
		body: JSON.stringify(payload)
	});
}

export function signUploadParts(videoId, payload, options = {}) {
	return sendJsonRequest(`/api/videos/${videoId}/parts/sign`, {
		...options,
		method: 'POST',
		body: JSON.stringify(payload)
	});
}

export function completeVideoUpload(videoId, payload, options = {}) {
	return sendJsonRequest(`/api/videos/${videoId}/complete`, {
		...options,
		method: 'POST',
		body: JSON.stringify(payload)
	});
}
