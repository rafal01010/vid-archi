export const MAX_UPLOAD_SIZE_BYTES = 1024 * 1024 * 1024;

export const ALLOWED_VIDEO_MIME_TYPES = new Set([
	'video/mp4',
	'video/quicktime',
	'video/webm',
	'video/x-msvideo',
	'video/vnd.avi'
]);

export const ALLOWED_VIDEO_EXTENSIONS = new Set(['.mp4', '.mov', '.webm', '.avi']);
