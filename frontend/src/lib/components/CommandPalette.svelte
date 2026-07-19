<script lang="ts">
	import { X, Search, LayoutDashboard, Settings, Users, LogOut, Moon, Sun, GitBranch, Box } from '@lucide/svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	import { authStore } from '$lib/stores/auth.store';
	import { clearAuthCookies } from '$lib/auth/cookies';
	import { api } from '$lib/api/client';

	interface Props {
		open: boolean;
		onClose: () => void;
	}

	let { open, onClose }: Props = $props();

	let searchQuery = $state('');
	let selectedIndex = $state(0);
	let searchInput: HTMLInputElement | undefined = $state();

	interface Command {
		id: string;
		label: string;
		description?: string;
		shortcut?: string[];
		icon?: any;
		action: () => void;
		category: string;
	}

	const orgSlug = $derived($page.params.orgSlug ?? '');
	const projectSlug = $derived($page.params.projectSlug ?? '');

	const isDark = $derived(
		typeof document !== 'undefined'
			? document.documentElement.getAttribute('data-theme') === 'dark'
			: false
	);

	const allCommands = $derived<Command[]>([
		// Navigation — always available
		{
			id: 'nav-orgs',
			label: 'Go to Organizations',
			description: 'View all your organizations',
			shortcut: ['G', 'O'],
			icon: LayoutDashboard,
			action: () => { goto('/orgs'); onClose(); },
			category: 'Navigation'
		},
		// Org-scoped navigation
		...(orgSlug ? [
			{
				id: 'nav-projects',
				label: 'Go to Projects',
				description: `All projects in ${orgSlug}`,
				shortcut: ['G', 'P'],
				icon: Box,
				action: () => { goto(`/orgs/${orgSlug}/projects`); onClose(); },
				category: 'Navigation'
			},
			{
				id: 'nav-settings',
				label: 'Organization Settings',
				description: 'General & infrastructure settings',
				shortcut: ['G', 'S'],
				icon: Settings,
				action: () => { goto(`/orgs/${orgSlug}/settings/general`); onClose(); },
				category: 'Navigation'
			},
			{
				id: 'nav-members',
				label: 'Manage Members',
				description: 'Add, remove, or change member roles',
				shortcut: ['G', 'M'],
				icon: Users,
				action: () => { goto(`/orgs/${orgSlug}/settings/members`); onClose(); },
				category: 'Navigation'
			},
		] : []),
		// Project-scoped navigation
		...(orgSlug && projectSlug ? [
			{
				id: 'nav-canvas',
				label: 'Go to Canvas',
				description: 'Project topology & service graph',
				shortcut: ['G', 'C'],
				icon: GitBranch,
				action: () => { goto(`/orgs/${orgSlug}/projects/${projectSlug}`); onClose(); },
				category: 'Navigation'
			},
		] : []),
		// Appearance
		{
			id: 'theme-toggle',
			label: isDark ? 'Switch to Light Mode' : 'Switch to Dark Mode',
			description: 'Toggle the application theme',
			shortcut: ['T'],
			icon: isDark ? Sun : Moon,
			action: () => {
				const dark = document.documentElement.getAttribute('data-theme') === 'dark';
				if (dark) {
					document.documentElement.removeAttribute('data-theme');
					try { localStorage.setItem('shipyard_theme', 'light'); } catch { /* */ }
				} else {
					document.documentElement.setAttribute('data-theme', 'dark');
					try { localStorage.setItem('shipyard_theme', 'dark'); } catch { /* */ }
				}
				onClose();
			},
			category: 'Appearance'
		},
		// Account
		{
			id: 'logout',
			label: 'Sign Out',
			description: 'Log out of your account',
			icon: LogOut,
			action: () => {
				onClose();
				api.logout().finally(() => {
					clearAuthCookies();
					authStore.logout();
					api.setToken(null);
					goto('/login');
				});
			},
			category: 'Account'
		},
	]);

	const filteredCommands = $derived(
		searchQuery.trim()
			? allCommands.filter((c) => {
					const q = searchQuery.toLowerCase();
					return (
						c.label.toLowerCase().includes(q) ||
						(c.description?.toLowerCase().includes(q) ?? false) ||
						c.category.toLowerCase().includes(q)
					);
				})
			: allCommands
	);

	const grouped = $derived(
		filteredCommands.reduce(
			(acc, cmd) => {
				(acc[cmd.category] ??= []).push(cmd);
				return acc;
			},
			{} as Record<string, Command[]>
		)
	);

	// Focus input when opened
	$effect(() => {
		if (open) {
			searchQuery = '';
			selectedIndex = 0;
			requestAnimationFrame(() => searchInput?.focus());
		}
	});

	// Clamp selectedIndex when filter changes
	$effect(() => {
		if (selectedIndex >= filteredCommands.length) {
			selectedIndex = Math.max(0, filteredCommands.length - 1);
		}
	});

	function handleKeydown(e: KeyboardEvent) {
		if (!open) return;
		if (e.key === 'Escape') { e.preventDefault(); onClose(); return; }
		if (e.key === 'ArrowDown') {
			e.preventDefault();
			selectedIndex = Math.min(selectedIndex + 1, filteredCommands.length - 1);
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			selectedIndex = Math.max(selectedIndex - 1, 0);
		} else if (e.key === 'Enter') {
			e.preventDefault();
			filteredCommands[selectedIndex]?.action();
		}
	}

	function handleBackdrop(e: MouseEvent) {
		if (e.target === e.currentTarget) onClose();
	}
</script>

<svelte:document onkeydown={handleKeydown} />

