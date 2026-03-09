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
					<div class="video-card-content">
						<div class="video-card-top">
							<span class="meta-text">Uploaded {formatDate(video.createdAt)}</span>
						</div>
						<h3>{video.title || video.originalFilename}</h3>
						<p>{describeVideoStatus(video.status)}</p>
						<div class="video-card-footer">
							<a class="inline-link" href={video.playbackPath}>Open video</a>
						</div>
					</div>
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
		gap: 12px;
		padding: 20px;
		border-radius: 24px;
		background: rgba(255, 255, 255, 0.84);
		border: 1px solid rgba(0, 0, 0, 0.1);
		transition: transform 140ms ease, border-color 140ms ease, box-shadow 140ms ease;
	}

	.video-card:hover {
		transform: translateY(-2px);
		border-color: rgba(17, 18, 20, 0.28);
		box-shadow: 0 16px 32px rgba(0, 0, 0, 0.12);
	}

	.video-card-content {
		display: grid;
		gap: 12px;
	}

	.video-card-top,
	.video-card-footer {
		display: flex;
		justify-content: space-between;
		gap: 12px;
		flex-wrap: wrap;
	}

	h3 {
		margin: 0;
		font-size: 1.05rem;
	}

	p {
		margin: 0;
		color: var(--text-muted);
	}

	.video-card-footer {
		font-size: 0.8rem;
		color: var(--text-muted);
		word-break: break-all;
	}

	.inline-link {
		color: var(--accent);
		font-weight: 700;
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
