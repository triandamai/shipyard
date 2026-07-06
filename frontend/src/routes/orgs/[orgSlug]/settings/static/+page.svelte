<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { can, perm, isAdminRole } from '$lib/auth/permissions';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';
	import { FileText, RefreshCw, ChevronRight, AlertCircle, Globe } from '@lucide/svelte';

	interface NginxConfEntry { name: string }
	interface NginxConfList {
		dir: string;
		files: NginxConfEntry[];
		error?: string;
	}
	interface NginxConfFile {
		name: string;
		content?: string;
		exists: boolean;
		error?: string;
	}

	let orgId    = $derived($orgStore.activeOrg?.id ?? '');
	let myRole   = $derived($orgStore.myMembership?.role ?? null);
	let myPerms  = $derived($orgStore.myMembership?.permissions ?? []);
	let membershipLoaded = $derived($orgStore.membershipLoaded);
	let isAdmin  = $derived(isAdminRole(myRole));
	let canRead  = $derived(isAdmin || can(myRole, myPerms, perm(orgId, 'static', 'read')) || can(myRole, myPerms, perm(orgId, 'infra', 'read')));

	let confList   = $state<NginxConfList | null>(null);
	let loading    = $state(true);
	let listError  = $state('');

	let selected    = $state<string | null>(null);
	let fileContent = $state<NginxConfFile | null>(null);
	let loadingFile = $state(false);

	async function loadList() {
		loading = true;
		listError = '';
		const res = await api.get<NginxConfList>(`/admin/nginx-static/confs?org_id=${orgId}`);
		if (res.data) {
			confList = res.data;
			if (res.data.error) listError = res.data.error;
		} else if (res.error) {
			listError = res.error.message;
		}
		loading = false;
	}

	async function selectConf(name: string) {
		if (selected === name) {
			selected = null;
			fileContent = null;
			return;
		}
		selected = name;
		loadingFile = true;
		fileContent = null;
		const res = await api.get<NginxConfFile>(`/admin/nginx-static/confs/${encodeURIComponent(name)}?org_id=${orgId}`);
		if (res.data) fileContent = res.data;
		loadingFile = false;
	}

	function siteIdFromName(name: string): string {
		return name.replace(/\.conf$/, '');
	}

	onMount(() => {
		if (orgId) loadList();
	});

	$effect(() => {
		if (orgId && canRead) loadList();
	});
</script>

<PermissionDeniedDialog
	open={membershipLoaded && !!orgId && !canRead}
	message="You need the 'View static server' or 'View infrastructure' permission to access this page."
	onDismiss={() => history.back()}
	onBack={() => history.back()}
/>

