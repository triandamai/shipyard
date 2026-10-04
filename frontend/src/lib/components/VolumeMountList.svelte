<!--
  Reusable volume mount binding editor.
  Each row is { source, target, read_only } → Docker mount spec: source:target[:ro]
  Source can be a named volume (e.g. "myvolume") or a bind-mount path (e.g. "/host/data").
  Clicking the 📂 icon on the source field opens VolumePickerPanel to fill from existing volumes.
-->
<script lang="ts">
	import { uiStore } from '$lib/stores/ui.store';
	import { HardDrive, Plus, Trash2, FolderOpen, Lock } from '@lucide/svelte';
	import { FormField, TextField, Button, EmptyState } from '$lib/components/ui';
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
		<EmptyState message="No volume mounts. Add one below to bind a named volume or host path into the container." />
	{:else}
		<div class="mount-rows">
			{#each mounts as mount, i (i)}
				<div class="mount-row">
					<!-- Source -->
					<div class="field">
						<FormField label="Source" for="mount-src-{i}">
							<div class="src-wrap mono-field">
								<TextField
									id="mount-src-{i}"
									type="text"
									placeholder="volume-name or /host/path"
									value={mount.source}
									oninput={(e) => updateSource(i, (e.target as HTMLInputElement).value)}
								>
									{#snippet icon()}<HardDrive size={12} />{/snippet}
								</TextField>
								<span class="pick-btn">
									<Button variant="ghost" size="icon" title="Pick existing volume" aria-label="Pick existing volume" onclick={() => openVolumePicker(i)}>
										<FolderOpen size={12} />
									</Button>
								</span>
							</div>
						</FormField>
					</div>

					<span class="arrow">→</span>

					<!-- Target -->
					<div class="field mono-field">
						<FormField label="Container Path" for="mount-tgt-{i}">
							<TextField
								id="mount-tgt-{i}"
								type="text"
								placeholder="/app/data"
								value={mount.target}
								oninput={(e) => updateTarget(i, (e.target as HTMLInputElement).value)}
							/>
						</FormField>
					</div>

					<!-- Read-only toggle -->
					<span class={mount.read_only ? 'ro ro-on' : 'ro'}>
						<Button
							variant="secondary"
							size="icon"
							title={mount.read_only ? 'Read-only (click to make writable)' : 'Writable (click to make read-only)'}
							aria-label={mount.read_only ? 'Read-only (click to make writable)' : 'Writable (click to make read-only)'}
							onclick={() => toggleReadOnly(i)}
						>
							<Lock size={11} />
						</Button>
					</span>

					<!-- Remove -->
					<Button variant="ghost" size="icon" title="Remove mount" aria-label="Remove mount" onclick={() => removeMount(i)}>
						<Trash2 size={12} />
					</Button>
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

	<div class="add-row">
		<Button variant="secondary" size="sm" onclick={addMount}>
			<Plus size={12} />
			Add Volume Mount
		</Button>
	</div>
</div>

<style>
	.mount-list { display: flex; flex-direction: column; gap: 8px; }
	.mount-rows { display: flex; flex-direction: column; gap: 10px; }
	.mount-row { display: flex; align-items: flex-end; gap: 6px; }
	.field { flex: 1; min-width: 0; }
	.mono-field :global(input) { font-family: var(--font-mono); font-size: 12px; }

	.src-wrap { position: relative; }
	.src-wrap :global(input) { padding-right: 38px; }
	.pick-btn { position: absolute; right: 2px; top: 0; height: 36px; display: flex; align-items: center; }

	.arrow { font-size: 14px; color: var(--text-dim); flex-shrink: 0; padding-bottom: 10px; }

	/* Read-only state: the lock button turns yellow when active */
	.ro { display: flex; }
	.ro :global(.ui-btn:hover) { border-color: var(--accent-yellow); color: var(--accent-yellow); }
	.ro-on :global(.ui-btn) {
		border-color: var(--accent-yellow); color: var(--accent-yellow); background: var(--accent-yellow-muted);
	}

	.mount-row > :global(.ui-btn), .ro { margin-bottom: 2px; }

	.mount-preview { padding-left: 2px; margin-top: -4px; }
	.mount-preview code {
		font-family: var(--font-mono); font-size: 10px; color: var(--text-dim);
		background: var(--bg-base); padding: 1px 5px; border-radius: 3px;
	}

	.add-row { width: fit-content; }
</style>
