<script>
	import { browser } from '$app/environment';

	export let sharePath = '';

	let copyState = 'idle';

	$: shareUrl =
		browser && sharePath ? new URL(sharePath, window.location.origin).toString() : sharePath;
	$: copyButtonLabel =
		copyState === 'copied' ? 'Copied' : copyState === 'error' ? 'Copy failed' : 'Copy link';

	async function handleCopyLink() {
		if (!browser || !shareUrl) {
			return;
		}

		try {
			await copyTextToClipboard(shareUrl);
			copyState = 'copied';
		} catch {
			copyState = 'error';
		}

		window.setTimeout(() => {
			copyState = 'idle';
		}, 1800);
	}

	async function copyTextToClipboard(text) {
		if (navigator.clipboard?.writeText && window.isSecureContext) {
			await navigator.clipboard.writeText(text);
			return;
		}

		const fallbackInput = document.createElement('textarea');
		fallbackInput.value = text;
		fallbackInput.setAttribute('readonly', 'true');
		fallbackInput.style.position = 'absolute';
		fallbackInput.style.left = '-9999px';
		document.body.appendChild(fallbackInput);
		fallbackInput.select();

		const copySucceeded = document.execCommand('copy');
		document.body.removeChild(fallbackInput);

		if (!copySucceeded) {
			throw new Error('Clipboard copy failed.');
		}
	}
</script>

{#if sharePath}
	<section class="share-box">
		<div>
			<p class="share-label">Share link</p>
			<a href={sharePath} class="share-url">{shareUrl}</a>
		</div>

		<div class="share-actions">
			<button class="button-secondary" type="button" on:click={handleCopyLink}>
				{copyButtonLabel}
			</button>
			<a class="button-primary" href={sharePath}>Open page</a>
		</div>
	</section>
{/if}

<style>
	.share-box {
		display: grid;
		gap: 14px;
		padding: 18px;
		border-radius: 22px;
		background: rgba(255, 255, 255, 0.86);
		border: 1px solid rgba(0, 0, 0, 0.12);
	}

	.share-label {
		margin: 0 0 6px;
		font-size: 0.82rem;
		font-weight: 700;
		letter-spacing: 0.05em;
		text-transform: uppercase;
		color: var(--text-muted);
	}

	.share-url {
		display: inline-block;
		word-break: break-all;
		color: var(--accent);
		font-weight: 700;
	}

	.share-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 10px;
	}
</style>
