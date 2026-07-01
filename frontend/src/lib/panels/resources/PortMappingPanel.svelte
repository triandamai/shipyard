<script lang="ts">
	import { Plus, Trash2, Check } from '@lucide/svelte';
	import { uiStore } from '$lib/stores/ui.store';

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
			<span class="col-container">Container port</span>
			<span class="col-host">Host port <span class="optional">(optional)</span></span>
			<span class="col-proto">Protocol</span>
			<span class="col-del"></span>
		</div>

		{#if entries.length === 0}
			<div class="pm-empty">No ports added yet.</div>
		{:else}
			{#each entries as entry (entry.id)}
				<div class="pm-row">
					<input
						class="pm-input col-container"
						type="text"
						placeholder="3000"
						value={entry.containerPort}
						oninput={(e) => update(entry.id, 'containerPort', (e.target as HTMLInputElement).value)}
						spellcheck="false"
					/>
					<input
						class="pm-input col-host"
						type="text"
						placeholder="leave blank = not exposed"
						value={entry.hostPort}
						oninput={(e) => update(entry.id, 'hostPort', (e.target as HTMLInputElement).value)}
						spellcheck="false"
					/>
					<select
						class="pm-select col-proto"
						value={entry.protocol}
						onchange={(e) => update(entry.id, 'protocol', (e.target as HTMLSelectElement).value)}
					>
						<option value="tcp">TCP</option>
						<option value="udp">UDP</option>
					</select>
					<button class="pm-del col-del" type="button" onclick={() => removeEntry(entry.id)} title="Remove">
						<Trash2 size={12} />
					</button>
				</div>
			{/each}
		{/if}
	</div>

	<button class="btn btn-secondary btn-sm add-btn" type="button" onclick={addEntry}>
		<Plus size={13} />
		Add Port
	</button>

	{#if validationError}
		<div class="pm-error">{validationError}</div>
	{/if}

	<div class="pm-footer">
		<button class="btn btn-primary" type="button" onclick={save}>
			<Check size={14} />
			Save Port Mapping
		</button>
	</div>
</div>

<style>
	.pm-wrap {
		padding: 14px;
		display: flex;
		flex-direction: column;
		gap: 14px;
		height: 100%;
	}

	.pm-hint {
		font-size: 12px;
		color: var(--text-muted);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		padding: 8px 10px;
		line-height: 1.5;
	}

	.pm-table {
		display: flex;
		flex-direction: column;
		gap: 0;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		overflow: hidden;
	}

	.pm-thead {
		display: grid;
		grid-template-columns: 1fr 1fr 72px 28px;
		gap: 0;
		padding: 6px 10px;
		background: var(--bg-elevated);
		border-bottom: 1px solid var(--border);
		font-size: 10px;
		font-weight: 600;
		color: var(--text-dim);
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}

	.pm-empty {
		padding: 20px;
		text-align: center;
		font-size: 12px;
		color: var(--text-dim);
	}

	.pm-row {
		display: grid;
		grid-template-columns: 1fr 1fr 72px 28px;
		gap: 0;
		border-bottom: 1px solid var(--border);
		align-items: center;
	}
	.pm-row:last-child { border-bottom: none; }

	.pm-input {
		background: transparent;
		border: none;
		border-right: 1px solid var(--border);
		color: var(--text-primary);
		font-family: var(--font-mono);
		font-size: 12px;
		padding: 8px 10px;
		outline: none;
		width: 100%;
		box-sizing: border-box;
		transition: background var(--transition-fast);
	}
	.pm-input:focus { background: var(--bg-elevated); }

	.pm-select {
		background: transparent;
		border: none;
		border-right: 1px solid var(--border);
		color: var(--text-secondary);
		font-size: 11px;
		font-family: var(--font-sans);
		padding: 8px 6px;
		outline: none;
		cursor: pointer;
		width: 100%;
		box-sizing: border-box;
	}
	.pm-select:focus { background: var(--bg-elevated); }

	.pm-del {
		background: transparent;
		border: none;
		cursor: pointer;
		color: var(--text-dim);
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 8px 6px;
		transition: color var(--transition-fast);
	}
	.pm-del:hover { color: #EF4444; }

	.optional {
		font-weight: 400;
		text-transform: none;
		letter-spacing: 0;
		font-size: 9px;
		opacity: 0.7;
	}

	.add-btn {
		align-self: flex-start;
		display: flex;
		align-items: center;
		gap: 5px;
	}

	.pm-error {
		font-size: 12px;
		color: #EF4444;
		background: rgba(239,68,68,0.08);
		border: 1px solid rgba(239,68,68,0.2);
		border-radius: var(--radius-sm);
		padding: 7px 10px;
	}

	.pm-footer {
		margin-top: auto;
		padding-top: 4px;
		border-top: 1px solid var(--border);
	}

	.pm-footer .btn {
		width: 100%;
		justify-content: center;
		display: flex;
		align-items: center;
		gap: 6px;
	}
</style>
