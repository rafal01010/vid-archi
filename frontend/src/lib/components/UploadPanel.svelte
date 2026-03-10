<script>
	import { createEventDispatcher } from 'svelte';
	import { createVideoUpload, deleteVideo } from '$lib/api';
	import { formatBytes } from '$lib/formatters';
	import {
		ALLOWED_VIDEO_EXTENSIONS,
		ALLOWED_VIDEO_MIME_TYPES,
		MAX_UPLOAD_SIZE_BYTES
	} from '$lib/policy';
	import ShareLinkBox from '$lib/components/ShareLinkBox.svelte';
	import { isUploadCancelledError, uploadMultipartVideo } from '$lib/uploads/multipartUpload';

	const dispatch = createEventDispatcher();

	let selectedFile = null;
	let title = '';
	let deleteCode = '';
	let deleteCodeConfirmation = '';
	let uploadPhase = 'idle';
	let uploadBytesTransferred = 0;
	let uploadErrorMessage = '';
	let sharePath = '';
	let uploadController = null;
	let cancelRequestedDuringPreparation = false;

	$: isUploading = ['preparing', 'uploading', 'completing'].includes(uploadPhase);
	$: canCancelUpload = ['preparing', 'uploading'].includes(uploadPhase);
	$: progressPercent = selectedFile
		? Math.min(100, Math.round((uploadBytesTransferred / selectedFile.size) * 100))
		: 0;
	$: selectedFilename = selectedFile?.name ?? 'No file selected yet';
	$: uploadButtonLabel = uploadPhase === 'preparing'
		? 'Preparing...'
		: uploadPhase === 'uploading'
			? 'Uploading...'
		: uploadPhase === 'completing'
			? 'Finalizing...'
			: uploadPhase === 'cancelled'
				? 'Start upload again'
			: uploadPhase === 'failed'
				? 'Retry upload'
		: 'Upload video';
	$: uploadStatusLabel = uploadPhase === 'preparing'
		? 'Preparing upload'
		: uploadPhase === 'uploading'
			? 'Uploading'
		: uploadPhase === 'completing'
			? 'Finishing upload'
		: uploadPhase === 'succeeded'
			? 'Upload complete'
		: uploadPhase === 'cancelled'
			? 'Upload cancelled'
		: uploadPhase === 'failed'
			? 'Upload failed'
		: 'Not started';
	$: canSubmit =
		Boolean(selectedFile) &&
		Boolean(deleteCode.trim()) &&
		Boolean(deleteCodeConfirmation.trim()) &&
		!isUploading;

	function handleFileSelected(event) {
		const [file] = event.currentTarget.files ?? [];

		selectedFile = file ?? null;
		uploadPhase = 'idle';
		uploadBytesTransferred = 0;
		uploadErrorMessage = '';
		sharePath = '';
		cancelRequestedDuringPreparation = false;
	}

	async function handleUploadStarted() {
		if (!selectedFile) {
			return;
		}

		const validationMessage = validateSelectedFile(selectedFile);
		const deleteCodeValidationMessage = validateDeleteCode();

		if (validationMessage) {
			uploadErrorMessage = validationMessage;
			return;
		}

		if (deleteCodeValidationMessage) {
			uploadErrorMessage = deleteCodeValidationMessage;
			return;
		}

		uploadErrorMessage = '';
		sharePath = '';
		uploadPhase = 'preparing';
		cancelRequestedDuringPreparation = false;
		const normalizedDeleteCode = deleteCode.trim();
		let createdVideoPublicId = '';

		try {
			const createResponse = await createVideoUpload({
				filename: selectedFile.name,
				contentType: selectedFile.type || inferContentType(selectedFile.name),
				sizeBytes: selectedFile.size,
				title: title.trim() || null,
				deleteCode: normalizedDeleteCode
			});
			createdVideoPublicId = createResponse.publicId;

			if (cancelRequestedDuringPreparation) {
				await cleanupCancelledUpload(createdVideoPublicId, normalizedDeleteCode);
				uploadPhase = 'cancelled';
				uploadErrorMessage = 'Upload cancelled.';
				dispatch('uploadcancelled');
				return;
			}

			uploadPhase = 'uploading';
			uploadController = new AbortController();
			const completeResponse = await uploadMultipartVideo({
				file: selectedFile,
				createResponse,
				signal: uploadController.signal,
				onProgress: (loadedBytes) => {
					uploadBytesTransferred = loadedBytes;
				},
				onCompleting: () => {
					uploadPhase = 'completing';
				}
			});

			sharePath = completeResponse.playbackPath;
			uploadBytesTransferred = selectedFile.size;
			uploadPhase = 'succeeded';

			dispatch('uploadcompleted', {
				publicId: completeResponse.publicId,
				playbackPath: completeResponse.playbackPath
			});
		} catch (error) {
			if (isUploadCancelledError(error)) {
				if (createdVideoPublicId) {
					await cleanupCancelledUpload(createdVideoPublicId, normalizedDeleteCode);
				}

				uploadPhase = 'cancelled';
				uploadErrorMessage = 'Upload cancelled.';
				dispatch('uploadcancelled');
				return;
			}

			uploadPhase = 'failed';
			uploadErrorMessage = error instanceof Error ? error.message : 'Upload failed';
		} finally {
			uploadController = null;
			cancelRequestedDuringPreparation = false;
		}
	}

	function handleUploadCancelled() {
		if (uploadPhase === 'preparing') {
			cancelRequestedDuringPreparation = true;
			uploadErrorMessage = 'Cancelling upload...';
			return;
		}

		uploadController?.abort();
	}

	async function cleanupCancelledUpload(publicId, normalizedDeleteCode) {
		try {
			await deleteVideo(publicId, { deleteCode: normalizedDeleteCode });
		} catch (error) {
			console.warn('Failed to delete cancelled upload', error);
		}
	}

	function validateSelectedFile(file) {
		if (file.size <= 0) {
			return 'Select a non-empty video file.';
		}

		if (file.size > MAX_UPLOAD_SIZE_BYTES) {
			return 'The file exceeds the 1 GB size limit.';
		}

		const normalizedType = (file.type || '').toLowerCase();
		const normalizedName = file.name.toLowerCase();
		const hasAllowedMimeType = normalizedType && ALLOWED_VIDEO_MIME_TYPES.has(normalizedType);
		const hasAllowedExtension = Array.from(ALLOWED_VIDEO_EXTENSIONS).some((extension) =>
			normalizedName.endsWith(extension)
		);

		if (!hasAllowedMimeType && !hasAllowedExtension) {
			return 'Allowed formats are MP4, MOV, WebM, and AVI.';
		}

		return '';
	}

	function inferContentType(filename) {
		const normalizedName = filename.toLowerCase();

		if (normalizedName.endsWith('.mov')) {
			return 'video/quicktime';
		}

		if (normalizedName.endsWith('.webm')) {
			return 'video/webm';
		}

		if (normalizedName.endsWith('.avi')) {
			return 'video/x-msvideo';
		}

		return 'video/mp4';
	}

	function validateDeleteCode() {
		const normalizedDeleteCode = deleteCode.trim();
		const normalizedDeleteCodeConfirmation = deleteCodeConfirmation.trim();

		if (normalizedDeleteCode.length < 6) {
			return 'Set a delete code with at least 6 characters.';
		}

		if (normalizedDeleteCode !== normalizedDeleteCodeConfirmation) {
			return 'Delete code confirmation does not match.';
		}

		return '';
	}
