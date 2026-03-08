export function formatBytes(value) {
	if (!Number.isFinite(value) || value <= 0) {
		return '0 B';
	}

	const units = ['B', 'KB', 'MB', 'GB'];
	let currentValue = value;
	let unitIndex = 0;

	while (currentValue >= 1024 && unitIndex < units.length - 1) {
		currentValue /= 1024;
		unitIndex += 1;
	}

	const precision = currentValue >= 10 || unitIndex === 0 ? 0 : 1;
	return `${currentValue.toFixed(precision)} ${units[unitIndex]}`;
}

export function formatDateTime(value) {
	if (!value) {
		return 'Unknown time';
	}

	return new Intl.DateTimeFormat('en-US', {
		dateStyle: 'medium',
		timeStyle: 'short'
	}).format(new Date(value));
}
