<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { ChevronLeft, Loader2, AlertCircle } from '@lucide/svelte';
	import api from '$lib/api/client';
	import type { Deployment } from '$lib/api/types';
	import DeploymentLogsPanel from '$lib/panels/DeploymentLogsPanel.svelte';
	import { orgStore } from '$lib/stores/org.store';
	import { can, perm } from '$lib/auth/permissions';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';

	let orgId = $derived($orgStore.activeOrg?.id ?? '');
	let myRole = $derived($orgStore.myMembership?.role ?? null);
	let myPerms = $derived($orgStore.myMembership?.permissions ?? []);
	let membershipLoaded = $derived($orgStore.membershipLoaded);

	let canDeploymentsRead = $derived(
		can(myRole, myPerms, perm(orgId, 'deployments', 'read')) ||
		can(myRole, myPerms, perm(orgId, 'settings', 'read'))
	);

	let deploymentId = $derived($page.params.deploymentId);
	let orgSlug = $derived($page.params.orgSlug);

	let deployment = $state<Deployment | null>(null);
	let loading = $state(true);
	let error = $state('');

	async function loadDeployment() {
		if (!deploymentId) return;
		loading = true;
		error = '';
		try {
			const res = await api.getDeployment(deploymentId);
			if (res.data) {
				deployment = res.data;
			} else {
				error = res.error?.message ?? 'Failed to load deployment details';
			}
		} catch {
			error = 'Failed to load deployment details';
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		if (canDeploymentsRead) {
			loadDeployment();
		}
	});
</script>

<PermissionDeniedDialog
	open={membershipLoaded && !!orgId && !canDeploymentsRead}
	message="You need the 'View deployments' permission to access this page."
	onDismiss={() => history.back()}
	onBack={() => history.back()}
/>

{#if canDeploymentsRead}
<div class="page">
	<div class="page-header">
		<button class="back-btn" onclick={() => goto(`/orgs/${orgSlug}/settings/deployments`)}>
			<ChevronLeft size={16} />
			Back to Deployments
		</button>
	</div>

	{#if loading}
		<div class="state-container">
			<Loader2 size={24} class="spin text-muted" />
			<span class="text-muted">Loading deployment details…</span>
		</div>
	{:else if error}
		<div class="state-container error">
			<AlertCircle size={24} />
			<span>{error}</span>
		</div>
	{:else if deployment}
		<div class="panel-container">
			<DeploymentLogsPanel
				orgId={orgId}
				projectId={deployment.project_id || ''}
				serviceId={deployment.service_id}
				deployment={deployment}
			/>
		</div>
	{/if}
</div>
{/if}

<style>
	.page {
		display: flex;
		flex-direction: column;
		height: 100%;
		gap: 16px;
	}

	.page-header {
		display: flex;
		align-items: center;
		flex-shrink: 0;
	}

	.back-btn {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		background: none;
		border: none;
		font-size: 13px;
		font-weight: 500;
		color: var(--text-muted);
		cursor: pointer;
		padding: 4px 8px;
		border-radius: 4px;
		margin-left: -8px;
		transition: color 0.12s, background 0.12s;
	}
	.back-btn:hover {
		color: var(--text-primary);
		background: var(--bg-muted);
	}

	.state-container {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 12px;
		flex: 1;
		padding: 48px;
		font-size: 14px;
	}
	.state-container.error {
		color: #ef4444;
	}

	.panel-container {
		display: flex;
		flex-direction: column;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--bg-surface);
		overflow: hidden;
		flex: 1;
	}

	.text-muted {
		color: var(--text-muted);
	}

	:global(.spin) {
		animation: spin 1s linear infinite;
	}
	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
