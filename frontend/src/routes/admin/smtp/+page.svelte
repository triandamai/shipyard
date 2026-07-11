<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';

	interface SmtpSettings {
		smtp_enabled?: boolean;
		smtp_host?: string;
		smtp_port?: number;
		smtp_username?: string;
		smtp_password?: string;
		smtp_from_address?: string;
		smtp_from_name?: string;
		smtp_security?: string;
	}

	let settings        = $state<SmtpSettings>({});
	let loading         = $state(true);
	let saving          = $state(false);
	let saved           = $state(false);
	let saveError       = $state('');
	let showPassword    = $state(false);

	let testTo      = $state('');
	let testSubject = $state('Shipyard SMTP Test');
	let testBody    = $state('This is a test email sent from Shipyard to verify your SMTP configuration.');
	let testing     = $state(false);
	let testResult  = $state<{ ok: boolean; msg: string } | null>(null);
	let showTest    = $state(false);

	async function load() {
		const res = await api.get<SmtpSettings>('/settings');
		if (res.data) settings = {
			smtp_enabled:      (res.data as any).smtp_enabled,
			smtp_host:         (res.data as any).smtp_host,
			smtp_port:         (res.data as any).smtp_port,
			smtp_username:     (res.data as any).smtp_username,
			smtp_password:     (res.data as any).smtp_password,
			smtp_from_address: (res.data as any).smtp_from_address,
			smtp_from_name:    (res.data as any).smtp_from_name,
			smtp_security:     (res.data as any).smtp_security,
		};
		loading = false;
	}

	async function save(e: SubmitEvent) {
		e.preventDefault();
		saving = true; saved = false; saveError = '';
		const res = await api.put('/settings', settings);
		if (res.error) saveError = res.error.message;
		else { saved = true; setTimeout(() => (saved = false), 3000); }
		saving = false;
	}

	async function testSmtp() {
		testing = true;
		testResult = null;
		const res = await api.post<{ message: string }>('/admin/smtp/test', {
			to: testTo, subject: testSubject, body: testBody,
		});
		testResult = res.error
			? { ok: false, msg: res.error.message }
			: { ok: true, msg: res.data?.message ?? 'Test email sent' };
		testing = false;
	}

	onMount(load);
</script>