{#if open}
	<div class="palette-backdrop" onclick={handleBackdrop} role="presentation">
		<div class="palette-dialog" role="dialog" aria-label="Command palette" aria-modal="true">
			<!-- Search bar -->
			<div class="palette-search">
				<Search size={15} class="search-icon" />
				<input
					bind:this={searchInput}
					bind:value={searchQuery}
					type="text"
					placeholder="Search commands…"
					class="search-input"
					autocomplete="off"
					spellcheck="false"
				/>
				<button class="esc-hint" onclick={onClose} aria-label="Close">esc</button>
			</div>

			<!-- Command list -->
			<div class="palette-list" role="listbox">
				{#each Object.entries(grouped) as [category, commands]}
					<div class="palette-group">
						<div class="group-label" role="presentation">{category}</div>
						{#each commands as cmd}
							{@const idx = filteredCommands.indexOf(cmd)}
							<button
								class="palette-item"
								class:selected={idx === selectedIndex}
								onclick={cmd.action}
								onmouseenter={() => (selectedIndex = idx)}
								role="option"
								aria-selected={idx === selectedIndex}
							>
								{#if cmd.icon}
									<span class="item-icon">
										<cmd.icon size={15} />
									</span>
								{/if}
								<div class="item-body">
									<span class="item-label">{cmd.label}</span>
									{#if cmd.description}
										<span class="item-desc">{cmd.description}</span>
									{/if}
								</div>
								{#if cmd.shortcut?.length}
									<div class="item-shortcut">
										{#each cmd.shortcut as key}
											<kbd>{key}</kbd>
										{/each}
									</div>
								{/if}
							</button>
						{/each}
					</div>
				{/each}

				{#if filteredCommands.length === 0}
					<div class="palette-empty">
						No results for <strong>"{searchQuery}"</strong>
					</div>
				{/if}
			</div>

			<!-- Footer hints -->
			<div class="palette-footer">
				<span><kbd>↑</kbd><kbd>↓</kbd> navigate</span>
				<span><kbd>↵</kbd> select</span>
				<span>right-click or <kbd>⌘K</kbd> to open</span>
			</div>
		</div>
	</div>
{/if}

<style>
	.palette-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(15, 23, 42, 0.45);
		backdrop-filter: blur(3px);
		-webkit-backdrop-filter: blur(3px);
		display: flex;
		align-items: flex-start;
		justify-content: center;
		padding-top: 15vh;
		z-index: 9999;
	}

	.palette-dialog {
		width: min(560px, calc(100vw - 32px));
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-xl);
		box-shadow: var(--shadow-lg), 0 0 0 1px rgba(37, 99, 235, 0.06);
		overflow: hidden;
		display: flex;
		flex-direction: column;
		max-height: 60vh;
	}

	/* ── Search bar ── */
	.palette-search {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 14px 16px;
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}

	.palette-search :global(.search-icon) {
		color: var(--text-muted);
		flex-shrink: 0;
	}

	.search-input {
		flex: 1;
		border: none;
		outline: none;
		background: transparent;
		font-size: 14px;
		font-family: var(--font-sans);
		color: var(--text-primary);
		caret-color: var(--accent);
	}

	.search-input::placeholder {
		color: var(--text-dim);
	}

	.esc-hint {
		font-size: 11px;
		font-family: var(--font-mono);
		color: var(--text-dim);
		background: var(--bg-hover);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		padding: 2px 6px;
		cursor: pointer;
		flex-shrink: 0;
		transition: color var(--transition-fast);
	}

	.esc-hint:hover {
		color: var(--text-muted);
	}

	/* ── Command list ── */
	.palette-list {
		flex: 1;
		overflow-y: auto;
		padding: 6px 0;
	}

	.palette-group {
		padding-bottom: 4px;
	}

	.group-label {
		font-size: 10px;
		font-weight: 600;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--text-dim);
		padding: 8px 16px 4px;
	}

	.palette-item {
		display: flex;
		align-items: center;
		gap: 10px;
		width: 100%;
		padding: 8px 16px;
		background: transparent;
		border: none;
		cursor: pointer;
		text-align: left;
		border-radius: 0;
		transition: background var(--transition-fast);
	}

	.palette-item.selected,
	.palette-item:hover {
		background: var(--accent-muted);
	}

	.palette-item.selected .item-label {
		color: var(--accent);
	}

	.item-icon {
		display: flex;
		align-items: center;
		color: var(--text-muted);
		flex-shrink: 0;
	}

	.palette-item.selected .item-icon {
		color: var(--accent);
	}

	.item-body {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 1px;
		min-width: 0;
	}

	.item-label {
		font-size: 13px;
		font-weight: 500;
		color: var(--text-primary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.item-desc {
		font-size: 11px;
		color: var(--text-muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.item-shortcut {
		display: flex;
		gap: 3px;
		flex-shrink: 0;
	}

	/* ── Footer ── */
	.palette-footer {
		display: flex;
		align-items: center;
		gap: 16px;
		padding: 8px 16px;
		border-top: 1px solid var(--border);
		font-size: 11px;
		color: var(--text-dim);
		flex-shrink: 0;
		background: var(--bg-elevated);
	}

	/* ── kbd tags ── */
	kbd {
		display: inline-block;
		font-size: 10px;
		font-family: var(--font-mono);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-bottom-width: 2px;
		border-radius: 3px;
		padding: 1px 5px;
		color: var(--text-secondary);
		line-height: 1.5;
	}

	.palette-item.selected kbd,
	.palette-footer kbd {
		background: var(--bg-hover);
	}

	/* ── Empty state ── */
	.palette-empty {
		padding: 24px 16px;
		text-align: center;
		font-size: 13px;
		color: var(--text-muted);
	}

	.palette-empty strong {
		color: var(--text-secondary);
	}
</style>
