<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { can, perm } from '$lib/auth/permissions';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';
	import SlidePanel from '$lib/components/SlidePanel.svelte';
	import { Mail, Save, Check, Loader2, Eye, EyeOff, Send, AlertCircle } from '@lucide/svelte';

	let orgId    = $derived($orgStore.activeOrg?.id ?? '');
	let myRole   = $derived($orgStore.myMembership?.role ?? null);
	let myPerms  = $derived($orgStore.myMembership?.permissions ?? []);
	let membershipLoaded = $derived($orgStore.membershipLoaded);
	let canSmtpRead  = $derived(
		can(myRole, myPerms, perm(orgId, 'smtp', 'read')) ||
		can(myRole, myPerms, perm(orgId, 'settings', 'read'))
	);
	let canSmtpWrite = $derived(can(myRole, myPerms, perm(orgId, 'smtp', 'write')));
	let canSmtpAny   = $derived(canSmtpRead || canSmtpWrite);

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

	let settings    = $state<SmtpSettings>({});
	let loading     = $state(true);
	let saving      = $state(false);
	let saved       = $state(false);
	let saveError   = $state('');

	let showSmtpPassword = $state(false);

	let smtpTestPanelOpen = $state(false);
	let smtpTestTo      = $state('');
	let smtpTestSubject = $state('Shipyard SMTP Test');
	let smtpTestBody    = $state('This is a test email sent from Shipyard to verify your SMTP configuration.');
	let smtpTesting     = $state(false);
	let smtpTestResult  = $state<{ ok: boolean; msg: string } | null>(null);

	async function testSmtp() {
		smtpTesting = true;
		smtpTestResult = null;
		const res = await api.post<{ message: string }>(`/admin/smtp/test?org_id=${orgId}`, {
			to: smtpTestTo,
			subject: smtpTestSubject,
			body: smtpTestBody,
		});
		smtpTestResult = res.error
			? { ok: false, msg: res.error.message }
			: { ok: true, msg: res.data?.message ?? 'Test email sent' };
		smtpTesting = false;
	}

	async function save(e: SubmitEvent) {
		e.preventDefault();
		if (!canSmtpWrite || !orgId) return;
		saving = true; saved = false; saveError = '';
		try {
			const res = await api.put<SmtpSettings>(`/settings/smtp?org_id=${orgId}`, settings);
			if (res.error) saveError = res.error.message;
			else { saved = true; setTimeout(() => (saved = false), 3000); }
		} finally { saving = false; }
	}

	onMount(async () => {
		if (!canSmtpAny) { loading = false; return; }
		const res = await api.get<SmtpSettings>('/settings');
		if (res.data) settings = {
			smtp_enabled:      res.data.smtp_enabled,
			smtp_host:         res.data.smtp_host,
			smtp_port:         res.data.smtp_port,
			smtp_username:     res.data.smtp_username,
			smtp_password:     res.data.smtp_password,
			smtp_from_address: res.data.smtp_from_address,
			smtp_from_name:    res.data.smtp_from_name,
			smtp_security:     res.data.smtp_security,
		};
		loading = false;
	});
</script>

<PermissionDeniedDialog
	open={membershipLoaded && !!orgId && !canSmtpAny}
	message="You need the 'View SMTP config' permission to access this page."
	onDismiss={() => history.back()}
	onBack={() => history.back()}
/>

