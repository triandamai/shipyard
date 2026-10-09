<script lang="ts">
	import { Plus, Trash2, Check } from '@lucide/svelte';
	import { uiStore } from '$lib/stores/ui.store';
	import { TextField, Select, Button, InlineAlert } from '$lib/components/ui';

	interface Props {
		initialPorts?: string[];
		onConfirm: (ports: string[]) => void;
	}

	let { initialPorts = [], onConfirm }: Props = $props();

	interface PortEntry {
		id: number;
		containerPort: string;
		hostPort: string;
		protocol: 'tcp' | 'udp';
	}

	let nextId = 0;

	function parsePortString(s: string): PortEntry {
		let protocol: 'tcp' | 'udp' = 'tcp';
		let mapping = s.trim();
		if (mapping.endsWith('/udp')) {
			protocol = 'udp';
			mapping = mapping.slice(0, -4);
		} else if (mapping.endsWith('/tcp')) {
			mapping = mapping.slice(0, -4);
		}
		const colonIdx = mapping.indexOf(':');
		if (colonIdx !== -1) {
			return { id: nextId++, hostPort: mapping.slice(0, colonIdx), containerPort: mapping.slice(colonIdx + 1), protocol };
		}
		return { id: nextId++, containerPort: mapping, hostPort: '', protocol };
	}

	function toPortString(e: PortEntry): string {
		const container = e.containerPort.trim();
		const host = e.hostPort.trim();
		// No host port → internal-only, not published to the host.
		const base = host ? `${host}:${container}` : container;
		return e.protocol === 'udp' ? `${base}/udp` : base;
	}

	let entries = $state<PortEntry[]>(
		initialPorts.filter(Boolean).map(parsePortString)
	);

	function addEntry() {
		entries = [...entries, { id: nextId++, containerPort: '', hostPort: '', protocol: 'tcp' }];
	}

	function removeEntry(id: number) {
		entries = entries.filter(e => e.id !== id);
	}

	function update(id: number, field: keyof PortEntry, val: string) {
		entries = entries.map(e => e.id === id ? { ...e, [field]: val } : e);
	}

	let validationError = $state('');

	function save() {
		validationError = '';
		const valid = entries.filter(e => e.containerPort.trim());
		const invalid = valid.filter(e => isNaN(parseInt(e.containerPort)) || (e.hostPort && isNaN(parseInt(e.hostPort))));
		if (invalid.length > 0) {
			validationError = 'Port numbers must be numeric.';
			return;
		}
		const ports = valid.map(toPortString);
		onConfirm(ports);
		uiStore.popPanel();
	}
</script>

<div class="pm-wrap">
	<div class="pm-hint">
		Add one row per port the container exposes. Leave host port blank to keep the port internal (not published to the host). Fill it in to bind a host port (e.g. host 8080 → container 3000).
	</div>

	<div class="pm-table">
		<div class="pm-thead">
			<span>Container port</span>
			<span>Host port <span class="optional">(optional)</span></span>
			<span>Protocol</span>
			<span></span>
		</div>

		{#if entries.length === 0}
			<div class="pm-empty">No ports added yet.</div>
		{:else}
			{#each entries as entry (entry.id)}
				<div class="pm-row">
					<div class="pm-mono">
						<TextField
							type="text"
							placeholder="3000"
							aria-label="Container port"
							value={entry.containerPort}
							oninput={(e) => update(entry.id, 'containerPort', (e.target as HTMLInputElement).value)}
							spellcheck="false"
						/>
					</div>
					<div class="pm-mono">
						<TextField
							type="text"
							placeholder="blank = not exposed"
							aria-label="Host port"
							value={entry.hostPort}
							oninput={(e) => update(entry.id, 'hostPort', (e.target as HTMLInputElement).value)}
							spellcheck="false"
						/>
					</div>
					<Select
						aria-label="Protocol"
						value={entry.protocol}
						onchange={(e) => update(entry.id, 'protocol', (e.target as HTMLSelectElement).value)}
						options={[{ value: 'tcp', label: 'TCP' }, { value: 'udp', label: 'UDP' }]}
					/>
					<Button variant="ghost" size="icon" title="Remove" aria-label="Remove port" onclick={() => removeEntry(entry.id)}>
						<Trash2 size={13} />
					</Button>
				</div>
			{/each}
		{/if}
	</div>

	<div class="add-row">
		<Button variant="secondary" size="sm" onclick={addEntry}>
			<Plus size={13} />
			Add Port
		</Button>
	</div>

	{#if validationError}
		<div role="alert"><InlineAlert tone="error">{validationError}</InlineAlert></div>
	{/if}

	<div class="pm-footer">
		<Button variant="primary" onclick={save}>
			<Check size={14} />
			Save Port Mapping
		</Button>
	</div>
</div>

<style>
	.pm-wrap { padding: 14px; display: flex; flex-direction: column; gap: 14px; height: 100%; }

	.pm-hint {
		font-size: 12px; color: var(--text-muted); background: var(--bg-elevated);
		border: 1px solid var(--border); border-radius: var(--radius-sm);
		padding: 8px 10px; line-height: 1.5;
	}

	.pm-table {
		display: flex; flex-direction: column;
		border: 1px solid var(--border); border-radius: var(--radius-sm); overflow: hidden;
	}
	.pm-thead, .pm-row {
		display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) 76px 36px;
		gap: 8px; align-items: center; padding: 6px 10px;
	}
	.pm-thead {
		background: var(--bg-elevated); border-bottom: 1px solid var(--border);
		font-size: 10px; font-weight: 600; color: var(--text-dim);
	}
	.pm-row { border-bottom: 1px solid var(--border); }
	.pm-row:last-child { border-bottom: none; }
	.pm-mono :global(input) { font-family: var(--font-mono); font-size: 12px; }
	.pm-empty { padding: 20px; text-align: center; font-size: 12px; color: var(--text-dim); }

	.optional { font-weight: 400; text-transform: none; letter-spacing: 0; font-size: 9px; opacity: 0.7; }

	.add-row { align-self: flex-start; }

	.pm-footer { margin-top: auto; padding-top: 4px; border-top: 1px solid var(--border); }
	.pm-footer :global(.ui-btn) { width: 100%; justify-content: center; }
</style>
