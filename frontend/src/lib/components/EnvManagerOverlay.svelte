<script lang="ts">
	import { X } from '@lucide/svelte';
	import { Button } from '$lib/components/ui';
	import EnvManagerPanel from '$lib/panels/EnvManagerPanel.svelte';

	interface Props {
		open:         boolean;
		onClose:      () => void;
		serviceId:    string;
		projectId:    string;
		serviceName?: string;
	}

	let { open, onClose, serviceId, projectId, serviceName = 'Service' }: Props = $props();
</script>

{#if open}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div
		class="emo-backdrop"
		role="presentation"
		onclick={(e) => { if (e.target === e.currentTarget) onClose(); }}
		onkeydown={() => {}}
	>
		<div class="emo-panel">

			<!-- Header -->
			<div class="emo-header">
				<div class="emo-title-group">
					<span class="emo-title">Environment Variables</span>
					<span class="emo-subtitle">{serviceName}</span>
				</div>
				<Button variant="ghost" size="icon" onclick={onClose} title="Close" aria-label="Close">
					<X size={15} />
				</Button>
			</div>

			<!-- Body -->
			<div class="emo-body">
				<EnvManagerPanel {serviceId} {projectId} {serviceName} />
			</div>
		</div>
	</div>
{/if}

<style>
	.emo-backdrop {
		position: fixed; inset: 0;
		background: rgba(0, 0, 0, 0.55);
		display: flex; align-items: flex-end; justify-content: center;
		z-index: 500;
		padding: 0;
	}

	.emo-panel {
		width: 100%; max-width: 1100px;
		height: 68vh; min-height: 400px;
		display: flex; flex-direction: column;
		background: var(--bg-base);
		border: 1px solid var(--border);
		border-bottom: none;
		border-radius: var(--radius-lg) var(--radius-lg) 0 0;
		overflow: hidden;
		box-shadow: var(--shadow-lg);
	}

	.emo-header {
		display: flex; align-items: center;
		padding: 10px 14px; gap: 10px;
		background: var(--bg-surface);
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}

	.emo-title-group {
		display: flex; align-items: center; gap: 10px; flex: 1; min-width: 0;
	}

	.emo-title {
		font-size: 13px; font-weight: 700; color: var(--text-primary);
		white-space: nowrap;
	}

	.emo-subtitle {
		font-size: 11px; color: var(--text-muted);
		font-family: var(--font-mono);
		white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
	}

	.emo-body {
		flex: 1; min-height: 0; overflow: hidden;
		display: flex; flex-direction: column;
	}
</style>
