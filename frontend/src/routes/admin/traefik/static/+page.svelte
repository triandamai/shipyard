<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';

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
	<div class="card sk-wrap"><div class="sk" style="height:200px"></div></div>
{:else if error}
	<div class="err">{error}</div>
{:else if staticFile}
	<div class="tpl-card">
		<div class="tpl-hdr">
			<span class="tpl-title mono">{staticFile.path}</span>
			<button class="copy-btn" onclick={() => copyCode(staticFile!.content)}>
				{copied ? 'Copied!' : 'Copy'}
			</button>
		</div>
		<pre class="code">{staticFile.content}</pre>
	</div>
{:else}
	<div class="empty">No static config found on server.</div>
{/if}

<style>
	.card { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); padding:24px; box-shadow:var(--shadow-sm); }
	.sk-wrap { display:flex; flex-direction:column; gap:16px; }
	.sk { background:var(--border); border-radius:4px; animation:sk 1.3s ease-in-out infinite; }
	@keyframes sk { 0%,100%{opacity:.5} 50%{opacity:1} }

	.tpl-card { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:var(--shadow-sm); }
	.tpl-hdr { display:flex; align-items:center; justify-content:space-between; padding:10px 14px; border-bottom:1px solid var(--border); background:var(--surface-2); }
	.tpl-title { font-size:12px; font-weight:600; color:var(--text-2); }
	.copy-btn { padding:4px 11px; border-radius:var(--radius-sm); font-size:11px; font-weight:600; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); transition:background .15s, color .15s; font-family:var(--font); }
	.copy-btn:hover { background:var(--accent); border-color:var(--accent); color:#000; }
	.code { margin:0; padding:16px; font-size:11.5px; line-height:1.65; color:var(--text-2); font-family:var(--mono); white-space:pre-wrap; word-break:break-all; overflow-x:auto; }
	.mono { font-family:var(--mono); }
	.err { padding:11px 14px; background:var(--danger-soft); border:1px solid rgba(220,38,38,0.2); border-radius:var(--radius); font-size:13px; color:var(--danger); }
	.empty { padding:48px; text-align:center; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); color:var(--text-3); font-size:13px; }
</style>