{#if canSmtpAny}
	{#if loading}
		<div class="loading">
			<div class="spinner"></div>
			<span>Loading SMTP settings…</span>
		</div>
	{:else}
		<form class="smtp-form" onsubmit={save}>
			<section class="settings-section">
				<div class="section-header">
					<div class="section-icon"><Mail size={16} /></div>
					<div>
						<h2 class="section-title">Email (SMTP)</h2>
						<p class="section-desc">Configure outgoing email for invitation links and notifications.</p>
					</div>
				</div>

				<div class="smtp-toggle-row">
					<label class="toggle-label">
						<input type="checkbox" bind:checked={settings.smtp_enabled} disabled={!canSmtpWrite} />
						<span class="toggle-track"></span>
						<span class="toggle-text">{settings.smtp_enabled ? 'Enabled' : 'Disabled'}</span>
					</label>
				</div>

				{#if settings.smtp_enabled}
					<div class="fields-grid">
						<div class="field">
							<label class="field-label" for="smtp-host">SMTP Host</label>
							<input id="smtp-host" class="field-input font-mono" type="text" bind:value={settings.smtp_host} placeholder="smtp.example.com" disabled={!canSmtpWrite} />
						</div>
						<div class="field">
							<label class="field-label" for="smtp-port">Port</label>
							<input id="smtp-port" class="field-input font-mono" type="number" bind:value={settings.smtp_port} placeholder="587" min="1" max="65535" disabled={!canSmtpWrite} />
						</div>
						<div class="field">
							<label class="field-label" for="smtp-security">Security</label>
							<select id="smtp-security" class="field-input" bind:value={settings.smtp_security} disabled={!canSmtpWrite}>
								<option value="starttls">STARTTLS (port 587)</option>
								<option value="tls">Implicit TLS (port 465)</option>
								<option value="none">None (port 25)</option>
							</select>
						</div>
						<div class="field">
							<label class="field-label" for="smtp-user">Username</label>
							<input id="smtp-user" class="field-input" type="text" bind:value={settings.smtp_username} placeholder="user@example.com" autocomplete="off" disabled={!canSmtpWrite} />
						</div>
						<div class="field">
							<label class="field-label" for="smtp-pass">Password</label>
							<div class="password-row">
								<input
									id="smtp-pass"
									class="field-input"
									type={showSmtpPassword ? 'text' : 'password'}
									bind:value={settings.smtp_password}
									placeholder="••••••••"
									autocomplete="new-password"
									disabled={!canSmtpWrite}
								/>
								<button type="button" class="eye-btn" onclick={() => (showSmtpPassword = !showSmtpPassword)}>
									{#if showSmtpPassword}<EyeOff size={14} />{:else}<Eye size={14} />{/if}
								</button>
							</div>
						</div>
						<div class="field">
							<label class="field-label" for="smtp-from">From Address</label>
							<input id="smtp-from" class="field-input" type="email" bind:value={settings.smtp_from_address} placeholder="noreply@example.com" disabled={!canSmtpWrite} />
						</div>
						<div class="field">
							<label class="field-label" for="smtp-name">From Name</label>
							<input id="smtp-name" class="field-input" type="text" bind:value={settings.smtp_from_name} placeholder="Shipyard" disabled={!canSmtpWrite} />
						</div>
					</div>

					{#if canSmtpWrite}
						<div class="smtp-test-bar">
							<button type="button" class="smtp-test-btn" onclick={() => { smtpTestPanelOpen = true; smtpTestResult = null; }}>
								<Mail size={12} />Send test email
							</button>
						</div>
					{/if}
				{/if}
			</section>

			{#if saveError}
				<div class="error-banner"><AlertCircle size={14} />{saveError}</div>
			{/if}

			{#if canSmtpWrite}
				<div class="save-bar">
					<button class="btn btn-primary save-btn" type="submit" disabled={saving}>
						{#if saving}<div class="btn-spinner"></div>Saving…
						{:else if saved}<Check size={14} />Saved
						{:else}<Save size={14} />Save Settings
						{/if}
					</button>
				</div>
			{/if}
		</form>

		{#if smtpTestPanelOpen}
			<div class="panel-backdrop" onclick={() => (smtpTestPanelOpen = false)} role="none"></div>
			<SlidePanel title="Send Test Email" onClose={() => (smtpTestPanelOpen = false)}>
				<div class="smtp-panel-body">
					<div class="smtp-panel-fields">
						<div class="field">
							<label class="field-label" for="test-to">To</label>
							<input id="test-to" class="field-input" type="email" bind:value={smtpTestTo} placeholder="you@example.com" />
						</div>
						<div class="field">
							<label class="field-label" for="test-subject">Subject</label>
							<input id="test-subject" class="field-input" type="text" bind:value={smtpTestSubject} />
						</div>
						<div class="field">
							<label class="field-label" for="test-body">Body</label>
							<textarea id="test-body" class="field-input smtp-panel-textarea" bind:value={smtpTestBody} rows={5}></textarea>
						</div>
					</div>

					{#if smtpTestResult}
						<div class="smtp-panel-result" class:ok={smtpTestResult.ok} class:fail={!smtpTestResult.ok}>
							{smtpTestResult.ok ? '✓' : '✗'} {smtpTestResult.msg}
						</div>
					{/if}

					<div class="smtp-panel-actions">
						<button class="btn btn-secondary" onclick={() => (smtpTestPanelOpen = false)}>Cancel</button>
						<button
							class="btn btn-primary"
							disabled={smtpTesting || !smtpTestTo}
							onclick={testSmtp}
						>
							{#if smtpTesting}<Loader2 size={13} class="spin" />Sending…{:else}<Send size={13} />Send{/if}
						</button>
					</div>
				</div>
			</SlidePanel>
		{/if}
	{/if}
{/if}

<style>
	:global(.spin) { animation: spin 0.8s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }

	.loading { display: flex; align-items: center; gap: 10px; color: var(--text-muted); font-size: 13px; padding: 40px 0; }
	.spinner { width: 18px; height: 18px; border: 2px solid var(--border); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.7s linear infinite; }

	.smtp-form { display: flex; flex-direction: column; gap: 20px; }

	.settings-section {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		overflow: hidden;
	}

	.section-header {
		display: flex;
		gap: 14px;
		padding: 18px 20px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
	}

	.section-icon {
		width: 32px; height: 32px;
		border-radius: var(--radius-md);
		background: rgba(16, 185, 129, 0.1);
		color: #10B981;
		display: flex; align-items: center; justify-content: center;
		flex-shrink: 0;
		margin-top: 1px;
	}

	.section-title { font-size: 14px; font-weight: 600; color: var(--text-primary); margin: 0 0 3px; }
	.section-desc  { font-size: 12px; color: var(--text-muted); margin: 0; line-height: 1.5; }

	.smtp-toggle-row { display: flex; align-items: center; padding: 14px 20px; border-bottom: 1px solid var(--border); }
	.toggle-label { display: flex; align-items: center; gap: 10px; cursor: pointer; user-select: none; }
	.toggle-label input[type="checkbox"] { display: none; }
	.toggle-track {
		width: 34px; height: 18px;
		background: var(--border);
		border-radius: 99px;
		position: relative;
		transition: background 0.2s;
		flex-shrink: 0;
	}
	.toggle-track::after {
		content: '';
		position: absolute;
		width: 13px; height: 13px;
		background: white;
		border-radius: 50%;
		top: 2.5px; left: 2.5px;
		transition: left 0.2s;
		box-shadow: 0 1px 3px rgba(0,0,0,0.2);
	}
	.toggle-label input:checked ~ .toggle-track { background: #10B981; }
	.toggle-label input:checked ~ .toggle-track::after { left: 18.5px; }
	.toggle-text { font-size: 13px; color: var(--text-secondary); }

	.fields-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; padding: 18px 20px; }
	.field { display: flex; flex-direction: column; gap: 5px; }
	.field-label { font-size: 11px; font-weight: 600; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.06em; }
	.field-input {
		background: var(--bg-base);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-primary);
		font-size: 13px;
		font-family: var(--font-sans);
		padding: 8px 10px;
		outline: none;
		transition: border-color var(--transition-fast);
	}
	.field-input.font-mono { font-family: var(--font-mono); }
	.field-input:focus { border-color: var(--accent); }
	.field-input:disabled { opacity: 0.6; cursor: not-allowed; }

	.password-row { display: flex; align-items: center; gap: 4px; }
	.password-row .field-input { flex: 1; }
	.eye-btn { background: var(--bg-base); border: 1px solid var(--border); border-radius: var(--radius-sm); color: var(--text-dim); padding: 8px 9px; cursor: pointer; display: flex; align-items: center; transition: color var(--transition-fast); flex-shrink: 0; }
	.eye-btn:hover { color: var(--text-primary); }

	.smtp-test-bar { display: flex; align-items: center; gap: 10px; padding: 12px 20px; border-top: 1px solid var(--border); }
	.smtp-test-btn { display: inline-flex; align-items: center; gap: 6px; padding: 6px 14px; background: transparent; border: 1px solid var(--border); border-radius: var(--radius-sm); color: var(--text-muted); font-size: 12px; font-weight: 500; font-family: var(--font-sans); cursor: pointer; transition: all var(--transition-fast); }
	.smtp-test-btn:hover:not(:disabled) { border-color: #10B981; color: #10B981; }

	.error-banner { display: flex; align-items: center; gap: 8px; padding: 10px 14px; background: rgba(239,68,68,0.08); border: 1px solid rgba(239,68,68,0.25); border-radius: var(--radius-md); color: #EF4444; font-size: 13px; }

	.save-bar { display: flex; justify-content: flex-end; padding: 4px 0 8px; }
	.save-btn { display: flex; align-items: center; gap: 6px; min-width: 140px; justify-content: center; }
	.btn-spinner { width: 12px; height: 12px; border: 2px solid rgba(255,255,255,0.3); border-top-color: white; border-radius: 50%; animation: spin 0.7s linear infinite; }

	.panel-backdrop { position: fixed; inset: 0; background: rgba(0,0,0,0.35); z-index: 59; }
	.smtp-panel-body { display: flex; flex-direction: column; gap: 16px; padding: 16px; height: 100%; }
	.smtp-panel-fields { display: flex; flex-direction: column; gap: 14px; }
	.smtp-panel-textarea { resize: vertical; min-height: 100px; font-family: var(--font-sans); line-height: 1.5; }
	.smtp-panel-actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: auto; padding-top: 8px; border-top: 1px solid var(--border); }
	.smtp-panel-result { font-size: 12px; font-weight: 500; padding: 8px 12px; border-radius: var(--radius-sm); }
	.smtp-panel-result.ok   { background: rgba(16,185,129,0.1); color: #10B981; border: 1px solid rgba(16,185,129,0.25); }
	.smtp-panel-result.fail { background: rgba(239,68,68,0.08); color: #EF4444;  border: 1px solid rgba(239,68,68,0.25); }

	@media (max-width: 639px) {
		.fields-grid { grid-template-columns: 1fr; padding: 14px 16px; }
	}
</style>
