<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { PageHeader, Card, FormField, TextField, Textarea, Select, Toggle, Button, InlineAlert, Skeleton } from '$lib/components/ui';

	interface SmtpSettings {
		smtp_enabled: boolean;
		smtp_host: string;
		smtp_port: string;
		smtp_username: string;
		smtp_password: string;
		smtp_from_address: string;
		smtp_from_name: string;
		smtp_security: string;
	}

	let settings        = $state<SmtpSettings>({
		smtp_enabled: false,
		smtp_host: '',
		smtp_port: '',
		smtp_username: '',
		smtp_password: '',
		smtp_from_address: '',
		smtp_from_name: '',
		smtp_security: 'tls'
	});
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
		const payload = {
			...settings,
			smtp_port: settings.smtp_port === '' ? undefined : Number(settings.smtp_port),
		};
		const res = await api.put('/settings', payload);
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

<div class="smtp-page">
	<PageHeader title="SMTP" subtitle="Platform email delivery settings.">
		{#snippet actions()}
			<Button variant="ghost" size="sm" onclick={() => (showTest = !showTest)}>
				{showTest ? 'Hide Test' : 'Send Test Email'}
			</Button>
		{/snippet}
	</PageHeader>

	{#if showTest}
		<Card padding="18px">
			<div class="smtp-card-title">Send Test Email</div>
			<FormField label="Recipient" for="test-to">
				<TextField id="test-to" bind:value={testTo} placeholder="you@example.com" type="email" />
			</FormField>
			<FormField label="Subject" for="test-sub">
				<TextField id="test-sub" bind:value={testSubject} />
			</FormField>
			<FormField label="Body" for="test-body">
				<Textarea id="test-body" bind:value={testBody} rows={3} />
			</FormField>
			{#if testResult}
				<InlineAlert tone={testResult.ok ? 'success' : 'error'}>{testResult.msg}</InlineAlert>
			{/if}
			<div class="smtp-form-foot">
				<Button disabled={testing || !testTo} onclick={testSmtp}>
					{testing ? 'Sending…' : 'Send'}
				</Button>
			</div>
		</Card>
	{/if}

	{#if loading}
		<Card padding="18px">
			<div class="smtp-sk-wrap">
				{#each Array(5) as _}<Skeleton variant="row" height="34px" />{/each}
			</div>
		</Card>
	{:else}
		<Card padding="18px">
			<form onsubmit={save}>
				<div class="smtp-card-title">SMTP Settings</div>

				<div class="smtp-toggle-row">
					<span class="smtp-toggle-label">Enable SMTP</span>
					<Toggle bind:checked={settings.smtp_enabled} label="Enable SMTP" />
				</div>

				<div class="smtp-row2">
					<FormField label="Host" for="host">
						<TextField id="host" bind:value={settings.smtp_host} placeholder="smtp.example.com" />
					</FormField>
					<FormField label="Port" for="port">
						<TextField id="port" type="number" bind:value={settings.smtp_port} placeholder="587" />
					</FormField>
				</div>

				<FormField label="Security" for="security">
					<Select
						id="security"
						bind:value={settings.smtp_security}
						options={[
							{ value: 'tls', label: 'TLS' },
							{ value: 'starttls', label: 'STARTTLS' },
							{ value: 'none', label: 'None' }
						]}
					/>
				</FormField>

				<div class="smtp-row2">
					<FormField label="Username" for="user">
						<TextField id="user" bind:value={settings.smtp_username} />
					</FormField>
					<FormField label="Password" for="pass">
						<div class="smtp-pass-wrap">
							<TextField id="pass" type={showPassword ? 'text' : 'password'} bind:value={settings.smtp_password} />
							<button type="button" class="smtp-eye-btn" onclick={() => (showPassword = !showPassword)}>
								{showPassword ? 'Hide' : 'Show'}
							</button>
						</div>
					</FormField>
				</div>

				<div class="smtp-row2">
					<FormField label="From Address" for="from-addr">
						<TextField id="from-addr" type="email" bind:value={settings.smtp_from_address} placeholder="noreply@example.com" />
					</FormField>
					<FormField label="From Name" for="from-name">
						<TextField id="from-name" bind:value={settings.smtp_from_name} placeholder="Shipyard" />
					</FormField>
				</div>

				{#if saveError}<InlineAlert tone="error">{saveError}</InlineAlert>{/if}
				<div class="smtp-form-foot">
					<Button type="submit" disabled={saving}>
						{#if saved}Saved{:else if saving}Saving…{:else}Save Changes{/if}
					</Button>
				</div>
			</form>
		</Card>
	{/if}
</div>

<style>
	.smtp-page { max-width: 640px; }
	.smtp-card-title { font-size: 13px; font-weight: 700; color: var(--text-primary); margin-bottom: 16px; }
	.smtp-toggle-row { display: flex; align-items: center; justify-content: space-between; margin-bottom: 14px; }
	.smtp-toggle-label { font-size: 11.5px; font-weight: 600; color: var(--text-secondary); }
	.smtp-row2 { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; margin-bottom: 14px; }
	.smtp-pass-wrap { position: relative; }
	.smtp-eye-btn {
		position: absolute; right: 8px; top: 50%; transform: translateY(-50%);
		font-size: 11px; font-weight: 600; color: var(--text-dim);
		background: none; border: none; cursor: pointer; padding: 4px;
	}
	.smtp-eye-btn:hover { color: var(--accent); }
	.smtp-form-foot { display: flex; justify-content: flex-end; margin-top: 18px; padding-top: 16px; border-top: 1px solid var(--border); }
	.smtp-sk-wrap { display: flex; flex-direction: column; gap: 14px; }
</style>
