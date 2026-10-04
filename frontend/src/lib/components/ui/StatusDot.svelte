<script lang="ts">
	import type { DotStatus } from '$lib/utils/status';

	interface Props {
		status: DotStatus;
	}

	let { status }: Props = $props();
</script>

<span class="ui-dot ui-dot--{status}" aria-hidden="true"></span>

<style>
	.ui-dot {
		display: inline-block;
		width: 7px;
		height: 7px;
		border-radius: 50%;
		flex-shrink: 0;
	}
	.ui-dot--running   { background: var(--accent-green); }
	.ui-dot--pending   { background: var(--accent-yellow); animation: ui-dot-pulse 1.6s ease-in-out infinite; }
	.ui-dot--deploying { background: var(--accent); animation: ui-dot-pulse 1.2s ease-in-out infinite; }
	.ui-dot--warning   { background: var(--accent-yellow); }
	.ui-dot--failed   { background: var(--accent-red); }
	.ui-dot--stopped   { background: var(--text-dim); }

	@keyframes ui-dot-pulse {
		0%, 100% { opacity: 1; }
		50%      { opacity: 0.35; }
	}
</style>
