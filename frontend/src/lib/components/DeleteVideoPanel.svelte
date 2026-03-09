<script>
	import { createEventDispatcher } from 'svelte';
	import { deleteVideo } from '$lib/api';

	const dispatch = createEventDispatcher();

	export let publicId = '';
	export let variant = 'page';

	let isFormVisible = false;
	let deleteCode = '';
	let deletePhase = 'idle';
	let errorMessage = '';

	$: isDeleting = deletePhase === 'deleting';
	$: panelClassName = variant === 'card' ? 'delete-panel card-variant' : 'delete-panel page-variant';
	$: deleteButtonLabel = isDeleting ? 'Deleting...' : 'Delete video';

	function handleDeleteToggle() {
		isFormVisible = !isFormVisible;
		errorMessage = '';
	}

	async function handleDeleteSubmitted() {
		const normalizedDeleteCode = deleteCode.trim();

		if (normalizedDeleteCode.length < 6) {
			errorMessage = 'Enter the delete code you set during upload.';
			return;
		}

		errorMessage = '';
		deletePhase = 'deleting';

		try {
			const response = await deleteVideo(publicId, { deleteCode: normalizedDeleteCode });
			deletePhase = 'deleted';
			dispatch('deleted', response);
		} catch (error) {
			deletePhase = 'idle';
			errorMessage =
				error instanceof Error ? error.message : 'The video could not be deleted right now.';
		}
	}
</script>

<section class={panelClassName}>
	<div class="delete-copy">
		<p class="detail-label">Delete video</p>
		<p class="detail-value">
			Deleting removes the upload source, processed HLS files, and the public page.
		</p>
	</div>

	{#if deletePhase === 'deleted'}
		<div class="message message-success">Video deleted.</div>
	{:else}
		<div class="delete-actions">
			<button class="button-secondary delete-toggle" type="button" on:click={handleDeleteToggle}>
				{isFormVisible ? 'Cancel' : 'Delete with code'}
			</button>
		</div>

		{#if isFormVisible}
			<div class="delete-form">
				<label class="field-label" for={`delete-code-${publicId}`}>Delete code</label>
				<input
					id={`delete-code-${publicId}`}
					class="input"
					type="password"
					placeholder="Enter the code from upload"
					bind:value={deleteCode}
					disabled={isDeleting}
				/>
				<p class="delete-note">This action cannot be undone.</p>
				<div class="delete-actions">
					<button class="button-danger" type="button" on:click={handleDeleteSubmitted} disabled={isDeleting}>
						{deleteButtonLabel}
					</button>
				</div>
			</div>
		{/if}
	{/if}

	{#if errorMessage}
		<div class="message message-error">{errorMessage}</div>
	{/if}
</section>

<style>
	.delete-panel {
		display: grid;
		gap: 12px;
		padding: 18px;
		border-radius: 22px;
		background: rgba(255, 255, 255, 0.84);
		border: 1px solid rgba(0, 0, 0, 0.1);
	}

	.card-variant {
		padding: 14px;
		border-radius: 18px;
	}

	.delete-copy {
		display: grid;
		gap: 6px;
	}

	.delete-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 10px;
	}

	.delete-form {
		display: grid;
		gap: 10px;
	}

	.delete-note {
		margin: 0;
		font-size: 0.9rem;
		color: var(--text-muted);
	}

	.button-danger {
		appearance: none;
		border: 0;
		border-radius: 999px;
		padding: 12px 18px;
		font-size: 0.95rem;
		font-weight: 700;
		color: white;
		background: #8f1d1d;
		cursor: pointer;
	}

	.button-danger:disabled {
		cursor: progress;
		opacity: 0.7;
	}

	.message-success {
		padding: 14px 16px;
		border-radius: 16px;
		background: rgba(16, 93, 52, 0.14);
		color: #105d34;
	}

	.message-error {
		padding: 14px 16px;
		border-radius: 16px;
		background: var(--danger-soft);
		color: var(--danger);
	}
</style>
