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
			return 'Upload received';
		case 'PROCESSING_BASELINE':
			return 'Preparing video';
		case 'BASELINE_READY':
			return 'Ready to watch';
		case 'PROCESSING_FULL':
			return 'More quality options are still being prepared';
		case 'READY':
			return 'Ready to watch';
		case 'FAILED':
			return 'Playback issue';
		default:
			return 'Updating';
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
