<script>
	import { onDestroy } from 'svelte';
	import { loadHlsConstructor } from '$lib/video/hlsLoader';

	export let manifestUrl = null;
	export let qualityOptions = [];
	export let defaultQuality = 'auto';
	export let title = 'Video';

	let videoElement;
	let hlsInstance = null;
	let playbackError = '';
	let currentSourceUrl = '';
	let selectedQuality = defaultQuality;

	$: qualityChoices = buildQualityChoices(manifestUrl, qualityOptions);
	$: if (!qualityChoices.some((choice) => choice.value === selectedQuality)) {
		selectedQuality = qualityChoices[0]?.value ?? 'auto';
	}
	$: selectedSourceUrl = resolveSelectedSourceUrl(manifestUrl, qualityOptions, selectedQuality);
	$: if (videoElement) {
		void syncPlayerSource();
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

	async function syncPlayerSource() {
		if (!selectedSourceUrl) {
			currentSourceUrl = '';
			destroyHlsInstance();
			if (videoElement) {
				videoElement.removeAttribute('src');
				videoElement.load();
			}
			return;
		}

		if (selectedSourceUrl === currentSourceUrl) {
			return;
		}

		const resumeTime = videoElement.currentTime || 0;
		const shouldResumePlayback = !videoElement.paused && !videoElement.ended;

		playbackError = '';

		try {
			await attachSourceToPlayer(selectedSourceUrl, resumeTime, shouldResumePlayback);
			currentSourceUrl = selectedSourceUrl;
		} catch (error) {
			playbackError =
				error instanceof Error ? error.message : 'The browser could not start HLS playback.';
		}
	}

	async function attachSourceToPlayer(sourceUrl, resumeTime, shouldResumePlayback) {
		destroyHlsInstance();

		if (supportsNativeHls()) {
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
	<div class="player-toolbar">
		<div>
			<p class="toolbar-label">Playback</p>
			<p class="toolbar-copy">
				Auto uses the master manifest for adaptive bitrate. Choosing a resolution locks playback to
				that rendition playlist.
			</p>
		</div>

		{#if qualityChoices.length > 0}
			<label class="quality-picker">
				<span>Quality</span>
				<select bind:value={selectedQuality}>
					{#each qualityChoices as choice}
						<option value={choice.value}>{choice.label}</option>
					{/each}
				</select>
			</label>
		{/if}
	</div>

	<video bind:this={videoElement} class="video-frame" controls playsinline preload="metadata">
		Your browser does not support HTML5 video playback.
	</video>

	{#if playbackError}
		<p class="player-error">{playbackError}</p>
	{/if}

	{#if selectedSourceUrl}
		<p class="stream-hint">
			{#if selectedQuality === 'auto'}
				Streaming <strong>{title}</strong> with adaptive bitrate enabled.
			{:else}
				Streaming <strong>{title}</strong> locked to <strong>{selectedQuality}</strong>.
			{/if}
		</p>
	{/if}
</div>

<style>
	.player-shell {
		display: grid;
		gap: 16px;
	}

	.player-toolbar {
		display: flex;
		justify-content: space-between;
		gap: 18px;
		align-items: end;
	}

	.toolbar-label {
		margin: 0 0 6px;
		font-size: 0.8rem;
		font-weight: 700;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--text-muted);
	}

	.toolbar-copy {
		margin: 0;
		max-width: 48rem;
		color: var(--text-muted);
	}

	.quality-picker {
		display: grid;
		gap: 8px;
		font-size: 0.86rem;
		font-weight: 700;
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
		width: 100%;
		border-radius: 22px;
		background: #05080d;
		aspect-ratio: 16 / 9;
	}

	.player-error {
		margin: 0;
		color: var(--danger);
		font-weight: 700;
	}

	.stream-hint {
		margin: 0;
		color: var(--text-muted);
	}

	@media (max-width: 720px) {
		.player-toolbar {
			align-items: stretch;
			flex-direction: column;
		}

		.quality-picker {
			width: 100%;
		}

		.quality-picker select {
			width: 100%;
		}
	}
</style>
