<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { can, perm } from '$lib/auth/permissions';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';
	import SlidePanel from '$lib/components/SlidePanel.svelte';
	import { Mail, Save, Check, Eye, EyeOff, Send } from '@lucide/svelte';
	import { Card, FormField, TextField, Textarea, Select, Toggle, Button, InlineAlert, Spinner } from '$lib/components/ui';

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

	// Text-field components bind `string`; the port is held as string | number
	// (TextField type="number" yields a number once edited) and coerced on save.
	interface SmtpSettings {
		smtp_enabled: boolean;
		smtp_host: string;
		smtp_port: string | number;
		smtp_username: string;
		smtp_password: string;
		smtp_from_address: string;
		smtp_from_name: string;
		smtp_security: string;
	}

	let settings    = $state<SmtpSettings>({
		smtp_enabled: false,
		smtp_host: '',
		smtp_port: '',
		smtp_username: '',
		smtp_password: '',
		smtp_from_address: '',
		smtp_from_name: '',
		smtp_security: 'starttls'
	});
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
		try {
			const res = await api.post<{ message: string }>(`/admin/smtp/test?org_id=${orgId}`, {
				to: smtpTestTo,
				subject: smtpTestSubject,
				body: smtpTestBody,
			});
			smtpTestResult = res.error
				? { ok: false, msg: res.error.message }
				: { ok: true, msg: res.data?.message ?? 'Test email sent' };
		} finally { smtpTesting = false; }
	}

	async function save(e: SubmitEvent) {
		e.preventDefault();
		if (!canSmtpWrite || !orgId) return;
		saving = true; saved = false; saveError = '';
		try {
			const payload = {
				...settings,
				smtp_port: settings.smtp_port === '' || settings.smtp_port == null ? undefined : Number(settings.smtp_port),
			};
			const res = await api.put<SmtpSettings>(`/settings/smtp?org_id=${orgId}`, payload);
			if (res.error) saveError = res.error.message;
			else { saved = true; setTimeout(() => (saved = false), 3000); }
		} finally { saving = false; }
	}

	onMount(async () => {
		if (!canSmtpAny) { loading = false; return; }
		const res = await api.get<Partial<SmtpSettings>>('/settings');
		if (res.data) settings = {
			smtp_enabled:      res.data.smtp_enabled as boolean,
			smtp_host:         res.data.smtp_host as string,
			smtp_port:         res.data.smtp_port as string | number,
			smtp_username:     res.data.smtp_username as string,
			smtp_password:     res.data.smtp_password as string,
			smtp_from_address: res.data.smtp_from_address as string,
			smtp_from_name:    res.data.smtp_from_name as string,
			smtp_security:     res.data.smtp_security as string,
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
			<Spinner size={18} />
			<span>Loading SMTP settings…</span>
		</div>
	{:else}
		<form class="smtp-form" onsubmit={save}>
			<Card padding="0">
				<div class="section-header">
					<div class="section-icon"><Mail size={16} /></div>
					<div>
						<h2 class="section-title">Email (SMTP)</h2>
						<p class="section-desc">Configure outgoing email for invitation links and notifications.</p>
					</div>
				</div>

				<div class="smtp-toggle-row">
					<Toggle bind:checked={settings.smtp_enabled} disabled={!canSmtpWrite} label="Enable SMTP" />
					<span class="toggle-text">{settings.smtp_enabled ? 'Enabled' : 'Disabled'}</span>
				</div>

				{#if settings.smtp_enabled}
					<div class="fields-grid">
						<FormField label="SMTP Host" for="smtp-host">
							<TextField id="smtp-host" type="text" bind:value={settings.smtp_host} placeholder="smtp.example.com" disabled={!canSmtpWrite} />
						</FormField>
						<FormField label="Port" for="smtp-port">
							<TextField id="smtp-port" type="number" bind:value={settings.smtp_port as string} placeholder="587" min="1" max="65535" disabled={!canSmtpWrite} />
						</FormField>
						<FormField label="Security" for="smtp-security">
							<Select
								id="smtp-security"
								bind:value={settings.smtp_security}
								disabled={!canSmtpWrite}
								options={[
									{ value: 'starttls', label: 'STARTTLS (port 587)' },
									{ value: 'tls', label: 'Implicit TLS (port 465)' },
									{ value: 'none', label: 'None (port 25)' }
								]}
							/>
						</FormField>
						<FormField label="Username" for="smtp-user">
							<TextField id="smtp-user" type="text" bind:value={settings.smtp_username} placeholder="user@example.com" autocomplete="off" disabled={!canSmtpWrite} />
						</FormField>
						<FormField label="Password" for="smtp-pass">
							<div class="password-row">
								<TextField
									id="smtp-pass"
									type={showSmtpPassword ? 'text' : 'password'}
									bind:value={settings.smtp_password}
									placeholder="••••••••"
									autocomplete="new-password"
									disabled={!canSmtpWrite}
								/>
								<Button variant="secondary" size="icon" aria-label={showSmtpPassword ? 'Hide password' : 'Show password'} onclick={() => (showSmtpPassword = !showSmtpPassword)}>
									{#if showSmtpPassword}<EyeOff size={14} />{:else}<Eye size={14} />{/if}
								</Button>
							</div>
						</FormField>
						<FormField label="From Address" for="smtp-from">
							<TextField id="smtp-from" type="email" bind:value={settings.smtp_from_address} placeholder="noreply@example.com" disabled={!canSmtpWrite} />
						</FormField>
						<FormField label="From Name" for="smtp-name">
							<TextField id="smtp-name" type="text" bind:value={settings.smtp_from_name} placeholder="Shipyard" disabled={!canSmtpWrite} />
						</FormField>
					</div>

					{#if canSmtpWrite}
						<div class="smtp-test-bar">
							<Button variant="secondary" size="sm" onclick={() => { smtpTestPanelOpen = true; smtpTestResult = null; }}>
								<Mail size={12} />Send test email
							</Button>
						</div>
					{/if}
				{/if}
			</Card>

			{#if saveError}
				<div role="alert"><InlineAlert tone="error">{saveError}</InlineAlert></div>
			{/if}

			{#if canSmtpWrite}
				<div class="save-bar">
					<Button type="submit" disabled={saving}>
						{#if saving}<Spinner size={12} tone="current" />Saving…
						{:else if saved}<Check size={14} />Saved
						{:else}<Save size={14} />Save Settings
						{/if}
					</Button>
				</div>
			{/if}
		</form>

		{#if smtpTestPanelOpen}
			<div class="panel-backdrop" onclick={() => (smtpTestPanelOpen = false)} role="none"></div>
			<SlidePanel title="Send Test Email" onClose={() => (smtpTestPanelOpen = false)}>
				<div class="smtp-panel-body">
					<div class="smtp-panel-fields">
						<FormField label="To" for="test-to">
							<TextField id="test-to" type="email" bind:value={smtpTestTo} placeholder="you@example.com" />
						</FormField>
						<FormField label="Subject" for="test-subject">
							<TextField id="test-subject" type="text" bind:value={smtpTestSubject} />
						</FormField>
						<FormField label="Body" for="test-body">
							<Textarea id="test-body" bind:value={smtpTestBody} rows={5} />
						</FormField>
					</div>

					{#if smtpTestResult}
						<div role="alert"><InlineAlert tone={smtpTestResult.ok ? 'success' : 'error'}>{smtpTestResult.msg}</InlineAlert></div>
					{/if}

					<div class="smtp-panel-actions">
						<Button variant="secondary" onclick={() => (smtpTestPanelOpen = false)}>Cancel</Button>
						<Button disabled={smtpTesting || !smtpTestTo} onclick={testSmtp}>
							{#if smtpTesting}<Spinner size={13} tone="current" />Sending…{:else}<Send size={13} />Send{/if}
						</Button>
					</div>
				</div>
			</SlidePanel>
		{/if}
	{/if}
{/if}

<style>
	.loading { display: flex; align-items: center; gap: 10px; color: var(--text-muted); font-size: 13px; padding: 40px 0; }

	.smtp-form { display: flex; flex-direction: column; gap: 20px; }

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
		background: var(--accent-green-muted);
		color: var(--accent-green);
		display: flex; align-items: center; justify-content: center;
		flex-shrink: 0;
		margin-top: 1px;
	}
	.section-title { font-size: 14px; font-weight: 600; color: var(--text-primary); margin: 0 0 3px; }
	.section-desc  { font-size: 12px; color: var(--text-muted); margin: 0; line-height: 1.5; }

	.smtp-toggle-row { display: flex; align-items: center; gap: 10px; padding: 14px 20px; border-bottom: 1px solid var(--border); }
	.toggle-text { font-size: 13px; color: var(--text-secondary); }

	.fields-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; padding: 18px 20px; }
	.password-row { display: flex; align-items: center; gap: 4px; }
	.password-row :global(.ui-textfield) { flex: 1; }

	.smtp-test-bar { display: flex; align-items: center; gap: 10px; padding: 12px 20px; border-top: 1px solid var(--border); }

	.save-bar { display: flex; justify-content: flex-end; padding: 4px 0 8px; }

	.panel-backdrop { position: fixed; inset: 0; background: rgba(0,0,0,0.35); z-index: 59; }
	.smtp-panel-body { display: flex; flex-direction: column; gap: 16px; padding: 16px; height: 100%; }
	.smtp-panel-fields { display: flex; flex-direction: column; gap: 14px; }
	.smtp-panel-actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: auto; padding-top: 8px; border-top: 1px solid var(--border); }

	@media (max-width: 639px) {
		.fields-grid { grid-template-columns: 1fr; padding: 14px 16px; }
	}
</style>
