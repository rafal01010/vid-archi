<script>
	import { onDestroy } from 'svelte';
	import { loadHlsConstructor } from '$lib/video/hlsLoader';

	export let manifestUrl = null;
	export let qualityOptions = [];
	export let defaultQuality = 'auto';
	export let selectedQuality = 'auto';

	let videoElement;
	let hlsInstance = null;
	let playbackError = '';
	let currentAttachmentKey = '';
	$: qualityChoices = buildQualityChoices(manifestUrl, qualityOptions);
	$: if (!selectedQuality) {
		selectedQuality = defaultQuality || 'auto';
	}
	$: if (!qualityChoices.some((choice) => choice.value === selectedQuality)) {
		selectedQuality = qualityChoices[0]?.value ?? 'auto';
	}
	$: {
		const hasMountedPlayer = Boolean(videoElement);
		const activeManifestUrl = manifestUrl;
		const availableQualityOptions = qualityOptions;
		const activeQuality = selectedQuality;

		if (hasMountedPlayer) {
			void syncPlayerSource(activeManifestUrl, availableQualityOptions, activeQuality);
		}
	}

	onDestroy(() => {
		destroyHlsInstance();
	});

	function buildQualityChoices(activeManifestUrl, options) {
		const choices = [];

		if (activeManifestUrl) {
			choices.push({
				value: 'auto',
				label: 'Auto'
			});
		}

		for (const option of options) {
			choices.push({
				value: option.name,
				label: option.label
			});
		}

		return choices;
	}

	function resolveSelectedSourceUrl(activeManifestUrl, options, activeQuality) {
		if (activeQuality === 'auto') {
			return activeManifestUrl ?? '';
		}

		return options.find((option) => option.name === activeQuality)?.playlistUrl ?? activeManifestUrl ?? '';
	}

	async function syncPlayerSource(activeManifestUrl, availableQualityOptions, activeQuality) {
		const useNativeHls = supportsNativeHls();
		const sourceUrl = resolvePlaybackSource(
			activeManifestUrl,
			availableQualityOptions,
			activeQuality
		);
		const attachmentKey = buildAttachmentKey(sourceUrl, useNativeHls);

		if (!sourceUrl) {
			currentAttachmentKey = '';
			destroyHlsInstance();
			if (videoElement) {
				videoElement.removeAttribute('src');
				videoElement.load();
			}
			return;
		}

		if (attachmentKey === currentAttachmentKey) {
			return;
		}

		const resumeTime = videoElement.currentTime || 0;
		const shouldResumePlayback = !videoElement.paused && !videoElement.ended;

		playbackError = '';

		try {
			await attachSourceToPlayer(sourceUrl, resumeTime, shouldResumePlayback, useNativeHls);
			currentAttachmentKey = attachmentKey;
		} catch (error) {
			playbackError =
				error instanceof Error ? error.message : 'The browser could not start HLS playback.';
		}
	}

	function resolvePlaybackSource(activeManifestUrl, options, activeQuality) {
		return resolveSelectedSourceUrl(activeManifestUrl, options, activeQuality);
	}

	function buildAttachmentKey(sourceUrl, useNativeHls) {
		if (!sourceUrl) {
			return '';
		}

		return `${useNativeHls ? 'native' : 'hlsjs'}:${sourceUrl}`;
	}

	async function attachSourceToPlayer(sourceUrl, resumeTime, shouldResumePlayback, useNativeHls) {
		destroyHlsInstance();
		resetVideoElement();

		if (useNativeHls) {
			attachNativeSource(sourceUrl, resumeTime, shouldResumePlayback);
			return;
		}

		const HlsConstructor = await loadHlsConstructor();

		if (!HlsConstructor || !HlsConstructor.isSupported()) {
			throw new Error('This browser does not support HLS playback.');
		}

		const hls = new HlsConstructor({
			enableWorker: true
		});

		hls.on(HlsConstructor.Events.MANIFEST_PARSED, async () => {
			await restorePlaybackState(resumeTime, shouldResumePlayback);
		});
		hls.on(HlsConstructor.Events.ERROR, (_event, data) => {
			if (!data?.fatal) {
				return;
			}

			if (data.type === HlsConstructor.ErrorTypes.NETWORK_ERROR) {
				playbackError = 'Temporary network issue detected. Retrying stream fetch.';
				hls.startLoad();
				return;
			}

			if (data.type === HlsConstructor.ErrorTypes.MEDIA_ERROR) {
				playbackError = 'Temporary media decode issue detected. Recovering playback.';
				hls.recoverMediaError();
				return;
			}

			playbackError = 'The HLS stream encountered an unrecoverable playback error.';
			hls.destroy();
			hlsInstance = null;
		});

		hls.loadSource(sourceUrl);
		hls.attachMedia(videoElement);
		hlsInstance = hls;
	}

	function attachNativeSource(sourceUrl, resumeTime, shouldResumePlayback) {
		videoElement.src = sourceUrl;
		videoElement.load();
		videoElement.addEventListener(
			'loadedmetadata',
			() => {
				void restorePlaybackState(resumeTime, shouldResumePlayback);
			},
			{ once: true }
		);
	}

	function supportsNativeHls() {
		if (!videoElement?.canPlayType) {
			return false;
		}

		return Boolean(
			videoElement.canPlayType('application/vnd.apple.mpegurl') ||
				videoElement.canPlayType('application/x-mpegURL')
		);
	}

	function resetVideoElement() {
		if (!videoElement) {
			return;
		}

		videoElement.pause();
		videoElement.removeAttribute('src');
		videoElement.load();
	}

	async function restorePlaybackState(resumeTime, shouldResumePlayback) {
		if (resumeTime > 0) {
			try {
				videoElement.currentTime = resumeTime;
			} catch {
				// Ignore seek failures while the browser is still priming metadata.
			}
		}

		if (shouldResumePlayback) {
			try {
				await videoElement.play();
			} catch {
				// Autoplay might still be blocked after a source swap.
			}
		}
	}

	function destroyHlsInstance() {
		if (hlsInstance) {
			hlsInstance.destroy();
			hlsInstance = null;
		}
	}
