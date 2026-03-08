const readyStatuses = new Set(['BASELINE_READY', 'PROCESSING_FULL', 'READY']);

export function isVideoStreamable(status, isStreamable) {
	return Boolean(isStreamable || readyStatuses.has(status));
}

export function describeVideoStatus(status) {
	switch (status) {
		case 'INITIATED':
			return 'Ready to upload';
		case 'UPLOADING':
			return 'Uploading';
		case 'UPLOADED':
			return 'Preparing video';
		case 'PROCESSING_BASELINE':
			return 'Preparing video';
		case 'BASELINE_READY':
			return 'Ready to watch';
		case 'PROCESSING_FULL':
			return 'Ready to watch';
		case 'READY':
			return 'Ready';
		case 'FAILED':
			return 'Video unavailable';
		default:
			return status;
	}
}

export function statusTone(status) {
	switch (status) {
		case 'READY':
		case 'BASELINE_READY':
		case 'PROCESSING_FULL':
			return 'success';
		case 'FAILED':
			return 'danger';
		case 'UPLOADED':
		case 'PROCESSING_BASELINE':
			return 'warning';
		default:
			return 'neutral';
	}
}
