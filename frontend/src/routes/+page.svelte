<script>
	import { onMount } from 'svelte';
	import UploadPanel from '$lib/components/UploadPanel.svelte';
	import VideoLibrary from '$lib/components/VideoLibrary.svelte';
	import { listRecentVideos } from '$lib/api';

	let videos = [];
	let libraryState = 'loading';
	let libraryErrorMessage = '';
	let currentPage = 1;
	const pageSize = 10;
	let pagination = {
		page: 1,
		pageSize,
		totalCount: 0,
		totalPages: 1,
		hasPreviousPage: false,
		hasNextPage: false
	};

	onMount(() => {
		void loadRecentVideos();
	});

	async function loadRecentVideos(page = currentPage) {
		libraryState = 'loading';
		libraryErrorMessage = '';

		try {
			const response = await listRecentVideos({ page, pageSize });
			videos = response.videos;
			currentPage = response.page;
			pagination = {
				page: response.page,
				pageSize: response.pageSize,
				totalCount: response.totalCount,
				totalPages: response.totalPages,
				hasPreviousPage: response.hasPreviousPage,
				hasNextPage: response.hasNextPage
			};
			libraryState = 'ready';
		} catch (error) {
			libraryState = 'error';
			libraryErrorMessage =
				error instanceof Error ? error.message : "Couldn't load videos right now.";
		}
	}

	async function handleUploadCompleted() {
		await loadRecentVideos(1);
	}

	async function handlePreviousPageRequested() {
		if (!pagination.hasPreviousPage) {
			return;
		}

		await loadRecentVideos(currentPage - 1);
	}

	async function handleNextPageRequested() {
		if (!pagination.hasNextPage) {
			return;
		}

		await loadRecentVideos(currentPage + 1);
	}
</script>

<div class="page-shell home-page">
	<UploadPanel on:uploadcompleted={handleUploadCompleted} />

	<VideoLibrary
		{videos}
		isLoading={libraryState === 'loading'}
		errorMessage={libraryState === 'error' ? libraryErrorMessage : ''}
		{pagination}
		on:previouspagerequested={handlePreviousPageRequested}
		on:nextpagerequested={handleNextPageRequested}
	/>
</div>

<style>
	.home-page {
		display: grid;
		gap: 22px;
	}
</style>
