<script>
	import { onMount } from 'svelte';
	import { page } from '$app/stores';
	import { getVideoDetails } from '$lib/api';
	import { formatDateTime } from '$lib/formatters';
	import ShareLinkBox from '$lib/components/ShareLinkBox.svelte';
	import StatusBadge from '$lib/components/StatusBadge.svelte';
	import { describeVideoStatus, isVideoStreamable } from '$lib/status';

	let video = null;
	let pageState = 'loading';
	let errorMessage = '';

	onMount(() => {
		void loadVideoDetails();
	});

	async function loadVideoDetails() {
		pageState = 'loading';
		errorMessage = '';

		try {
			video = await getVideoDetails($page.params.publicId);
			pageState = 'ready';
		} catch (error) {
			pageState = 'error';
			errorMessage = error instanceof Error ? error.message : 'Failed to load the video.';
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
		{:else if video}
			<div class="playback-header">
				<div>
					<p class="eyebrow">Video</p>
					<h1 class="section-title">{video.title || video.originalFilename}</h1>
					<p class="section-copy">{describeVideoStatus(video.status)}</p>
				</div>
				<StatusBadge status={video.status} />
			</div>

			<div class="details-grid">
				<div class="details-card">
					<p class="detail-label">Uploaded</p>
					<p class="detail-value">{formatDateTime(video.createdAt)}</p>
				</div>
			</div>

			<ShareLinkBox sharePath={video.playbackPath} />

			{#if isVideoStreamable(video.status, video.isStreamable) && video.manifestUrl}
				<div class="player-placeholder">
					<p class="detail-label">Watch</p>
					<a href={video.manifestUrl}>Open stream</a>
				</div>
			{:else}
				<div class="player-placeholder">
					<p class="detail-label">Availability</p>
					<p class="detail-value">
						This video is still being prepared. Check back again in a little while.
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
	.player-placeholder {
		padding: 20px;
		border-radius: 22px;
		background: rgba(255, 255, 255, 0.84);
		border: 1px solid rgba(0, 0, 0, 0.1);
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
