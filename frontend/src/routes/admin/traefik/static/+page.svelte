<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { Card, Button, EmptyState, InlineAlert, Skeleton } from '$lib/components/ui';

	interface TraefikFileResponse { content: string; path: string }

	let staticFile = $state<TraefikFileResponse | null>(null);
	let loading = $state(true);
	let error = $state('');
	let copied = $state(false);

	async function load() {
		loading = true; error = '';
		const r = await api.get<TraefikFileResponse>('/settings/traefik/static');
		if (r.data) staticFile = r.data;
		else error = r.error?.message ?? 'Failed to load static config';
		loading = false;
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
{:else if error}
	<InlineAlert tone="error">{error}</InlineAlert>
{:else if staticFile}
	<div class="tpl-section">
		<div class="tpl-hdr">
			<span class="mono tpl-title">{staticFile.path}</span>
			<Button variant="secondary" size="sm" onclick={() => copyCode(staticFile!.content)}>
				{copied ? 'Copied!' : 'Copy'}
			</Button>
		</div>
		<Card padding="0">
			<pre class="code">{staticFile.content}</pre>
		</Card>
	</div>
{:else}
	<EmptyState message="No static config found on server." />
{/if}

<style>
	.tpl-section { margin-top:0; }
	.tpl-hdr { display:flex; align-items:center; justify-content:space-between; margin-bottom:10px; }
	.tpl-title { font-size:12px; color:var(--text-secondary); }
	.code { margin:0; padding:16px; font-size:11.5px; line-height:1.65; color:var(--text-secondary); font-family:var(--font-mono); white-space:pre-wrap; word-break:break-all; overflow-x:auto; }

	.mono { font-family:var(--font-mono); }
</style>
