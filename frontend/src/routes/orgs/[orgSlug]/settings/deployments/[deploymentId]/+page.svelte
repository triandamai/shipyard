<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/stores';
	import { ChevronLeft } from '@lucide/svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import InlineAlert from '$lib/components/ui/InlineAlert.svelte';
	import Spinner from '$lib/components/ui/Spinner.svelte';
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
		<Button variant="ghost" size="sm" href="/orgs/{orgSlug}/settings/deployments">
			<ChevronLeft size={16} />
			Back to Deployments
		</Button>
	</div>

	{#if loading}
		<div class="state-container">
			<Spinner size={24} />
			<span class="text-muted">Loading deployment details…</span>
		</div>
	{:else if error}
		<div role="alert"><InlineAlert tone="error">{error}</InlineAlert></div>
	{:else if deployment}
		<div class="panel-container">
			<Card padding="0">
				<DeploymentLogsPanel
					orgId={orgId}
					projectId={deployment.project_id || ''}
					serviceId={deployment.service_id}
					deployment={deployment}
				/>
			</Card>
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

	.panel-container {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-height: 0;
	}
	.panel-container :global(.ui-card) {
		display: flex;
		flex-direction: column;
		flex: 1;
		overflow: hidden;
	}

	.text-muted {
		color: var(--text-muted);
	}
</style>