<div class="p">
	<header class="hdr">
		<div>
			<h1 class="ttl">SMTP</h1>
			<p class="sub">Platform email delivery settings.</p>
		</div>
		<button class="btn-ghost" onclick={() => (showTest = !showTest)}>
			{showTest ? 'Hide Test' : 'Send Test Email'}
		</button>
	</header>

	{#if showTest}
		<div class="card" style="margin-bottom:16px">
			<div class="card-title">Send Test Email</div>
			<div class="field">
				<label class="lbl" for="test-to">Recipient</label>
				<input id="test-to" class="inp" bind:value={testTo} placeholder="you@example.com" type="email" />
			</div>
			<div class="field">
				<label class="lbl" for="test-sub">Subject</label>
				<input id="test-sub" class="inp" bind:value={testSubject} />
			</div>
			<div class="field">
				<label class="lbl" for="test-body">Body</label>
				<textarea id="test-body" class="inp ta" bind:value={testBody} rows={3}></textarea>
			</div>
			{#if testResult}
				<div class="result-msg" class:result-ok={testResult.ok} class:result-err={!testResult.ok}>
					{testResult.msg}
				</div>
			{/if}
			<div class="form-foot">
				<button class="btn-primary" onclick={testSmtp} disabled={testing || !testTo}>
					{testing ? 'Sending…' : 'Send'}
				</button>
			</div>
		</div>
	{/if}

	{#if loading}
		<div class="card sk-wrap">
			{#each [0,1,2,3,4] as _}
				<div class="sk-row"><div class="sk sk-label"></div><div class="sk sk-input"></div></div>
			{/each}
		</div>
	{:else}
		<form class="card" onsubmit={save}>
			<div class="card-title">SMTP Settings</div>

			<div class="field toggle-row">
				<label class="lbl" for="enabled">Enable SMTP</label>
				<button
					type="button"
					id="enabled"
					class="toggle"
					class:on={settings.smtp_enabled}
					onclick={() => (settings.smtp_enabled = !settings.smtp_enabled)}
					role="switch"
					aria-checked={settings.smtp_enabled}
					aria-label="Enable SMTP"
				>
					<span class="toggle-thumb"></span>
				</button>
			</div>

			<div class="row2">
				<div class="field">
					<label class="lbl" for="host">Host</label>
					<input id="host" class="inp" bind:value={settings.smtp_host} placeholder="smtp.example.com" />
				</div>
				<div class="field">
					<label class="lbl" for="port">Port</label>
					<input id="port" class="inp" type="number" bind:value={settings.smtp_port} placeholder="587" />
				</div>
			</div>

			<div class="field">
				<label class="lbl" for="security">Security</label>
				<select id="security" class="inp sel" bind:value={settings.smtp_security}>
					<option value="tls">TLS</option>
					<option value="starttls">STARTTLS</option>
					<option value="none">None</option>
				</select>
			</div>

			<div class="row2">
				<div class="field">
					<label class="lbl" for="user">Username</label>
					<input id="user" class="inp" bind:value={settings.smtp_username} autocomplete="off" />
				</div>
				<div class="field">
					<label class="lbl" for="pass">Password</label>
					<div class="pass-wrap">
						<input
							id="pass" class="inp" autocomplete="new-password"
							type={showPassword ? 'text' : 'password'}
							bind:value={settings.smtp_password}
						/>
						<button type="button" class="eye-btn" onclick={() => (showPassword = !showPassword)}>
							{showPassword ? 'Hide' : 'Show'}
						</button>
					</div>
				</div>
			</div>

			<div class="row2">
				<div class="field">
					<label class="lbl" for="from-addr">From Address</label>
					<input id="from-addr" class="inp" bind:value={settings.smtp_from_address} placeholder="noreply@example.com" type="email" />
				</div>
				<div class="field">
					<label class="lbl" for="from-name">From Name</label>
					<input id="from-name" class="inp" bind:value={settings.smtp_from_name} placeholder="Shipyard" />
				</div>
			</div>

			{#if saveError}
				<div class="err-msg">{saveError}</div>
			{/if}
			<div class="form-foot">
				<button type="submit" class="btn-primary" disabled={saving}>
					{#if saved}Saved{:else if saving}Saving…{:else}Save Changes{/if}
				</button>
			</div>
		</form>
	{/if}
</div>

<style>
	.p { max-width:680px; margin:0 auto; padding:40px 36px; }
	.hdr { display:flex; align-items:flex-start; justify-content:space-between; gap:12px; margin-bottom:20px; }
	.ttl { font-size:18px; font-weight:700; color:var(--text); margin:0 0 4px; letter-spacing:-0.02em; }
	.sub { font-size:12.5px; color:var(--text-3); margin:0; }

	.card { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); padding:24px; box-shadow:0 1px 2px rgba(0,0,0,.07); }
	.card-title { font-size:13px; font-weight:700; color:var(--text); margin-bottom:16px; }

	.sk-wrap { display:flex; flex-direction:column; gap:16px; }
	.sk { background:var(--border); border-radius:4px; animation:sk 1.3s ease-in-out infinite; }
	.sk-label { width:90px; height:11px; }
	.sk-input { height:34px; border-radius:var(--radius-sm); flex:1; }
	.sk-row { display:flex; align-items:center; gap:12px; }
	@keyframes sk { 0%,100%{opacity:.5} 50%{opacity:1} }

	.field { display:flex; flex-direction:column; gap:5px; margin-bottom:14px; }
	.field:last-of-type { margin-bottom:0; }
	.row2 { display:grid; grid-template-columns:1fr 1fr; gap:12px; }
	.toggle-row { flex-direction:row; align-items:center; justify-content:space-between; }
	.lbl { font-size:11.5px; font-weight:600; color:var(--text-2); }
	.inp { height:34px; padding:0 10px; background:var(--surface-2); border:1px solid var(--border); border-radius:var(--radius-sm); font-size:12.5px; color:var(--text); outline:none; width:100%; box-sizing:border-box; font-family:var(--font); transition:border-color .15s, box-shadow .15s; }
	.inp:focus { border-color:var(--accent); box-shadow:0 0 0 3px var(--accent-ring); }
	.ta { height:auto; resize:vertical; padding:8px 10px; line-height:1.5; }
	.sel { cursor:pointer; }

	.pass-wrap { position:relative; display:flex; align-items:center; }
	.pass-wrap .inp { padding-right:52px; }
	.eye-btn { position:absolute; right:8px; font-size:11px; font-weight:600; color:var(--text-3); background:none; border:none; cursor:pointer; padding:4px; font-family:var(--font); }
	.eye-btn:hover { color:var(--accent); }

	.toggle { width:38px; height:22px; border-radius:999px; border:none; cursor:pointer; background:var(--border-2); position:relative; transition:background .2s; padding:0; }
	.toggle.on { background:var(--accent); }
	.toggle-thumb { position:absolute; top:3px; left:3px; width:16px; height:16px; border-radius:50%; background:#fff; transition:transform .2s; }
	.toggle.on .toggle-thumb { transform:translateX(16px); }

	.err-msg { padding:8px 12px; background:var(--danger-soft); border:1px solid rgba(220,38,38,0.2); border-radius:var(--radius-sm); font-size:12px; color:var(--danger); margin-top:10px; }
	.result-msg { padding:8px 12px; border-radius:var(--radius-sm); font-size:12px; margin-top:10px; }
	.result-ok { background:var(--ok-soft); border:1px solid rgba(22,163,74,0.2); color:var(--ok); }
	.result-err { background:var(--danger-soft); border:1px solid rgba(220,38,38,0.2); color:var(--danger); }

	.form-foot { display:flex; justify-content:flex-end; margin-top:20px; padding-top:16px; border-top:1px solid var(--border); }
	.btn-primary { padding:7px 18px; height:34px; border-radius:var(--radius-sm); font-size:12.5px; font-weight:600; cursor:pointer; border:1px solid var(--accent); background:var(--accent); color:#000; transition:opacity .15s; font-family:var(--font); }
	.btn-primary:hover:not(:disabled) { opacity:.88; }
	.btn-primary:disabled { opacity:.5; cursor:not-allowed; }
	.btn-ghost { padding:6px 14px; height:32px; border-radius:var(--radius-sm); font-size:12px; font-weight:500; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); transition:background .15s; font-family:var(--font); white-space:nowrap; }
	.btn-ghost:hover { background:var(--surface-2); }
</style>
