<script lang="ts">
	import type { DotStatus } from '$lib/utils/status';

	interface Props {
		status: DotStatus;
	}

	let { status }: Props = $props();
</script>

<span class="ui-dot ui-dot--{status}" aria-hidden="true"></span>

<style>
	/* Status never relies on colour alone: running and in-progress states
	   are dots, warning a square, failed a diamond, stopped a hollow ring. */
	.ui-dot {
		display: inline-block;
		width: 7px;
		height: 7px;
		border-radius: 50%;
		flex-shrink: 0;
		box-sizing: border-box;
	}
	.ui-dot--running   { background: var(--accent-green); }
	.ui-dot--pending   { background: var(--accent-yellow); animation: ui-dot-pulse 1.6s ease-in-out infinite; }
	.ui-dot--deploying { background: var(--accent-blue); animation: ui-dot-pulse 1.2s ease-in-out infinite; }
	.ui-dot--warning   { background: var(--accent-yellow); border-radius: 1px; }
	.ui-dot--failed    { background: var(--accent-red); border-radius: 1px; transform: rotate(45deg) scale(0.9); }
	.ui-dot--stopped   { background: transparent; border: 1.5px solid var(--text-dim); }

	@keyframes ui-dot-pulse {
		0%, 100% { opacity: 1; }
		50%      { opacity: 0.35; }
	}
</style>