</script>

<section class="panel upload-panel">
	<div class="upload-copy">
		<p class="eyebrow">Upload</p>
		<h1 class="section-title">Share a video</h1>
		<p class="section-copy">Choose a file and create a page you can share with anyone.</p>
	</div>

	<div class="upload-grid">
		<div class="form-card">
			<label class="field-label" for="video-title">Title</label>
			<input
				id="video-title"
				class="input"
				type="text"
				placeholder="Optional title"
				bind:value={title}
				disabled={isUploading}
			/>

			<label class="field-label field-spacing" for="video-file">Video file</label>
			<input
				id="video-file"
				class="input"
				type="file"
				accept=".mp4,.mov,.webm,.avi,video/mp4,video/quicktime,video/webm,video/x-msvideo,video/vnd.avi"
				on:change={handleFileSelected}
				disabled={isUploading}
			/>

			<div class="file-meta">
				<div>
					<p class="meta-heading">Selected file</p>
					<p class="meta-value">{selectedFilename}</p>
				</div>
				<div>
					<p class="meta-heading">File size limit</p>
					<p class="meta-value">1 GB</p>
				</div>
				<div>
					<p class="meta-heading">Accepted files</p>
					<p class="meta-value">MP4, MOV, WebM, AVI</p>
				</div>
			</div>

			<label class="field-label field-spacing" for="delete-code">Delete code</label>
			<input
				id="delete-code"
				class="input"
				type="password"
				placeholder="Required to delete the video later"
				bind:value={deleteCode}
				disabled={isUploading}
			/>

			<label class="field-label field-spacing" for="delete-code-confirmation">Confirm delete code</label>
			<input
				id="delete-code-confirmation"
				class="input"
				type="password"
				placeholder="Re-enter the delete code"
				bind:value={deleteCodeConfirmation}
				disabled={isUploading}
			/>

			<p class="delete-code-copy">
				This code is stored as a hash and acts as the anonymous ownership check for deletion.
			</p>

			<div class="actions">
				<button class="button-primary" type="button" on:click={handleUploadStarted} disabled={!canSubmit}>
					{uploadButtonLabel}
				</button>
				{#if canCancelUpload}
					<button class="button-secondary" type="button" on:click={handleUploadCancelled}>
						Cancel upload
					</button>
				{/if}
				{#if sharePath}
					<a class="button-secondary" href={sharePath}>Open share page</a>
				{/if}
			</div>
		</div>

		<div class="status-card">
			<div class="status-block">
				<p class="status-label">Upload progress</p>
				<div class="progress-track" aria-hidden="true">
					<div class="progress-fill" style={`width: ${progressPercent}%`}></div>
				</div>
				<div class="progress-meta">
					<span>{progressPercent}%</span>
					<span>{formatBytes(uploadBytesTransferred)} / {formatBytes(selectedFile?.size ?? 0)}</span>
				</div>
			</div>

			<div class="status-block">
				<p class="status-label">Status</p>
				<p class="status-value">{uploadStatusLabel}</p>
			</div>

			{#if uploadErrorMessage}
				<div class="message message-error">{uploadErrorMessage}</div>
			{/if}

			{#if sharePath}
				<ShareLinkBox {sharePath} />
			{:else}
				<div class="message message-muted">
					Your share link will appear here when the upload is complete.
				</div>
			{/if}
		</div>
	</div>
</section>

<style>
	.upload-panel {
		padding: clamp(18px, 3.2vw, 26px);
	}

	.upload-copy {
		margin-bottom: 20px;
	}

	.upload-grid {
		display: grid;
		grid-template-columns: minmax(0, 1.1fr) minmax(300px, 0.9fr);
		gap: 14px;
	}

	.form-card,
	.status-card {
		padding: 18px;
		border-radius: 18px;
		background: rgba(255, 255, 255, 0.84);
		border: 1px solid rgba(0, 0, 0, 0.1);
	}

	.field-spacing {
		margin-top: 14px;
	}

	.file-meta {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 10px;
		margin: 14px 0 18px;
	}

	.meta-heading,
	.status-label {
		margin: 0 0 6px;
		font-size: 0.74rem;
		font-weight: 700;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--text-muted);
	}

	.meta-value,
	.status-value {
		margin: 0;
		font-weight: 700;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 10px;
	}

	.delete-code-copy {
		margin: 10px 0 18px;
		color: var(--text-muted);
		font-size: 0.88rem;
	}

	.status-card {
		display: grid;
		gap: 14px;
		align-content: start;
	}

	.status-block {
		display: grid;
		gap: 8px;
	}

	.progress-track {
		height: 10px;
		border-radius: 999px;
		background: rgba(17, 18, 20, 0.09);
		overflow: hidden;
	}

	.progress-fill {
		height: 100%;
		border-radius: inherit;
		background: linear-gradient(90deg, #111214 0%, #545961 100%);
		transition: width 140ms ease;
	}

	.progress-meta {
		display: flex;
		justify-content: space-between;
		gap: 10px;
		flex-wrap: wrap;
		color: var(--text-muted);
		font-size: 0.84rem;
	}

	.message {
		padding: 14px 16px;
		border-radius: 16px;
	}

	.message-error {
		background: var(--danger-soft);
		color: var(--danger);
	}

	.message-muted {
		background: rgba(255, 255, 255, 0.78);
		color: var(--text-muted);
		border: 1px solid rgba(0, 0, 0, 0.1);
	}

	@media (max-width: 920px) {
		.upload-grid {
			grid-template-columns: 1fr;
		}
	}

	@media (max-width: 720px) {
		.file-meta {
			grid-template-columns: 1fr;
		}
	}
</style>
