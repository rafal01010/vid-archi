const HLS_SCRIPT_URL = 'https://cdn.jsdelivr.net/npm/hls.js@1.5.20/dist/hls.min.js';

let hlsConstructorPromise = null;

export async function loadHlsConstructor() {
	if (typeof window === 'undefined') {
		return null;
	}

	if (window.Hls) {
		return window.Hls;
	}

	if (!hlsConstructorPromise) {
		hlsConstructorPromise = new Promise((resolve, reject) => {
			const existingScript = document.querySelector('script[data-hls-js-fallback="true"]');

			if (existingScript) {
				if (existingScript.dataset.loadState === 'loaded') {
					resolve(window.Hls ?? null);
					return;
				}

				if (existingScript.dataset.loadState === 'failed') {
					hlsConstructorPromise = null;
					reject(new Error('Failed to load the HLS.js playback fallback.'));
					return;
				}

				wireScriptEvents(existingScript, resolve, reject);
				return;
			}

			const script = document.createElement('script');
			script.src = HLS_SCRIPT_URL;
			script.async = true;
			script.defer = true;
			script.dataset.hlsJsFallback = 'true';
			wireScriptEvents(script, resolve, reject);
			document.head.appendChild(script);
		});
	}

	return hlsConstructorPromise;
}

function wireScriptEvents(script, resolve, reject) {
	const handleLoad = () => {
		script.dataset.loadState = 'loaded';
		resolve(window.Hls ?? null);
	};
	const handleError = () => {
		script.dataset.loadState = 'failed';
		hlsConstructorPromise = null;
		reject(new Error('Failed to load the HLS.js playback fallback.'));
	};

	script.addEventListener('load', handleLoad, { once: true });
	script.addEventListener('error', handleError, { once: true });
}
