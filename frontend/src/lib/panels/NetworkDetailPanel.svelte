<script lang="ts">
	import { onMount } from 'svelte';
	import { uiStore } from '$lib/stores/ui.store';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { can, permProject } from '$lib/auth/permissions';
	import { Network, Trash2, AlertTriangle, Copy, Check } from '@lucide/svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import FormField from '$lib/components/ui/FormField.svelte';
	import InlineAlert from '$lib/components/ui/InlineAlert.svelte';
	import KeyValueList from '$lib/components/ui/KeyValueList.svelte';
	import ListRow from '$lib/components/ui/ListRow.svelte';
	import Spinner from '$lib/components/ui/Spinner.svelte';
	import TextField from '$lib/components/ui/TextField.svelte';

	interface Props {
		networkId: string;
		projectId: string;
		onDeleted?: () => void;
	}

	let { networkId, projectId, onDeleted }: Props = $props();

	let orgId = $derived($orgStore.activeOrg?.id ?? '');
	let canNetworkWrite = $derived(can($orgStore.myMembership?.role ?? null, $orgStore.myMembership?.permissions ?? [], permProject(orgId, projectId, 'network', 'write')));

	interface NetworkDetail {
		id: string;
		project_id: string;
		name: string;
		driver: string;
		subnet: string;
		docker_network_id?: string;
		created_at: string;
	}

	let network = $state<NetworkDetail | null>(null);
	let loading = $state(true);
	let loadError = $state('');

	// Danger zone state
	let confirmName = $state('');
	let deleting = $state(false);
	let deleteError = $state('');

	let canDelete = $derived(confirmName.trim() === (network?.name ?? ''));

	// Copy-to-clipboard state
	let copied = $state('');
	function copyText(text: string, key: string) {
		navigator.clipboard.writeText(text);
		copied = key;
		setTimeout(() => (copied = ''), 2000);
	}

	onMount(async () => {
		loading = true;
		const res = await api.get<NetworkDetail>(`/projects/${projectId}/networks/${networkId}`);
		if (res.error) {
			loadError = res.error.message;
		} else if (res.data) {
			network = res.data;
		}
		loading = false;
	});

	async function deleteNetwork() {
		if (!canDelete) return;
		deleting = true;
		deleteError = '';
		const res = await api.delete(`/projects/${projectId}/networks/${networkId}`);
		deleting = false;
		if (res.error) {
			deleteError = res.error.message;
			return;
		}
		onDeleted?.();
		uiStore.popPanel();
	}

	function fmtDate(iso: string) {
		return new Date(iso).toLocaleString();
	}
</script>

<div class="panel-wrap">
	{#if loading}
		<div class="center-state">
			<Spinner size={18} />
			<span>Loading…</span>
		</div>
	{:else if loadError}
		<div class="center-state error">{loadError}</div>
	{:else if network}
		<!-- Header -->
		<Card padding="12px 16px">
			<ListRow title={network.name} meta={network.driver}>
				{#snippet icon()}<Network size={18} />{/snippet}
			</ListRow>
		</Card>

		<!-- Properties -->
		<section class="detail-section">
			<h3 class="section-title">Properties</h3>
			<KeyValueList
				keyWidth="90px"
				items={[
					{ key: 'Name', value: network.name, mono: true },
					{ key: 'Driver', value: network.driver },
					...(network.subnet ? [{ key: 'Subnet', value: network.subnet, mono: true }] : []),
					...(network.docker_network_id ? [{ key: 'Docker ID', value: network.docker_network_id.slice(0, 12), mono: true }] : []),
					{ key: 'Created', value: fmtDate(network.created_at) }
				]}
			>
				{#snippet value(item)}
					{#if item.key === 'Docker ID'}
						<span class="prop-value-copy">
							<span class="truncate">{item.value}</span>
							<Button
								variant="ghost"
								size="icon"
								title="Copy Docker ID"
								aria-label="Copy Docker ID"
								onclick={() => copyText(network!.docker_network_id!, 'dockerId')}
							>
								{#if copied === 'dockerId'}
									<Check size={11} />
								{:else}
									<Copy size={11} />
								{/if}
							</Button>
						</span>
					{:else}
						<span class="truncate">{item.value}</span>
					{/if}
				{/snippet}
			</KeyValueList>
		</section>

		<!-- Danger Zone -->
		{#if canNetworkWrite}
		<div class="danger-wrap">
		<Card tone="danger">
			<div class="danger-zone">
				<div class="danger-header">
					<AlertTriangle size={14} />
					<h3 class="danger-title">Danger Zone</h3>
				</div>
				<p class="danger-desc">
					Deleting this network will remove it from Docker and disconnect any attached services.
					This action cannot be undone.
				</p>
				<FormField label={`Type ${network.name} to confirm`} for="confirm-net-name">
					<TextField
						id="confirm-net-name"
						type="text"
						placeholder={network.name}
						bind:value={confirmName}
					/>
				</FormField>
				{#if deleteError}
					<div role="alert"><InlineAlert tone="error">{deleteError}</InlineAlert></div>
				{/if}
				<Button
					variant="danger-outline"
					disabled={!canDelete || deleting}
					onclick={deleteNetwork}
				>
					<Trash2 size={13} />
					{deleting ? 'Deleting…' : 'Delete Network'}
				</Button>
			</div>
		</Card>
		</div>
		{/if}
	{/if}
</div>

<style>
	.panel-wrap {
		padding: 16px;
		height: 100%;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		gap: 20px;
	}

	.center-state {
		display: flex;
		align-items: center;
		gap: 10px;
		color: var(--text-muted);
		font-size: 13px;
		padding: 40px 0;
		justify-content: center;
	}
	.center-state.error { color: var(--accent-red); }

	.detail-section { display: flex; flex-direction: column; gap: 10px; }

	.section-title {
		font-size: 11px; font-weight: 600; color: var(--text-dim);
		text-transform: uppercase; letter-spacing: 0.07em; margin: 0;
	}

	.prop-value-copy { display: flex; align-items: center; gap: 6px; min-width: 0; }
	.truncate { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

	/* Danger Zone */
	.danger-wrap { margin-top: auto; }
	.danger-zone { display: flex; flex-direction: column; gap: 12px; }
	.danger-header { display: flex; align-items: center; gap: 7px; color: var(--accent-red); }
	.danger-title { font-size: 13px; font-weight: 700; color: var(--accent-red); margin: 0; }
	.danger-desc { font-size: 12px; color: var(--text-muted); margin: 0; line-height: 1.5; }
</style>
