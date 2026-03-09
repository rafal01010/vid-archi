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
	$: panelClassName =
		variant === 'page'
			? 'delete-panel page-variant'
			: variant === 'list'
				? 'delete-panel list-variant'
				: 'delete-panel card-variant';
	$: deleteButtonLabel = isDeleting ? 'Deleting...' : 'Delete video';
	$: toggleButtonLabel = isFormVisible ? 'Cancel' : 'Delete with code';
	$: helperCopy =
		variant === 'card'
			? 'Need to remove this upload? Use the delete code from upload time.'
			: 'Delete this upload if you still have the delete code from upload time.';

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
	{#if deletePhase === 'deleted'}
		<div class="message message-success">Video deleted.</div>
	{:else if variant === 'list'}
		<div class="delete-actions">
			<button class="delete-toggle list-toggle" type="button" on:click={handleDeleteToggle}>
				{isFormVisible ? 'Cancel' : 'Delete'}
			</button>
		</div>

		{#if isFormVisible}
			<div class="delete-form">
				<label class="field-label" for={`delete-code-${publicId}`}>Delete code</label>
				<input
					id={`delete-code-${publicId}`}
					class="input"
					type="password"
					placeholder="Enter delete code"
					bind:value={deleteCode}
					disabled={isDeleting}
				/>
				<div class="delete-actions">
					<button class="button-danger" type="button" on:click={handleDeleteSubmitted} disabled={isDeleting}>
						{deleteButtonLabel}
					</button>
				</div>
			</div>
		{/if}
	{:else}
		<div class="delete-summary">
			<div class="delete-copy-block">
				<p class="delete-label">Delete video</p>
				<p class="delete-copy">{helperCopy}</p>
			</div>
			<div class="delete-actions">
				<button class="delete-toggle" type="button" on:click={handleDeleteToggle}>
					{toggleButtonLabel}
				</button>
			</div>
		</div>

		{#if isFormVisible}
			<div class="delete-form">
				<label class="field-label" for={`delete-code-${publicId}`}>Delete code</label>
				<input
					id={`delete-code-${publicId}`}
					class="input"
					type="password"
					placeholder="Enter delete code"
					bind:value={deleteCode}
					disabled={isDeleting}
				/>
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
		padding: 16px 18px;
		border-radius: 18px;
		background: rgba(17, 18, 20, 0.03);
		border: 1px solid rgba(0, 0, 0, 0.08);
	}

	.card-variant {
		padding: 14px 20px 18px;
		border-radius: 0;
		background: transparent;
		border: 0;
		border-top: 1px solid rgba(0, 0, 0, 0.08);
	}

	.list-variant {
		padding: 10px 12px 10px 0;
		border-radius: 0;
		background: transparent;
		border: 0;
		align-self: center;
	}

	.delete-summary {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 12px;
		flex-wrap: wrap;
	}

	.delete-copy-block {
		display: grid;
		gap: 4px;
	}

	.delete-label,
	.delete-copy {
		margin: 0;
	}

	.delete-label {
		font-size: 0.76rem;
		font-weight: 700;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--text-muted);
	}

	.delete-copy {
		max-width: 40ch;
		color: var(--text-muted);
		font-size: 0.9rem;
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

	.delete-toggle {
		appearance: none;
		border: 1px solid rgba(0, 0, 0, 0.12);
		border-radius: 999px;
		padding: 8px 13px;
		background: rgba(255, 255, 255, 0.78);
		color: var(--text-muted);
		font-size: 0.88rem;
		font-weight: 600;
		transition: background 140ms ease, color 140ms ease, border-color 140ms ease;
	}

	.list-toggle {
		padding: 7px 11px;
		font-size: 0.8rem;
	}

	.delete-toggle:hover {
		background: rgba(17, 18, 20, 0.08);
		color: var(--text-strong);
		border-color: rgba(0, 0, 0, 0.2);
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

	@media (max-width: 640px) {
		.card-variant,
		.page-variant,
		.list-variant {
			padding-inline: 14px;
		}
	}
</style>
