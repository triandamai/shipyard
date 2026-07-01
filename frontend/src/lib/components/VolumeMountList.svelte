<!--
  Reusable volume mount binding editor.
  Each row is { source, target, read_only } → Docker mount spec: source:target[:ro]
  Source can be a named volume (e.g. "myvolume") or a bind-mount path (e.g. "/host/data").
  Clicking the 📂 icon on the source field opens VolumePickerPanel to fill from existing volumes.
-->
<script lang="ts">
	import { uiStore } from '$lib/stores/ui.store';
	import { HardDrive, Plus, Trash2, FolderOpen, Lock } from '@lucide/svelte';
	import VolumePickerPanel from '$lib/panels/resources/VolumePickerPanel.svelte';
	import type { Volume } from '$lib/api/types';

	export interface VolumeMount {
		source: string;
		target: string;
		read_only: boolean;
	}

	interface Props {
		projectId: string;
		mounts: VolumeMount[];
	}

	let { projectId, mounts = $bindable([]) }: Props = $props();

	function addMount() {
		mounts = [...mounts, { source: '', target: '', read_only: false }];
	}

	function removeMount(i: number) {
		mounts = mounts.filter((_, idx) => idx !== i);
	}

	function updateSource(i: number, val: string) {
		mounts = mounts.map((m, idx) => idx === i ? { ...m, source: val } : m);
	}

	function updateTarget(i: number, val: string) {
		mounts = mounts.map((m, idx) => idx === i ? { ...m, target: val } : m);
	}

	function toggleReadOnly(i: number) {
		mounts = mounts.map((m, idx) => idx === i ? { ...m, read_only: !m.read_only } : m);
	}

	function openVolumePicker(i: number) {
		uiStore.pushPanel({
			component: VolumePickerPanel,
			title: 'Select Volume',
			props: {
				projectId,
				initialSelected: [],
				onConfirm: (_ids: string[], items: Volume[]) => {
					if (items.length > 0) updateSource(i, items[0].name);
				},
			},
		});
	}
</script>

