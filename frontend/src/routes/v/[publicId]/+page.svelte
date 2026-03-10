<script>
	import { goto } from '$app/navigation';
	import { onDestroy, onMount } from 'svelte';
	import { page } from '$app/stores';
	import { getVideoPlayback } from '$lib/api';
	import DeleteVideoPanel from '$lib/components/DeleteVideoPanel.svelte';
	import { formatDateTime } from '$lib/formatters';
	import ShareLinkBox from '$lib/components/ShareLinkBox.svelte';
	import VideoPlayer from '$lib/components/VideoPlayer.svelte';
	import { describeVideoStatus, isVideoStreamable } from '$lib/status';

	let playback = null;
	let pageState = 'loading';
	let errorMessage = '';
	let refreshTimer = null;
	let selectedQuality = 'auto';

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

	function statusCopy(currentPlayback) {
		if (!currentPlayback) {
			return '';
		}

		if (currentPlayback.status === 'FAILED' && currentPlayback.isStreamable) {
			return 'Ready to watch';
		}

		return describeVideoStatus(currentPlayback.status);
	}

	function availabilityMessage(currentPlayback) {
		if (!currentPlayback) {
			return 'This video is still being prepared. Check back again in a little while.';
		}

		if (currentPlayback.status === 'FAILED') {
			return currentPlayback.errorMessage || 'This video is unavailable right now.';
		}

		if (currentPlayback.status === 'BASELINE_READY' || currentPlayback.status === 'PROCESSING_FULL') {
			return 'You can watch now while higher quality versions continue to finish in the background.';
		}

		return 'This video is still being prepared. Check back again in a little while.';
	}

	function processingSummary(currentPlayback) {
		if (!currentPlayback) {
			return '';
		}

		if (currentPlayback.status === 'FAILED' && currentPlayback.isStreamable) {
			return 'Ready to watch';
		}

		if (currentPlayback.status === 'FAILED') {
			return 'Unavailable right now';
		}

		return statusCopy(currentPlayback);
	}

	function processingDetails(currentPlayback) {
		if (!currentPlayback) {
			return '';
		}

		if (currentPlayback.status === 'PROCESSING_FULL' || currentPlayback.status === 'BASELINE_READY') {
			return 'More quality options are still being prepared.';
		}

		if (currentPlayback.status === 'READY') {
			return 'All available playback qualities are ready.';
		}

		if (currentPlayback.status === 'FAILED' && currentPlayback.isStreamable) {
			return 'Playback is available, but some later processing steps did not finish cleanly.';
		}

		if (currentPlayback.status === 'FAILED') {
			return currentPlayback.errorMessage || 'This video is unavailable right now.';
		}

		return 'Your video is being prepared for playback.';
	}

	function buildTimelineItems(currentPlayback) {
		if (!currentPlayback) {
			return [];
		}

		const items = [
			{
				label: 'Uploaded',
				value: formatDateTime(currentPlayback.createdAt)
			}
		];

		if (currentPlayback.baselineReadyAt) {
			items.push({
				label: 'Ready to watch',
				value: formatDateTime(currentPlayback.baselineReadyAt)
			});
		}

		if (
			currentPlayback.processingCompletedAt &&
			currentPlayback.processingCompletedAt !== currentPlayback.baselineReadyAt
		) {
			items.push({
				label: 'All qualities finished',
				value: formatDateTime(currentPlayback.processingCompletedAt)
			});
		}

		return items;
	}

	async function handleVideoDeleted() {
		clearRefreshTimer();
		await goto('/');
	}
</script>

<div class="page-shell">
	<section class="panel playback-shell">
		{#if pageState === 'loading'}
			<div class="empty-state">Loading video.</div>
		{:else if pageState === 'error'}
			<div class="empty-state">{errorMessage}</div>
		{:else if playback}
			<header class="playback-header">
				<a class="back-link" href="/">Back to homepage</a>
				<h1 class="playback-title">{playback.title || playback.originalFilename}</h1>
			</header>

			{#if isVideoStreamable(playback.status, playback.isStreamable) && playback.manifestUrl}
				<div class="player-card">
					<VideoPlayer
						manifestUrl={playback.manifestUrl}
						qualityOptions={playback.availableQualities}
						defaultQuality={playback.defaultQuality}
						bind:selectedQuality
					/>
				</div>
			{:else}
				<div class="player-placeholder">
					<p class="placeholder-copy">{availabilityMessage(playback)}</p>
				</div>
			{/if}

			<div class="details-grid">
				<div class="details-card">
					<p class="detail-label">Status</p>
					<p class="detail-value">{processingSummary(playback)}</p>
					<p class="detail-copy">{processingDetails(playback)}</p>
				</div>

				{#each buildTimelineItems(playback) as item}
					<div class="details-card">
						<p class="detail-label">{item.label}</p>
						<p class="detail-value">{item.value}</p>
					</div>
				{/each}
			</div>

			{#if playback.errorMessage && playback.status === 'FAILED'}
				<div class="failure-card">
					<p class="detail-label">Playback note</p>
					<p class="detail-value">{playback.errorMessage}</p>
				</div>
			{/if}

			<div class="bottom-grid">
				<ShareLinkBox sharePath={playback.playbackPath} />
				<DeleteVideoPanel publicId={playback.publicId} on:deleted={handleVideoDeleted} />
			</div>

			{#if playback.status !== 'READY' && playback.status !== 'FAILED'}
				<p class="refresh-note">
					This page updates automatically every
					{Math.round((playback.pollIntervalMs ?? 5000) / 1000)} seconds while playback is being prepared.
				</p>
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
		color: var(--accent);
		font-weight: 700;
	}

	.playback-header {
		display: grid;
		gap: 10px;
		margin-bottom: 18px;
	}

	.playback-title {
		margin: 0;
		font-family: "Iowan Old Style", "Palatino Linotype", "Book Antiqua", Georgia, serif;
		font-size: clamp(1.85rem, 4vw, 2.6rem);
		line-height: 1.05;
	}

	.details-grid,
	.bottom-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
		gap: 12px;
		margin-top: 16px;
	}

	.details-card,
	.player-placeholder,
	.player-card,
	.failure-card {
		padding: 16px 18px;
		border-radius: 18px;
		background: rgba(255, 255, 255, 0.84);
		border: 1px solid rgba(0, 0, 0, 0.1);
	}

	.player-card,
	.player-placeholder,
	.failure-card {
		margin-top: 12px;
	}

	.failure-card {
		background: var(--danger-soft);
		border-color: rgba(151, 30, 30, 0.14);
	}

	.placeholder-copy,
	.detail-copy,
	.refresh-note {
		margin: 0;
		color: var(--text-muted);
	}

	.detail-label {
		margin: 0 0 6px;
		font-size: 0.74rem;
		font-weight: 700;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--text-muted);
	}

	.detail-value {
		margin: 0;
		word-break: break-word;
		font-size: 0.98rem;
		font-weight: 600;
		line-height: 1.35;
	}

	.detail-copy {
		margin-top: 6px;
		font-size: 0.9rem;
	}

	.refresh-note {
		margin-top: 18px;
		font-size: 0.9rem;
	}
</style>