</script>

<div class="player-shell">
	<video bind:this={videoElement} class="video-frame" controls playsinline preload="metadata">
		Your browser does not support HTML5 video playback.
	</video>

	<div class="player-footer">
		{#if playbackError}
			<p class="player-error">{playbackError}</p>
		{:else}
			<div></div>
		{/if}

		{#if qualityChoices.length > 0}
			<label class="quality-picker">
				<span>Quality</span>
				<select bind:value={selectedQuality} aria-label="Choose playback quality">
					{#each qualityChoices as choice}
						<option value={choice.value}>{choice.label}</option>
					{/each}
				</select>
			</label>
		{/if}
	</div>
</div>

<style>
	.player-shell {
		display: grid;
		gap: 16px;
	}

	.quality-picker {
		display: grid;
		gap: 8px;
		font-size: 0.86rem;
		font-weight: 700;
		justify-items: end;
	}

	.quality-picker select {
		min-width: 124px;
		padding: 10px 12px;
		border-radius: 14px;
		border: 1px solid rgba(0, 0, 0, 0.14);
		background: rgba(255, 255, 255, 0.95);
		font: inherit;
	}

	.video-frame {
		display: block;
		width: 100%;
		border-radius: 22px;
		background: #05080d;
		aspect-ratio: 16 / 9;
		object-fit: contain;
	}

	.player-footer {
		display: flex;
		justify-content: space-between;
		align-items: start;
		gap: 16px;
	}

	.player-error {
		margin: 0;
		color: var(--danger);
		font-weight: 700;
	}

	@media (max-width: 720px) {
		.player-footer {
			align-items: stretch;
			flex-direction: column;
		}

		.quality-picker {
			width: 100%;
			justify-items: stretch;
		}

		.quality-picker select {
			width: 100%;
		}
	}
</style>
