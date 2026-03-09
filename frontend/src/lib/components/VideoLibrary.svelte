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
			<div class="video-list">
				{#each videos as video}
					<article class="video-row">
						<a class="video-row-link" href={video.playbackPath}>
							<div class="video-row-action">
								<span class="card-open-button">Open</span>
							</div>
							<div class="video-row-copy">
								<div class="video-row-top">
									<h3>{video.title || video.originalFilename}</h3>
									<span class="video-status">{describeVideoStatus(video.status)}</span>
								</div>
								<p class="video-row-meta">Uploaded {formatDate(video.createdAt)}</p>
							</div>
						</a>
						<DeleteVideoPanel
						publicId={video.publicId}
						variant="list"
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
		padding: 22px;
	}

	.library-header {
		display: grid;
		gap: 8px;
		margin-bottom: 18px;
	}

	.library-title {
		margin: 8px 0 0;
		font-family: "Iowan Old Style", "Palatino Linotype", "Book Antiqua", Georgia, serif;
		font-size: clamp(1.45rem, 3vw, 2.1rem);
		line-height: 1;
	}

	.video-list {
		display: grid;
		gap: 10px;
		margin-bottom: 14px;
	}

	.video-row {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		gap: 0;
		align-items: center;
		border-radius: 18px;
		background: rgba(255, 255, 255, 0.84);
		border: 1px solid rgba(0, 0, 0, 0.1);
		overflow: hidden;
		transition: transform 140ms ease, border-color 140ms ease, box-shadow 140ms ease;
	}

	.video-row:hover {
		transform: translateY(-1px);
		border-color: rgba(17, 18, 20, 0.28);
		box-shadow: 0 12px 26px rgba(0, 0, 0, 0.1);
	}

	.video-row-link {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr);
		align-items: center;
		gap: 14px;
		padding: 14px 16px;
		min-width: 0;
	}

	.video-row-link:focus-visible {
		outline: 2px solid rgba(17, 18, 20, 0.28);
		outline-offset: -2px;
	}

	.video-row-copy {
		display: grid;
		gap: 4px;
		min-width: 0;
	}

	.video-row-top {
		display: flex;
		align-items: center;
		gap: 10px;
		flex-wrap: wrap;
	}

	h3 {
		margin: 0;
		font-size: 0.98rem;
		line-height: 1.25;
	}

	.video-row-meta {
		margin: 0;
		color: var(--text-muted);
	}

	.video-row-meta {
		font-size: 0.82rem;
	}

	.video-status {
		display: inline-flex;
		align-items: center;
		padding: 4px 8px;
		border-radius: 999px;
		background: rgba(17, 18, 20, 0.06);
		color: var(--text-muted);
		font-size: 0.7rem;
		font-weight: 700;
		letter-spacing: 0.03em;
		text-transform: uppercase;
	}

	.card-open-button {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 8px 12px;
		min-width: 64px;
		border-radius: 999px;
		background: #111214;
		color: #f7f8fa;
		font-size: 0.88rem;
		font-weight: 700;
		box-shadow: 0 8px 14px rgba(0, 0, 0, 0.14);
		transition: transform 140ms ease, box-shadow 140ms ease;
	}

	.video-row:hover .card-open-button,
	.video-row-link:focus-visible .card-open-button {
		transform: translateY(-1px);
		box-shadow: 0 10px 18px rgba(0, 0, 0, 0.16);
	}

	.pagination-row {
		display: flex;
		justify-content: space-between;
		gap: 12px;
		align-items: center;
		flex-wrap: wrap;
	}

	.pagination-actions {
		display: flex;
		gap: 8px;
		flex-wrap: wrap;
	}

	@media (max-width: 860px) {
		.video-row {
			grid-template-columns: 1fr;
		}

		.video-row-link {
			grid-template-columns: auto 1fr;
			gap: 12px;
		}
	}
</style>