{#if canRead}
<div class="static-page">

	<div class="page-toolbar">
		<div class="toolbar-left">
			<Globe size={15} />
			<span class="toolbar-title">Static Server</span>
			{#if confList}
				<span class="count-chip">{confList.files.length} site{confList.files.length === 1 ? '' : 's'}</span>
			{/if}
		</div>
		<button class="refresh-btn" onclick={loadList} disabled={loading}>
			<RefreshCw size={14} class={loading ? 'spin' : ''} />
			Refresh
		</button>
	</div>

	{#if listError}
		<div class="error-banner"><AlertCircle size={14} />{listError}</div>
	{/if}

	{#if loading}
		<div class="empty-state"><div class="spinner"></div> Loading nginx conf files…</div>
	{:else if !confList || confList.files.length === 0}
		<div class="empty-state muted">No .conf files found in {confList?.dir ?? '/etc/nginx/conf.d'}</div>
	{:else}

		<div class="conf-layout">

			<!-- File list -->
			<div class="conf-list-panel">
				<div class="panel-header">
					<FileText size={13} />
					<span>{confList.dir}</span>
				</div>
				<ul class="conf-list">
					{#each confList.files as entry (entry.name)}
						<li>
							<button
								class="conf-item"
								class:active={selected === entry.name}
								onclick={() => selectConf(entry.name)}
							>
								<div class="conf-item-inner">
									<div class="conf-dot"></div>
									<div class="conf-info">
										<span class="conf-name">{entry.name}</span>
										<span class="conf-id">{siteIdFromName(entry.name)}</span>
									</div>
								</div>
								<ChevronRight size={13} class="conf-chevron" />
							</button>
						</li>
					{/each}
				</ul>
			</div>

			<!-- File content -->
			<div class="conf-content-panel">
				{#if !selected}
					<div class="content-empty">
						<FileText size={24} />
						<span>Select a conf file to view its content</span>
					</div>
				{:else if loadingFile}
					<div class="content-empty"><div class="spinner"></div> Loading…</div>
				{:else if fileContent?.error}
					<div class="content-empty error"><AlertCircle size={16} />{fileContent.error}</div>
				{:else if fileContent?.content}
					<div class="content-header">
						<span class="content-filename">{fileContent.name}</span>
					</div>
					<pre class="conf-code">{fileContent.content}</pre>
				{/if}
			</div>

		</div>

	{/if}

</div>
{/if}

<style>
	@keyframes spin { to { transform: rotate(360deg); } }
	:global(.spin) { animation: spin 0.8s linear infinite; }

	.static-page { display: flex; flex-direction: column; gap: 16px; }

	.page-toolbar {
		display: flex; align-items: center; justify-content: space-between;
	}
	.toolbar-left { display: flex; align-items: center; gap: 8px; color: var(--text-secondary); font-size: 13px; }
	.toolbar-title { font-weight: 600; color: var(--text-primary); }
	.count-chip {
		padding: 2px 8px; background: var(--bg-muted); border: 1px solid var(--border);
		border-radius: 10px; font-size: 11px; color: var(--text-muted);
	}
	.refresh-btn {
		display: flex; align-items: center; gap: 6px;
		padding: 6px 12px; font-size: 12px; font-weight: 500;
		background: var(--bg-surface); border: 1px solid var(--border);
		border-radius: var(--radius); color: var(--text-secondary);
		cursor: pointer; transition: all var(--transition-fast);
	}
	.refresh-btn:hover:not(:disabled) { border-color: var(--accent); color: var(--accent); }
	.refresh-btn:disabled { opacity: 0.5; cursor: default; }

	.error-banner { display: flex; align-items: center; gap: 8px; padding: 10px 14px; background: rgba(239,68,68,0.08); border: 1px solid rgba(239,68,68,0.25); border-radius: var(--radius-md); color: #EF4444; font-size: 13px; }

	.empty-state { display: flex; align-items: center; justify-content: center; gap: 10px; padding: 60px; color: var(--text-muted); font-size: 13px; }
	.empty-state.muted { color: var(--text-dim); font-style: italic; }
	.spinner { width: 18px; height: 18px; border: 2px solid var(--border); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.8s linear infinite; }

	.conf-layout {
		display: grid;
		grid-template-columns: 260px 1fr;
		gap: 0;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		overflow: hidden;
		min-height: 400px;
	}

	.conf-list-panel {
		border-right: 1px solid var(--border);
		display: flex;
		flex-direction: column;
	}
	.panel-header {
		display: flex; align-items: center; gap: 7px;
		padding: 10px 14px;
		font-size: 11px; font-weight: 600; color: var(--text-muted);
		text-transform: uppercase; letter-spacing: 0.05em;
		background: var(--bg-muted); border-bottom: 1px solid var(--border);
		font-family: var(--font-mono);
	}
	.conf-list { list-style: none; margin: 0; padding: 0; overflow-y: auto; flex: 1; }

	.conf-item {
		width: 100%;
		display: flex; align-items: center; justify-content: space-between;
		padding: 10px 14px;
		background: transparent; border: none; border-bottom: 1px solid var(--border);
		cursor: pointer; text-align: left;
		transition: background var(--transition-fast);
	}
	.conf-item:last-child { border-bottom: none; }
	.conf-item:hover { background: var(--bg-elevated); }
	.conf-item.active { background: color-mix(in srgb, var(--accent) 8%, transparent); }
	.conf-item.active :global(.conf-chevron) { color: var(--accent); }

	.conf-item-inner { display: flex; align-items: center; gap: 10px; min-width: 0; }
	.conf-dot { width: 7px; height: 7px; border-radius: 50%; background: #22C55E; flex-shrink: 0; }
	.conf-info { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
	.conf-name { font-size: 12px; font-weight: 500; color: var(--text-primary); font-family: var(--font-mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.conf-id   { font-size: 10px; color: var(--text-dim); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	:global(.conf-chevron) { color: var(--text-dim); flex-shrink: 0; transition: color var(--transition-fast); }

	.conf-content-panel {
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}
	.content-empty {
		flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
		gap: 10px; color: var(--text-muted); font-size: 13px;
	}
	.content-empty.error { color: #EF4444; }
	.content-header {
		padding: 10px 16px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-muted);
	}
	.content-filename { font-size: 12px; font-weight: 600; color: var(--text-primary); font-family: var(--font-mono); }
	.conf-code {
		margin: 0;
		padding: 16px;
		font-family: var(--font-mono);
		font-size: 12px;
		line-height: 1.6;
		color: var(--text-secondary);
		white-space: pre;
		overflow: auto;
		flex: 1;
		background: transparent;
	}

	@media (max-width: 767px) {
		.conf-layout { grid-template-columns: 1fr; }
		.conf-list-panel { border-right: none; border-bottom: 1px solid var(--border); max-height: 220px; }
	}
</style>
