<script>
	import { describeVideoStatus, statusTone } from '$lib/status';

	export let status = 'INITIATED';

	$: tone = statusTone(status);
	$: label = describeVideoStatus(status);
</script>

<span class={`badge badge-${tone}`}>{label}</span>

<style>
	.badge {
		display: inline-flex;
		align-items: center;
		padding: 6px 12px;
		border-radius: 999px;
		font-size: 0.78rem;
		font-weight: 700;
		letter-spacing: 0.04em;
		text-transform: uppercase;
	}

	.badge-neutral {
		background: rgba(17, 18, 20, 0.08);
		color: #4c5158;
	}

	.badge-warning {
		background: var(--warning-soft);
		color: #353941;
	}

	.badge-success {
		background: var(--success-soft);
		color: var(--success);
	}

	.badge-danger {
		background: var(--danger-soft);
		color: var(--danger);
	}
</style>
