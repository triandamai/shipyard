<script lang="ts">
	interface Account {
		id: string;
		label: string;
		host: string;
		token: string;
	}

	interface Props {
		accounts: Account[];
		onSelect: (account: Account) => void;
	}

	let { accounts, onSelect }: Props = $props();
</script>

<div class="picker-wrap">
	{#if accounts.length === 0}
		<div class="empty">No Git accounts connected. Go to Settings → Git Providers to add one.</div>
	{:else}
		<p class="hint">Select the account that has access to the repository.</p>
		<div class="account-list">
			{#each accounts as account (account.id)}
				<button type="button" class="account-card" onclick={() => onSelect(account)}>
					<div class="account-logo">
						{#if account.id === 'github'}
							<svg viewBox="0 0 24 24" fill="currentColor" width="22" height="22"><path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0 0 24 12c0-6.63-5.37-12-12-12z"/></svg>
						{:else if account.id === 'gitlab'}
							<svg viewBox="0 0 24 24" fill="currentColor" width="22" height="22"><path d="M22.65 14.39L12 22.13 1.35 14.39a.84.84 0 0 1-.3-.94l1.22-3.78 2.44-7.51A.42.42 0 0 1 4.82 2a.43.43 0 0 1 .58 0 .42.42 0 0 1 .11.18l2.44 7.49h8.1l2.44-7.49a.42.42 0 0 1 .11-.18.43.43 0 0 1 .58 0 .42.42 0 0 1 .11.18l2.44 7.51L23 13.45a.84.84 0 0 1-.35.94z"/></svg>
						{:else}
							<svg viewBox="0 0 24 24" fill="currentColor" width="22" height="22"><path d="M.778 12C.778 5.773 5.772.778 12 .778c6.228 0 11.222 4.995 11.222 11.222 0 6.228-4.994 11.222-11.222 11.222C5.772 23.222.778 18.228.778 12zm11.907-6.258c-1.99 0-3.597 1.608-3.597 3.597 0 1.99 1.608 3.598 3.597 3.598s3.598-1.609 3.598-3.598c0-1.99-1.609-3.597-3.598-3.597zm-5.73 10.03c.598-1.806 2.286-3.116 4.283-3.116h2.895c1.997 0 3.685 1.31 4.283 3.116H6.955z"/></svg>
						{/if}
					</div>
					<div class="account-info">
						<span class="account-label">{account.label}</span>
						<span class="account-host">{account.host}</span>
					</div>
					<span class="chevron">›</span>
				</button>
			{/each}
		</div>
	{/if}
</div>

<style>
	.picker-wrap { padding: 16px; height: 100%; overflow-y: auto; }

	.hint { font-size: 12px; color: var(--text-muted); margin: 0 0 14px; }

	.empty {
		font-size: 13px; color: var(--text-muted); padding: 24px 16px;
		text-align: center; line-height: 1.5;
	}

	.account-list { display: flex; flex-direction: column; gap: 8px; }

	.account-card {
		display: flex; align-items: center; gap: 14px;
		padding: 14px 16px; background: var(--bg-elevated);
		border: 1px solid var(--border); border-radius: var(--radius-md);
		cursor: pointer; text-align: left; width: 100%;
		transition: all var(--transition-fast);
	}
	.account-card:hover { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 5%, var(--bg-elevated)); }

	.account-logo { width: 36px; height: 36px; display: flex; align-items: center; justify-content: center; flex-shrink: 0; color: var(--text-secondary); }

	.account-info { display: flex; flex-direction: column; gap: 2px; flex: 1; min-width: 0; }

	.account-label { font-size: 14px; font-weight: 600; color: var(--text-primary); }
	.account-host  { font-size: 12px; color: var(--text-dim); font-family: var(--font-mono); }

	.chevron { font-size: 18px; color: var(--text-dim); flex-shrink: 0; }
</style>
