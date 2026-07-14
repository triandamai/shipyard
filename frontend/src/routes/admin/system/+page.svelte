<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';

	type SwarmNode = { id: string; hostname: string; role: string; status: string; availability: string; engine_version: string | null; addr: string | null };

	let config = $state<Record<string, unknown>>({});
	let edits = $state<Record<string, string>>({});
	let saving = $state<string | null>(null);
	let saved = $state<string | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let saveErrors = $state<Record<string, string>>({});

	let swarmNodes = $state<SwarmNode[]>([]);
	let swarmLoading = $state(true);
	let swarmError = $state<string | null>(null);

	onMount(async () => {
		const [configRes, swarmRes] = await Promise.all([
			api.getSystemConfig(),
			api.getSwarmNodes(),
		]);

		if (configRes.data) {
			config = configRes.data;
			for (const [k, v] of Object.entries(configRes.data)) {
				edits[k] = JSON.stringify(v, null, 2);
			}
		} else {
			error = configRes.error?.message ?? 'Failed to load system config';
		}
		loading = false;

		if (swarmRes.data) {
			swarmNodes = swarmRes.data;
		} else {
			swarmError = swarmRes.error?.message ?? 'Failed to load swarm nodes';
		}
		swarmLoading = false;
	});

	async function save(key: string) {
		saving = key;
		saveErrors = { ...saveErrors, [key]: '' };
		let parsed: unknown;
		try {
			parsed = JSON.parse(edits[key]);
		} catch {
			saveErrors = { ...saveErrors, [key]: 'Invalid JSON' };
			saving = null;
			return;
		}
		const res = await api.patchSystemConfig(key, parsed);
		if (res.error) {
			saveErrors = { ...saveErrors, [key]: res.error.message };
		} else {
			saved = key;
			setTimeout(() => { if (saved === key) saved = null; }, 2500);
		}
		saving = null;
	}

	function typeOf(v: unknown): string {
		if (v === null) return 'null';
		if (Array.isArray(v)) return 'array';
		return typeof v;
	}

	// Subtle type label colors — not loud, just informative
	const typeColor: Record<string, string> = {
		string:  'var(--ok)',
		number:  'var(--accent)',
		boolean: 'var(--warn)',
		object:  'var(--text-3)',
		array:   'var(--text-3)',
		null:    'var(--text-3)',
	};
</script>

