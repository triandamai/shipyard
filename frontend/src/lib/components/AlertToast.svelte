<script lang="ts">
	import { onMount } from 'svelte';
	import { Cpu, MemoryStick, HardDrive, Wifi, X } from 'lucide-svelte';
	import { alertsStore, type SpikeAlert } from '$lib/stores/alerts.store';

	const ICONS = {
		cpu:  Cpu,
		mem:  MemoryStick,
		disk: HardDrive,
		net:  Wifi,
	} as const;

	const LABELS = {
		cpu:  'CPU spike',
		mem:  'Memory spike',
		disk: 'Disk usage',
		net:  'Network spike',
	} as const;

	const UNITS = {
		cpu:  '%',
		mem:  '%',
		disk: '%',
		net:  ' Mbps',
	} as const;

	// Auto-dismiss timers keyed by alert id
	let timers: Record<string, ReturnType<typeof setTimeout>> = {};

	function schedule(alert: SpikeAlert) {
		if (timers[alert.id]) return;
		timers[alert.id] = setTimeout(() => {
			alertsStore.dismiss(alert.id);
			delete timers[alert.id];
		}, 8000);
	}

	$effect(() => {
		// Schedule auto-dismiss for the 3 most recent (visible) alerts
		alertsStore.alerts.slice(0, 3).forEach(schedule);
	});

	onMount(() => () => Object.values(timers).forEach(clearTimeout));
</script>

<div class="alert-stack" aria-live="polite">
	{#each alertsStore.alerts.slice(0, 3) as alert (alert.id)}
		{@const Icon = ICONS[alert.metric] ?? Cpu}
		<div class="alert-toast alert-{alert.metric}" role="alert">
			<div class="alert-icon"><Icon size={14} /></div>
			<div class="alert-body">
				<span class="alert-title">{LABELS[alert.metric] ?? alert.metric}</span>
				<span class="alert-detail">
					{alert.value.toFixed(1)}{UNITS[alert.metric]} &gt; {alert.threshold}{UNITS[alert.metric]}
					· {alert.node_id}
				</span>
			</div>
			<button class="alert-close" onclick={() => alertsStore.dismiss(alert.id)} aria-label="Dismiss">
				<X size={12} />
			</button>
		</div>
	{/each}
</div>

<style>
	.alert-stack {
		position: fixed;
		bottom: 20px;
		right: 20px;
		z-index: 9999;
		display: flex;
		flex-direction: column;
		gap: 8px;
		pointer-events: none;
	}

	.alert-toast {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		padding: 10px 12px;
		border-radius: var(--radius-md, 8px);
		background: var(--surface-raised, #1e2535);
		border: 1px solid var(--border, #2d3748);
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
		min-width: 260px;
		max-width: 340px;
		pointer-events: all;
		animation: slide-in 0.2s ease;
	}

	@keyframes slide-in {
		from { transform: translateX(20px); opacity: 0; }
		to   { transform: translateX(0);    opacity: 1; }
	}

	.alert-cpu  { border-left: 3px solid #f59e0b; }
	.alert-mem  { border-left: 3px solid #8b5cf6; }
	.alert-disk { border-left: 3px solid #ef4444; }
	.alert-net  { border-left: 3px solid #3b82f6; }

	.alert-icon {
		flex-shrink: 0;
		margin-top: 1px;
		color: var(--text-muted);
	}
	.alert-cpu  .alert-icon { color: #f59e0b; }
	.alert-mem  .alert-icon { color: #8b5cf6; }
	.alert-disk .alert-icon { color: #ef4444; }
	.alert-net  .alert-icon { color: #3b82f6; }

	.alert-body {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.alert-title {
		font-size: 13px;
		font-weight: 600;
		color: var(--text-primary);
	}

	.alert-detail {
		font-size: 11px;
		color: var(--text-muted);
		line-height: 1.4;
	}

	.alert-close {
		flex-shrink: 0;
		background: none;
		border: none;
		cursor: pointer;
		color: var(--text-dim, #4a5568);
		padding: 0;
		display: flex;
		align-items: center;
		margin-top: 1px;
	}
	.alert-close:hover { color: var(--text-muted); }
</style>
