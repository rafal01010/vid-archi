<script>
	import { createEventDispatcher } from 'svelte';
	import DeleteVideoPanel from '$lib/components/DeleteVideoPanel.svelte';
	import { formatDate } from '$lib/formatters';
	import { describeVideoStatus } from '$lib/status';

	const dispatch = createEventDispatcher();

	export let videos = [];
	export let isLoading = false;
	export let errorMessage = '';
	export let pagination = {
		page: 1,
		pageSize: 10,
		totalCount: 0,
		totalPages: 1,
		hasPreviousPage: false,
		hasNextPage: false
	};

	function handlePreviousPageClicked() {
		dispatch('previouspagerequested');
	}

	function handleNextPageClicked() {
		dispatch('nextpagerequested');
	}

	function handleVideoDeleted() {
		dispatch('videodeleted');
	}
</script>

<section class="panel library-panel">
	<div class="library-header">
		<div>
			<p class="eyebrow">Library</p>
			<h2 class="library-title">Recent videos</h2>
		</div>
		<p class="section-copy">Browse the latest uploads and open any video page.</p>
	</div>

	{#if isLoading}
		<div class="empty-state">Loading the most recent uploads.</div>
	{:else if errorMessage}
		<div class="empty-state">{errorMessage}</div>
	{:else if videos.length === 0}
		<div class="empty-state">
			No videos yet. Upload one to get started.
		</div>
	{:else}
		<div class="video-grid">
			{#each videos as video}
				<article class="video-card">
					<a class="video-card-link" href={video.playbackPath}>
						<div class="video-card-top">
							<span class="meta-text">Uploaded {formatDate(video.createdAt)}</span>
							<span class="video-status">{describeVideoStatus(video.status)}</span>
						</div>
						<div class="video-card-content">
							<h3>{video.title || video.originalFilename}</h3>
							<p class="video-card-copy">Open the video page to watch or check processing progress.</p>
						</div>
						<div class="video-card-footer">
							<span class="meta-text">Watch, copy the link, or check processing</span>
							<span class="card-open-button">Open video</span>
						</div>
					</a>
					<DeleteVideoPanel
						publicId={video.publicId}
						variant="card"
						on:deleted={handleVideoDeleted}
					/>
				</article>
			{/each}
		</div>

		<div class="pagination-row">
			<div class="meta-text">
				Page {pagination.page} of {pagination.totalPages} · {pagination.totalCount} total videos
			</div>
			<div class="pagination-actions">
				<button
					class="button-secondary"
					type="button"
					on:click={handlePreviousPageClicked}
					disabled={!pagination.hasPreviousPage}
				>
					Previous
				</button>
				<button
					class="button-secondary"
					type="button"
					on:click={handleNextPageClicked}
					disabled={!pagination.hasNextPage}
				>
					Next
				</button>
			</div>
		</div>
	{/if}
</section>

<style>
	.library-panel {
		padding: 28px;
	}

	.library-header {
		display: grid;
		gap: 10px;
		margin-bottom: 22px;
	}

	.library-title {
		margin: 10px 0 0;
		font-family: "Iowan Old Style", "Palatino Linotype", "Book Antiqua", Georgia, serif;
		font-size: clamp(1.8rem, 4vw, 2.6rem);
		line-height: 1;
	}

	.video-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(230px, 1fr));
		gap: 16px;
		margin-bottom: 18px;
	}

	.video-card {
		display: grid;
		gap: 0;
		border-radius: 24px;
		background: rgba(255, 255, 255, 0.84);
		border: 1px solid rgba(0, 0, 0, 0.1);
		overflow: hidden;
		transition: transform 140ms ease, border-color 140ms ease, box-shadow 140ms ease;
	}

	.video-card:hover {
		transform: translateY(-2px);
		border-color: rgba(17, 18, 20, 0.28);
		box-shadow: 0 16px 32px rgba(0, 0, 0, 0.12);
	}

	.video-card-link {
		display: grid;
		gap: 14px;
		padding: 20px;
		min-height: 178px;
	}

	.video-card-link:focus-visible {
		outline: 2px solid rgba(17, 18, 20, 0.28);
		outline-offset: -2px;
	}

	.video-card-top,
	.video-card-footer {
		display: flex;
		justify-content: space-between;
		gap: 12px;
		flex-wrap: wrap;
		align-items: center;
	}

	.video-card-content {
		display: grid;
		gap: 8px;
	}

	h3 {
		margin: 0;
		font-size: 1.05rem;
	}

	.video-card-copy {
		margin: 0;
		color: var(--text-muted);
		font-size: 0.94rem;
	}

	.video-card-footer {
		margin-top: auto;
	}

	.video-status {
		display: inline-flex;
		align-items: center;
		padding: 5px 10px;
		border-radius: 999px;
		background: rgba(17, 18, 20, 0.06);
		color: var(--text-muted);
		font-size: 0.75rem;
		font-weight: 700;
		letter-spacing: 0.03em;
		text-transform: uppercase;
	}

	.card-open-button {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 10px 16px;
		border-radius: 999px;
		background: linear-gradient(135deg, #111214 0%, #3a3d42 100%);
		color: #f7f8fa;
		font-size: 0.95rem;
		font-weight: 700;
		box-shadow: 0 12px 24px rgba(0, 0, 0, 0.18);
		transition: transform 140ms ease, box-shadow 140ms ease;
	}

	.video-card:hover .card-open-button,
	.video-card-link:focus-visible .card-open-button {
		transform: translateY(-1px);
		box-shadow: 0 16px 28px rgba(0, 0, 0, 0.22);
	}

	.pagination-row {
		display: flex;
		justify-content: space-between;
		gap: 16px;
		align-items: center;
		flex-wrap: wrap;
	}

	.pagination-actions {
		display: flex;
		gap: 10px;
		flex-wrap: wrap;
	}
</style>