<div class="mount-list">
	{#if mounts.length === 0}
		<div class="empty-hint">
			No volume mounts. Add one below to bind a named volume or host path into the container.
		</div>
	{:else}
		<div class="mount-rows">
			{#each mounts as mount, i (i)}
				<div class="mount-row">
					<!-- Source -->
					<div class="field source-field">
						<label class="field-label">Source</label>
						<div class="input-with-icon">
							<HardDrive size={12} class="field-icon" />
							<input
								class="mount-input font-mono"
								type="text"
								placeholder="volume-name or /host/path"
								value={mount.source}
								oninput={(e) => updateSource(i, (e.target as HTMLInputElement).value)}
							/>
							<button
								type="button"
								class="pick-btn"
								title="Pick existing volume"
								onclick={() => openVolumePicker(i)}
							>
								<FolderOpen size={12} />
							</button>
						</div>
					</div>

					<span class="arrow">→</span>

					<!-- Target -->
					<div class="field target-field">
						<label class="field-label">Container Path</label>
						<input
							class="mount-input font-mono"
							type="text"
							placeholder="/app/data"
							value={mount.target}
							oninput={(e) => updateTarget(i, (e.target as HTMLInputElement).value)}
						/>
					</div>

					<!-- Read-only toggle -->
					<button
						type="button"
						class="ro-btn"
						class:active={mount.read_only}
						title={mount.read_only ? 'Read-only (click to make writable)' : 'Writable (click to make read-only)'}
						onclick={() => toggleReadOnly(i)}
					>
						<Lock size={11} />
					</button>

					<!-- Remove -->
					<button type="button" class="rm-btn" onclick={() => removeMount(i)}>
						<Trash2 size={12} />
					</button>
				</div>

				<!-- Preview spec -->
				{#if mount.source || mount.target}
					<div class="mount-preview">
						<code>{mount.source || '?'}:{mount.target || '?'}{mount.read_only ? ':ro' : ''}</code>
					</div>
				{/if}
			{/each}
		</div>
	{/if}

	<button type="button" class="add-btn" onclick={addMount}>
		<Plus size={12} />
		Add Volume Mount
	</button>
</div>

<style>
	.mount-list { display: flex; flex-direction: column; gap: 8px; }

	.empty-hint {
		font-size: 11px; color: var(--text-dim); line-height: 1.5;
		padding: 8px 10px; background: var(--bg-base);
		border: 1px dashed var(--border); border-radius: var(--radius-sm);
	}

	.mount-rows { display: flex; flex-direction: column; gap: 10px; }

	.mount-row {
		display: flex; align-items: flex-end; gap: 6px;
	}

	.field { display: flex; flex-direction: column; gap: 3px; }
	.source-field { flex: 1; min-width: 0; }
	.target-field { flex: 1; min-width: 0; }

	.field-label {
		font-size: 10px; font-weight: 600; color: var(--text-dim);
		text-transform: uppercase; letter-spacing: 0.05em;
	}

	.input-with-icon {
		display: flex; align-items: center; position: relative;
		background: var(--bg-elevated); border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		transition: border-color var(--transition-fast);
	}
	.input-with-icon:focus-within { border-color: var(--accent); }

	:global(.field-icon) {
		position: absolute; left: 8px; color: var(--text-dim); pointer-events: none;
	}

	.mount-input {
		background: transparent; border: none; outline: none;
		color: var(--text-primary); font-size: 12px; font-family: var(--font-mono);
		padding: 7px 32px 7px 26px; width: 100%;
	}

	/* target field doesn't have the icon offset */
	.target-field .mount-input {
		background: var(--bg-elevated); border: 1px solid var(--border);
		border-radius: var(--radius-sm); padding: 7px 10px;
	}
	.target-field .mount-input:focus { border-color: var(--accent); outline: none; }

	.pick-btn {
		position: absolute; right: 6px;
		background: none; border: none; padding: 2px; cursor: pointer;
		color: var(--text-dim); display: flex; align-items: center;
		border-radius: 3px; transition: color var(--transition-fast);
	}
	.pick-btn:hover { color: var(--accent); }

	.arrow {
		font-size: 14px; color: var(--text-dim); flex-shrink: 0;
		padding-bottom: 2px; /* align with input baseline */
	}

	.ro-btn {
		width: 30px; height: 30px; flex-shrink: 0;
		background: var(--bg-elevated); border: 1px solid var(--border);
		border-radius: var(--radius-sm); cursor: pointer; color: var(--text-dim);
		display: flex; align-items: center; justify-content: center;
		transition: all var(--transition-fast);
	}
	.ro-btn:hover  { border-color: var(--accent-yellow); color: var(--accent-yellow); }
	.ro-btn.active { border-color: var(--accent-yellow); color: var(--accent-yellow); background: var(--accent-yellow-muted); }

	.rm-btn {
		width: 30px; height: 30px; flex-shrink: 0;
		background: transparent; border: 1px solid var(--border);
		border-radius: var(--radius-sm); cursor: pointer; color: var(--text-dim);
		display: flex; align-items: center; justify-content: center;
		transition: all var(--transition-fast);
	}
	.rm-btn:hover { border-color: var(--accent-red); color: var(--accent-red); }

	.mount-preview {
		padding-left: 2px; margin-top: -4px;
	}
	.mount-preview code {
		font-family: var(--font-mono); font-size: 10px; color: var(--text-dim);
		background: var(--bg-base); padding: 1px 5px; border-radius: 3px;
	}

	.add-btn {
		display: flex; align-items: center; gap: 5px;
		background: transparent; border: 1px dashed var(--border);
		border-radius: var(--radius-sm); color: var(--text-muted);
		font-size: 12px; padding: 6px 12px; cursor: pointer;
		transition: all var(--transition-fast); width: fit-content;
	}
	.add-btn:hover { border-color: var(--accent); color: var(--accent); }

	.font-mono { font-family: var(--font-mono); }
</style>
