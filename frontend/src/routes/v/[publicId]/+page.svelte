<script>
	import { onDestroy, onMount } from 'svelte';
	import { page } from '$app/stores';
	import { getVideoPlayback } from '$lib/api';
	import { formatDateTime } from '$lib/formatters';
	import ShareLinkBox from '$lib/components/ShareLinkBox.svelte';
	import StatusBadge from '$lib/components/StatusBadge.svelte';
	import VideoPlayer from '$lib/components/VideoPlayer.svelte';
	import { describeVideoStatus, isVideoStreamable } from '$lib/status';

	let playback = null;
	let pageState = 'loading';
	let errorMessage = '';
	let refreshTimer = null;

	onMount(() => {
		void loadPlayback();
	});

	onDestroy(() => {
		clearRefreshTimer();
	});

	async function loadPlayback({ silent = false } = {}) {
		if (!silent) {
			pageState = 'loading';
		}

		errorMessage = '';

		try {
			playback = await getVideoPlayback($page.params.publicId);
			pageState = 'ready';
			scheduleRefresh(playback);
		} catch (error) {
			clearRefreshTimer();
			pageState = 'error';
			errorMessage = error instanceof Error ? error.message : 'Failed to load the video.';
		}
	}

	function scheduleRefresh(currentPlayback) {
		clearRefreshTimer();

		if (!shouldPollPlayback(currentPlayback)) {
			return;
		}

		refreshTimer = window.setTimeout(() => {
			void loadPlayback({ silent: true });
		}, currentPlayback.pollIntervalMs ?? 5000);
	}

	function shouldPollPlayback(currentPlayback) {
		if (!currentPlayback) {
			return false;
		}

		if (currentPlayback.status === 'FAILED') {
			return false;
		}

		return currentPlayback.status !== 'READY';
	}

	function clearRefreshTimer() {
		if (refreshTimer) {
			window.clearTimeout(refreshTimer);
			refreshTimer = null;
		}
	}
</script>

<div class="page-shell">
	<section class="panel playback-shell">
		<a class="back-link" href="/">Back to homepage</a>

		{#if pageState === 'loading'}
			<div class="empty-state">Loading video.</div>
		{:else if pageState === 'error'}
			<div class="empty-state">{errorMessage}</div>
		{:else if playback}
			<div class="playback-header">
				<div>
					<p class="eyebrow">Video</p>
					<h1 class="section-title">{playback.title || playback.originalFilename}</h1>
					<p class="section-copy">{describeVideoStatus(playback.status)}</p>
				</div>
				<StatusBadge status={playback.status} />
			</div>

			<div class="details-grid">
				<div class="details-card">
					<p class="detail-label">Uploaded</p>
					<p class="detail-value">{formatDateTime(playback.createdAt)}</p>
				</div>
				<div class="details-card">
					<p class="detail-label">Processing State</p>
					<p class="detail-value">{playback.status}</p>
				</div>
			</div>

			<ShareLinkBox sharePath={playback.playbackPath} />

			{#if isVideoStreamable(playback.status, playback.isStreamable) && playback.manifestUrl}
				<div class="player-card">
					<VideoPlayer
						title={playback.title || playback.originalFilename}
						manifestUrl={playback.manifestUrl}
						qualityOptions={playback.availableQualities}
						defaultQuality={playback.defaultQuality}
					/>
				</div>
			{:else}
				<div class="player-placeholder">
					<p class="detail-label">Availability</p>
					<p class="detail-value">
						This video is still being prepared. Check back again in a little while.
					</p>
				</div>
			{/if}

			{#if playback.status !== 'READY' && playback.status !== 'FAILED'}
				<div class="polling-note">
					<p class="detail-label">Live refresh</p>
					<p class="detail-value">
						This page refreshes playback metadata every
						{Math.round((playback.pollIntervalMs ?? 5000) / 1000)} seconds so the player picks up
						new renditions as processing continues.
					</p>
				</div>
			{/if}
		{/if}
	</section>
</div>

<style>
	.playback-shell {
		padding: clamp(22px, 4vw, 34px);
	}

	.back-link {
		display: inline-flex;
		margin-bottom: 18px;
		color: var(--accent);
		font-weight: 700;
	}

	.playback-header {
		display: flex;
		justify-content: space-between;
		gap: 18px;
		align-items: start;
		margin-bottom: 24px;
	}

	.details-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
		gap: 14px;
		margin-bottom: 18px;
	}

	.details-card,
	.player-placeholder,
	.player-card,
	.polling-note {
		padding: 20px;
		border-radius: 22px;
		background: rgba(255, 255, 255, 0.84);
		border: 1px solid rgba(0, 0, 0, 0.1);
	}

	.player-card,
	.player-placeholder,
	.polling-note {
		margin-top: 18px;
	}

	.detail-label {
		margin: 0 0 8px;
		font-size: 0.8rem;
		font-weight: 700;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--text-muted);
	}

	.detail-value {
		margin: 0;
		word-break: break-word;
	}

	@media (max-width: 720px) {
		.playback-header {
			flex-direction: column;
		}
	}
</style>
