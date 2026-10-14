<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { Button, EmptyState, InlineAlert, Skeleton } from '$lib/components/ui';
	import CodeEditor from '$lib/components/CodeEditor.svelte';
	import { authStore } from '$lib/stores/auth.store';

	interface TraefikFileResponse { content: string; path: string }

	let staticFile = $state<TraefikFileResponse | null>(null);
	let editorContent = $state('');
	let loading = $state(true);
	let saving = $state(false);
	let error = $state('');
	let successMsg = $state('');
	let copied = $state(false);

	const canManage = $derived(
		$authStore.user?.is_superadmin === true ||
		($authStore.user?.staff_permissions ?? []).includes('shipyard:admin:infra:traefik:manage')
	);

	let hasChanges = $derived(
		staticFile?.content !== undefined && editorContent !== staticFile.content
	);

	async function load() {
		loading = true; error = ''; successMsg = '';
		const r = await api.get<TraefikFileResponse>('/settings/traefik/static');
		if (r.data) {
			staticFile = r.data;
			editorContent = r.data.content ?? '';
		} else {
			error = r.error?.message ?? 'Failed to load static config';
		}
		loading = false;
	}

	async function save() {
		if (!canManage || saving) return;
		saving = true; error = ''; successMsg = '';
		const r = await api.put<TraefikFileResponse>('/settings/traefik/static', { content: editorContent });
		if (r.data) {
			staticFile = r.data;
			editorContent = r.data.content ?? '';
			successMsg = 'Configuration saved successfully.';
			setTimeout(() => { successMsg = ''; }, 3000);
		} else {
			error = r.error?.message ?? 'Failed to save configuration';
		}
		saving = false;
	}

	async function copyCode(text: string) {
		await navigator.clipboard.writeText(text);
		copied = true;
		setTimeout(() => (copied = false), 2000);
	}

	onMount(load);
</script>

{#if loading}
	<Skeleton variant="card" height="200px" />
{:else if error && !staticFile}
	<InlineAlert tone="error">{error}</InlineAlert>
{:else if staticFile}
	<div class="tpl-section">
		{#if error}
			<div class="alert-row">
				<InlineAlert tone="error">{error}</InlineAlert>
			</div>
		{/if}
		{#if successMsg}
			<div class="alert-row">
				<InlineAlert tone="success">{successMsg}</InlineAlert>
			</div>
		{/if}
		<div class="tpl-hdr">
			<span class="mono tpl-title">{staticFile.path}</span>
			<div class="tpl-actions">
				<Button variant="secondary" size="sm" onclick={() => copyCode(editorContent)}>
					{copied ? 'Copied!' : 'Copy'}
				</Button>
				{#if canManage}
					<Button
						variant="primary"
						size="sm"
						onclick={save}
						disabled={saving || !hasChanges}
					>
						{saving ? 'Saving…' : 'Save'}
					</Button>
				{/if}
			</div>
		</div>
		<CodeEditor
			value={editorContent}
			onChange={(v) => { editorContent = v; }}
			onRun={save}
			language="yaml"
			readonly={!canManage}
			height="480px"
		/>
	</div>
{:else}
	<EmptyState message="No static config found on server." />
{/if}

<style>
	.tpl-section { margin-top:0; }
	.alert-row { margin-bottom: 12px; }
	.tpl-hdr { display:flex; align-items:center; justify-content:space-between; margin-bottom:10px; }
	.tpl-actions { display:flex; align-items:center; gap:8px; }
	.tpl-title { font-size:12px; color:var(--text-secondary); }
	.mono { font-family:var(--font-mono); }
</style>