<div class="p">
	<!-- Swarm Nodes -->
	<section class="section">
		<h2 class="sec-ttl">Swarm Nodes</h2>
		{#if swarmLoading}
			<div class="tbl-shell">
				{#each [0,1] as _}
					<div class="sk-row">
						<div class="sk sk-host"></div>
						<div class="sk sk-role"></div>
						<div class="sk sk-ip"></div>
					</div>
				{/each}
			</div>
		{:else if swarmError}
			<div class="err">
				<svg viewBox="0 0 20 20" fill="currentColor" width="13" height="13"><path fill-rule="evenodd" d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7 4a1 1 0 11-2 0 1 1 0 012 0zm-1-9a1 1 0 00-1 1v4a1 1 0 102 0V6a1 1 0 00-1-1z" clip-rule="evenodd"/></svg>
				{swarmError}
			</div>
		{:else if swarmNodes.length === 0}
			<div class="empty">No swarm nodes found.</div>
		{:else}
			<div class="tbl-shell">
				<div class="tbl-head">
					<span>Hostname</span>
					<span>Role</span>
					<span>Status</span>
					<span>Availability</span>
					<span>IP Address</span>
					<span>Engine</span>
				</div>
				{#each swarmNodes as node}
					<div class="tbl-row">
						<span class="node-host">
							<code class="node-id">{node.id.slice(0, 12)}</code>
							<span class="node-name">{node.hostname}</span>
						</span>
						<span>
							<span class="badge {node.role === 'manager' ? 'badge-mgr' : 'badge-wkr'}">{node.role}</span>
						</span>
						<span>
							<span class="badge {node.status === 'ready' ? 'badge-ok' : 'badge-err'}">{node.status}</span>
						</span>
						<span class="cell-muted">{node.availability}</span>
						<span class="cell-ip">{node.addr ?? '—'}</span>
						<span class="cell-muted">{node.engine_version ?? '—'}</span>
					</div>
				{/each}
			</div>
		{/if}
	</section>

	<header class="hdr">
		<div>
			<h1 class="ttl">System Config</h1>
			<p class="sub">Platform-wide JSONB settings. Changes apply immediately.</p>
		</div>
		<div class="warn-tag">
			<svg viewBox="0 0 20 20" fill="currentColor" width="12" height="12">
				<path fill-rule="evenodd" d="M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l5.58 9.92c.75 1.334-.213 2.98-1.742 2.98H4.42c-1.53 0-2.493-1.646-1.743-2.98l5.58-9.92zM11 13a1 1 0 11-2 0 1 1 0 012 0zm-1-8a1 1 0 00-1 1v3a1 1 0 002 0V6a1 1 0 00-1-1z" clip-rule="evenodd"/>
			</svg>
			Danger zone
		</div>
	</header>

	{#if loading}
		<div class="cfg-shell">
			{#each [0,1,2] as _}
				<div class="sk-row">
					<div class="sk sk-key"></div>
					<div class="sk sk-val"></div>
				</div>
			{/each}
		</div>
	{:else if error}
		<div class="err">
			<svg viewBox="0 0 20 20" fill="currentColor" width="13" height="13"><path fill-rule="evenodd" d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7 4a1 1 0 11-2 0 1 1 0 012 0zm-1-9a1 1 0 00-1 1v4a1 1 0 102 0V6a1 1 0 00-1-1z" clip-rule="evenodd"/></svg>
			{error}
		</div>
	{:else if Object.keys(config).length === 0}
		<div class="empty">No configuration entries.</div>
	{:else}
		<div class="cfg-shell">
			{#each Object.entries(config) as [key, rawVal]}
				{@const t = typeOf(rawVal)}
				{@const tc = typeColor[t] ?? 'var(--text-3)'}
				<div class="cfg-row" class:cfg-err={!!saveErrors[key]}>
					<div class="cfg-meta">
						<code class="cfg-key">{key}</code>
						<span class="type-lbl" style="color:{tc}">{t}</span>
					</div>
					<div class="cfg-editor">
						<textarea
							class="cfg-ta"
							class:ta-err={!!saveErrors[key]}
							rows={Math.min(Math.max(edits[key]?.split('\n').length ?? 1, 1), 8)}
							bind:value={edits[key]}
							onkeydown={(e) => { if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) save(key); }}
							spellcheck={false}
						></textarea>
						{#if saveErrors[key]}
							<span class="fe">{saveErrors[key]}</span>
						{/if}
					</div>
					<div class="cfg-action">
						<button
							class="save-btn"
							class:save-ok={saved === key}
							onclick={() => save(key)}
							disabled={saving === key}
							title="⌘ Enter"
						>
							{#if saved === key}
								<svg viewBox="0 0 20 20" fill="currentColor" width="12" height="12">
									<path fill-rule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clip-rule="evenodd"/>
								</svg>
								Saved
							{:else if saving === key}
								<span class="mini-spin"></span>
							{:else}
								Save
							{/if}
						</button>
					</div>
				</div>
			{/each}
		</div>
	{/if}
</div>

<style>
	.p { max-width:900px; margin:0 auto; padding:40px 36px; }

	/* Swarm nodes section */
	.section { margin-bottom:36px; }
	.sec-ttl { font-size:14px; font-weight:700; color:var(--text); margin:0 0 12px; letter-spacing:-0.01em; }

	.tbl-shell { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; }
	.tbl-head {
		display:grid; grid-template-columns:1fr 90px 90px 100px 140px 90px;
		gap:12px; padding:9px 14px;
		background:var(--surface-2); border-bottom:1px solid var(--border);
		font-size:11px; font-weight:600; color:var(--text-3); text-transform:uppercase; letter-spacing:0.07em;
	}
	.tbl-row {
		display:grid; grid-template-columns:1fr 90px 90px 100px 140px 90px;
		gap:12px; padding:11px 14px; align-items:center;
		border-bottom:1px solid var(--border); font-size:12.5px;
	}
	.tbl-row:last-child { border-bottom:none; }
	.tbl-row:hover { background:var(--row-hover); }

	.node-host { display:flex; flex-direction:column; gap:2px; }
	.node-id { font-size:10px; color:var(--text-3); font-family:var(--mono); }
	.node-name { font-size:12.5px; font-weight:600; color:var(--text); }

	.badge {
		display:inline-flex; align-items:center; padding:2px 7px;
		border-radius:99px; font-size:10.5px; font-weight:600;
		text-transform:capitalize; white-space:nowrap;
	}
	.badge-mgr { background:var(--accent-soft,rgba(99,102,241,.1)); color:var(--accent); border:1px solid rgba(99,102,241,.2); }
	.badge-wkr { background:var(--surface-2); color:var(--text-2); border:1px solid var(--border); }
	.badge-ok  { background:var(--ok-soft); color:var(--ok); border:1px solid rgba(22,163,74,.2); }
	.badge-err { background:var(--danger-soft); color:var(--danger); border:1px solid rgba(220,38,38,.2); }

	.cell-muted { color:var(--text-3); font-size:12px; }
	.cell-ip { font-family:var(--mono); font-size:12px; color:var(--text); }

	/* Swarm skeleton */
	.sk-host { width:120px; height:13px; }
	.sk-role { width:60px; height:20px; border-radius:99px; }
	.sk-ip   { width:100px; height:13px; }

	.hdr { display:flex; align-items:flex-start; justify-content:space-between; gap:16px; margin-bottom:24px; }
	.ttl { font-size:18px; font-weight:700; color:var(--text); margin:0 0 4px; letter-spacing:-0.02em; }
	.sub { font-size:12.5px; color:var(--text-3); margin:0; }
	.warn-tag {
		display:flex; align-items:center; gap:5px; flex-shrink:0;
		padding:5px 10px; border-radius:var(--radius-sm);
		background:var(--warn-soft); color:var(--warn);
		border:1px solid rgba(180,83,9,0.2); font-size:11.5px; font-weight:600;
		margin-top:2px;
	}

	/* Skeleton */
	.sk { background:var(--border); border-radius:4px; animation:sk 1.3s ease-in-out infinite; }
	.sk-key { width:110px; height:13px; }
	.sk-val { flex:1; height:36px; border-radius:7px; }
	.sk-row { display:flex; align-items:center; gap:14px; padding:16px; border-bottom:1px solid var(--border); }
	.sk-row:last-child { border-bottom:none; }
	@keyframes sk { 0%,100%{opacity:.5} 50%{opacity:1} }

	.err { display:flex; align-items:center; gap:7px; padding:11px 14px; background:var(--danger-soft); border:1px solid rgba(220,38,38,0.2); border-radius:var(--radius); font-size:13px; color:var(--danger); }
	.empty { padding:48px; text-align:center; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); color:var(--text-3); font-size:13px; }

	/* Config shell */
	.cfg-shell { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:var(--shadow-sm); }
	.cfg-row {
		display:grid;
		grid-template-columns:180px 1fr auto;
		gap:12px;
		align-items:start;
		padding:15px 16px;
		border-bottom:1px solid var(--border);
		transition:background .1s;
	}
	.cfg-row:last-child { border-bottom:none; }
	.cfg-row:hover { background:var(--row-hover); }
	.cfg-row.cfg-err { background:rgba(220,38,38,0.025); }

	.cfg-meta { display:flex; flex-direction:column; gap:5px; padding-top:7px; }
	.cfg-key { font-size:12.5px; font-weight:600; color:var(--text); font-family:var(--mono); word-break:break-all; }
	.type-lbl { font-size:9.5px; font-weight:700; text-transform:uppercase; letter-spacing:0.09em; }

	.cfg-editor { display:flex; flex-direction:column; gap:4px; }
	.cfg-ta {
		width:100%; padding:7px 9px;
		font-size:12px; font-family:var(--mono);
		color:var(--text); line-height:1.6;
		background:var(--surface-2); border:1px solid var(--border);
		border-radius:var(--radius-sm); outline:none; resize:vertical;
		transition:border-color .15s, box-shadow .15s;
		box-sizing:border-box; min-height:36px;
	}
	.cfg-ta:focus { border-color:var(--accent); box-shadow:0 0 0 3px var(--accent-ring); background:var(--surface); }
	.cfg-ta.ta-err { border-color:var(--danger); }
	.cfg-ta.ta-err:focus { box-shadow:0 0 0 3px rgba(220,38,38,0.12); }
	.fe { font-size:11px; color:var(--danger); }

	.cfg-action { padding-top:1px; }
	.save-btn {
		display:inline-flex; align-items:center; gap:5px;
		padding:6px 14px; height:30px; border-radius:var(--radius-sm);
		font-size:11.5px; font-weight:600; cursor:pointer;
		border:1px solid var(--border);
		background:var(--surface-2); color:var(--text-2);
		transition:background .15s, border-color .15s, color .15s;
		white-space:nowrap; font-family:var(--font);
	}
	.save-btn:hover:not(:disabled) { background:var(--accent); border-color:var(--accent); color:#000; }
	.save-btn:disabled { opacity:.5; cursor:not-allowed; }
	.save-btn.save-ok { background:var(--ok-soft); border-color:rgba(22,163,74,0.2); color:var(--ok); }
	.save-btn.save-ok:hover { background:var(--ok-soft); color:var(--ok); }

	.mini-spin {
		display:inline-block; width:11px; height:11px;
		border:1.5px solid var(--border-2);
		border-top-color:var(--accent);
		border-radius:50%; animation:spin .6s linear infinite;
	}
	@keyframes spin { to { transform:rotate(360deg); } }
</style>
