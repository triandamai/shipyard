<script lang="ts">
	import { X } from '@lucide/svelte';
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
				<button class="emo-close-btn" onclick={onClose} title="Close">
					<X size={15} />
				</button>
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
		background: rgba(0, 0, 0, 0.65);
		display: flex; align-items: flex-end; justify-content: center;
		z-index: 500;
		padding: 0;
	}

	.emo-panel {
		width: 100%; max-width: 1100px;
		height: 68vh; min-height: 400px;
		display: flex; flex-direction: column;
		background: #0d1117;
		border: 1px solid rgba(255, 255, 255, 0.08);
		border-bottom: none;
		border-radius: 10px 10px 0 0;
		overflow: hidden;
		box-shadow: 0 -8px 40px rgba(0, 0, 0, 0.6);
	}

	.emo-header {
		display: flex; align-items: center;
		padding: 10px 14px; gap: 10px;
		background: #161b22;
		border-bottom: 1px solid rgba(255, 255, 255, 0.07);
		flex-shrink: 0;
	}

	.emo-title-group {
		display: flex; align-items: center; gap: 10px; flex: 1; min-width: 0;
	}

	.emo-title {
		font-size: 13px; font-weight: 700; color: #e6edf3;
		white-space: nowrap;
	}

	.emo-subtitle {
		font-size: 11px; color: #8b949e;
		font-family: var(--font-mono, monospace);
		white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
	}

	.emo-close-btn {
		display: flex; align-items: center; justify-content: center;
		width: 28px; height: 28px; border-radius: 6px;
		background: none; border: none; cursor: pointer;
		color: #8b949e; transition: all 0.12s;
	}
	.emo-close-btn:hover { background: rgba(255, 255, 255, 0.08); color: #e6edf3; }

	.emo-body {
		flex: 1; min-height: 0; overflow: hidden;
		display: flex; flex-direction: column;
	}
</style>
