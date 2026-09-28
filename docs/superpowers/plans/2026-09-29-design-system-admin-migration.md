# Design System + Admin Panel Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a shared Svelte component library under `frontend/src/lib/components/ui/` that speaks the main app's existing `layout.css` design tokens, then migrate every one of the admin panel's 32 pages onto it, retiring admin's separate `.shell` token system and hand-rolled per-page markup entirely.

**Architecture:** Foundation first (token unification in `admin/+layout.svelte`, a new `NavRail`/`NavDrawer` nav shell, and ~30 reusable components), then one migration task per admin page, grouped by the three recurring UI patterns found during investigation (data table, dashboard/stat grid, settings form). Every component is a plain Svelte 5 (runes) `.svelte` file with no new dependencies — no charting library, no component-framework, no new fonts.

**Tech Stack:** SvelteKit 5 (runes: `$state`, `$derived`, `$props`, `$bindable`, snippets), `@lucide/svelte` (icons — already a dependency), existing `layout.css` CSS custom properties (no new tokens).

**Spec:** `docs/superpowers/specs/2026-09-29-design-system-admin-migration-design.md`

## Global Constraints

- **No new color, radius, or font tokens.** Every component uses only tokens already defined in `frontend/src/routes/layout.css`: `--bg-base/-surface/-elevated/-hover`, `--accent` and its `-hover`/`-muted` variants, `--accent-green/-red/-yellow` and their `-muted` variants, `--border`/`-hover`, `--text-primary/-secondary/-muted/-dim`, `--font-sans` (Inter) / `--font-mono` (JetBrains Mono), `--radius-sm` (4px) / `--radius-md` (6px) / `--radius-lg` (10px), `--shadow-sm/-md/-lg/-glow`, `--transition-fast/-normal/-slow`.
- **Material 3 governs component structure/states/anatomy only** — state-layer hover/focus/pressed treatment, component anatomy (a Toggle's track+thumb, a Modal's scrim+container+actions), accessibility roles/keyboard behavior. It does **not** override the token values above (no M3 default colors, no M3's own type scale sizes, no pill-shaped buttons unless a task says so).
- **Icons are `@lucide/svelte` only.** Never write raw `<svg><path d="…"/></svg>` markup inline in any component or page. Every admin page currently does this; every migration task replaces it.
- **No backend changes**, with two narrow exceptions already identified in the spec: none are actually needed — `DataTable`'s server mode is wired only to the two endpoints (`/admin/projects`, `/admin/deployments/app`) that already accept `page`/`limit` query params today. Every other page uses `DataTable` in client mode (fetch everything, paginate/filter in memory) — the same data-fetching behavior those pages already have today, just through the shared component.
- **No test framework exists** (no Vitest). Verification per task is: `cd frontend && npm run check` (svelte-check, must show 0 new errors), `cd frontend && npm run build` (must succeed), and a manual visual check in a real browser in **both** light and dark theme (toggle via the existing theme button) — not a new automated test suite.
- **Svelte 5 runes only** — `let { prop }: Props = $props();`, `$state()`, `$derived()`, `$bindable()` for two-way-bound props, `{#snippet}`/`{@render}` for slot-like content. Match the syntax already used throughout `frontend/src/lib/components/*.svelte` (e.g. `FileTree.svelte`, `SandboxTerminal.svelte`).
- **Directory:** every new shared component lives in `frontend/src/lib/components/ui/`, one file per component, plus a barrel `frontend/src/lib/components/ui/index.ts` re-exporting all of them. This is a new directory — it does not touch or move anything in the existing `frontend/src/lib/components/` (page-specific components) or `frontend/src/lib/panels/` (slide-panel components).

---

## Part A — Foundation

### Task 1: Fix `Toast.svelte`'s remaining hardcoded colors

A toast component and store already exist (`frontend/src/lib/components/Toast.svelte`, `frontend/src/lib/stores/toast.store.ts`) and are already mounted globally in `frontend/src/routes/+layout.svelte`, so admin pages can use `toastStore.add(...)` with zero new wiring. It already uses `layout.css` tokens for its base surface (`--bg-surface`, `--border`, `--shadow-lg`, `--text-primary`, `--text-muted`) — it just has four hardcoded status hex colors and a radius that doesn't match the approved scale. No new component needed here, just a token cleanup.

**Files:**
- Modify: `frontend/src/lib/components/Toast.svelte`

**Interfaces:**
- Consumes: `--accent-green`, `--accent-red`, `--accent-yellow`, `--accent` (already exist in `layout.css`), `--radius-lg` (already exists).
- Produces: no interface change — `toastStore.add({ type, title, message? }, duration?)` is unchanged and every later task that calls it uses this exact signature.

- [ ] **Step 1: Replace the hardcoded status colors and radius**

In `frontend/src/lib/components/Toast.svelte`, change:

```css
.toast {
    /* ... unchanged ... */
    border-radius: 8px;
    /* ... unchanged ... */
}

.toast--success { border-left-color: #16a34a; }
.toast--error   { border-left-color: #dc2626; }
.toast--warning { border-left-color: #d97706; }
.toast--info    { border-left-color: var(--accent, #2563eb); }

.toast--success .toast__icon { color: #16a34a; }
.toast--error   .toast__icon { color: #dc2626; }
.toast--warning .toast__icon { color: #d97706; }
.toast--info    .toast__icon { color: var(--accent, #2563eb); }
```

to:

```css
.toast {
    /* ... unchanged ... */
    border-radius: var(--radius-lg);
    /* ... unchanged ... */
}

.toast--success { border-left-color: var(--accent-green); }
.toast--error   { border-left-color: var(--accent-red); }
.toast--warning { border-left-color: var(--accent-yellow); }
.toast--info    { border-left-color: var(--accent); }

.toast--success .toast__icon { color: var(--accent-green); }
.toast--error   .toast__icon { color: var(--accent-red); }
.toast--warning .toast__icon { color: var(--accent-yellow); }
.toast--info    .toast__icon { color: var(--accent); }
```

- [ ] **Step 2: Verify**

Run: `cd frontend && npm run check` — expect 0 new errors (this file already type-checks; this is a pure CSS value swap).

- [ ] **Step 3: Manual visual check**

Trigger a toast somewhere it's already wired (e.g. `frontend/src/routes/orgs/[orgSlug]/billing/+page.svelte` already calls `toastStore.add`) in both light and dark theme. Confirm the four status colors render as the app's existing green/red/yellow/blue tokens, not the old hardcoded hex.

- [ ] **Step 4: Commit**

```bash
cd frontend && git add src/lib/components/Toast.svelte
git commit -m "fix(design-system): Toast uses design tokens instead of hardcoded colors"
```

---

### Task 2: Retire admin's separate token system

`admin/+layout.svelte` defines an entire parallel `.shell`-scoped CSS custom-property block (`--bg`, `--accent: #1d4ed8`, `--radius: 9px`, `system-ui` font, its own dark/light values) and a second independent theme toggle (`localStorage['shipyard-admin-theme']`). This task deletes that block and wires admin onto the main app's existing tokens and theme mechanism. It does **not** touch the nav markup yet — that's Task 29. This task only changes what CSS variables resolve to and how theme state is read/written.

**Files:**
- Modify: `frontend/src/routes/admin/+layout.svelte`

**Interfaces:**
- Consumes: `layout.css`'s existing `[data-theme="dark"]` mechanism — theme is toggled by setting `document.documentElement.dataset.theme = 'dark' | ''`, read from `localStorage.getItem('shipyard_theme')`, exactly as `IconSidebar.svelte` already does today (see `frontend/src/lib/components/IconSidebar.svelte`'s `toggleTheme`/`$effect` block).
- Produces: every admin page's existing `var(--bg)`, `var(--surface)`, `var(--accent)`, etc. references now resolve to `layout.css` values instead of `.shell`'s own — later page-migration tasks replace these var names as they migrate each page's markup to the new components, but until a page is migrated, its old CSS still needs *some* value for these vars to not break visually mid-migration. This task therefore **aliases** the old admin-only var names to the new tokens (rather than deleting them outright) so unmigrated pages keep rendering correctly during the transition, and each page-migration task removes that page's use of the old names as it migrates.

- [ ] **Step 1: Replace the `.shell` token block with an alias layer**

In `frontend/src/routes/admin/+layout.svelte`, replace:

```css
	/* ── Design tokens ────────────────────── */
	.shell {
		--bg:            #f5f5f5;
		--surface:       #ffffff;
		--surface-2:     #f0f0f0;
		--border:        #e3e3e3;
		--border-2:      #cecece;
		--text:          #111111;
		--text-2:        #555555;
		--text-3:        #9a9a9a;
		--text-4:        #c4c4c4;
		--accent:        #1d4ed8;
		--accent-soft:   rgba(29,78,216,0.09);
		--accent-ring:   rgba(29,78,216,0.25);
		--ok:            #16a34a;
		--ok-soft:       rgba(22,163,74,0.08);
		--warn:          #b45309;
		--warn-soft:     rgba(180,83,9,0.08);
		--danger:        #dc2626;
		--danger-soft:   rgba(220,38,38,0.08);
		--row-hover:     rgba(0,0,0,0.022);
		--shadow-sm:     0 1px 2px rgba(0,0,0,0.07);
		--shadow:        0 1px 3px rgba(0,0,0,0.09), 0 1px 2px rgba(0,0,0,0.05);
		--shadow-md:     0 4px 8px rgba(0,0,0,0.08), 0 2px 4px rgba(0,0,0,0.04);
		--radius:        9px;
		--radius-sm:     6px;
		--font:          system-ui, -apple-system, 'Segoe UI', sans-serif;
		--mono:          ui-monospace, 'JetBrains Mono', 'Fira Code', monospace;
		display:flex;
		height:100vh;
		overflow:hidden;
		font-family:var(--font);
		font-size:13px;
		-webkit-font-smoothing:antialiased;
	}
	.shell[data-theme="dark"] {
		--bg:            #111111;
		--surface:       #1a1a1a;
		--surface-2:     #212121;
		--border:        #2d2d2d;
		--border-2:      #3d3d3d;
		--text:          #efefef;
		--text-2:        #999999;
		--text-3:        #585858;
		--text-4:        #3d3d3d;
		--accent:        #3b82f6;
		--accent-soft:   rgba(59,130,246,0.1);
		--accent-ring:   rgba(59,130,246,0.22);
		--ok:            #22c55e;
		--ok-soft:       rgba(34,197,94,0.1);
		--warn:          #f59e0b;
		--warn-soft:     rgba(245,158,11,0.1);
		--danger:        #ef4444;
		--danger-soft:   rgba(239,68,68,0.1);
		--row-hover:     rgba(255,255,255,0.025);
		--shadow-sm:     0 1px 2px rgba(0,0,0,0.35);
		--shadow:        0 1px 3px rgba(0,0,0,0.45), 0 1px 2px rgba(0,0,0,0.3);
		--shadow-md:     0 4px 8px rgba(0,0,0,0.5), 0 2px 4px rgba(0,0,0,0.3);
	}
```

with:

```css
	/* ── Design tokens: alias admin's old var names onto the shared design
	   system's real tokens (defined in frontend/src/routes/layout.css, which
	   the root layout already imports globally). This keeps every
	   not-yet-migrated admin page's existing `var(--bg)` etc. references
	   working during the page-by-page migration (Part B of the plan) — once
	   a page migrates, it stops using these names entirely and this alias
	   layer becomes dead weight to delete in a final cleanup task. */
	.shell {
		--bg:            var(--bg-base);
		--surface:       var(--bg-surface);
		--surface-2:     var(--bg-elevated);
		--border:        var(--border);
		--border-2:      var(--border-hover);
		--text:          var(--text-primary);
		--text-2:        var(--text-secondary);
		--text-3:        var(--text-muted);
		--text-4:        var(--text-dim);
		--accent:        var(--accent);
		--accent-soft:   var(--accent-muted);
		--accent-ring:   color-mix(in srgb, var(--accent) 25%, transparent);
		--ok:            var(--accent-green);
		--ok-soft:       var(--accent-green-muted);
		--warn:          var(--accent-yellow);
		--warn-soft:     var(--accent-yellow-muted);
		--danger:        var(--accent-red);
		--danger-soft:   var(--accent-red-muted);
		--row-hover:     var(--bg-hover);
		--shadow-sm:     var(--shadow-sm);
		--shadow:        var(--shadow-md);
		--shadow-md:     var(--shadow-lg);
		--radius:        var(--radius-lg);
		--radius-sm:     var(--radius-md);
		--font:          var(--font-sans);
		--mono:          var(--font-mono);
		display:flex;
		height:100vh;
		overflow:hidden;
		font-family:var(--font);
		font-size:13px;
		-webkit-font-smoothing:antialiased;
	}
```

Note there is no more `.shell[data-theme="dark"]` block at all — dark-mode values now come from `layout.css`'s own `[data-theme="dark"]` block on `:root`, the same one the rest of the app uses.

- [ ] **Step 2: Replace the admin-only theme toggle with the shared one**

In the `<script>` block of `frontend/src/routes/admin/+layout.svelte`, replace:

```typescript
	let theme         = $state<'light' | 'dark'>('light');
	// ...
	onMount(async () => {
		const savedTheme = localStorage.getItem('shipyard-admin-theme');
		if (savedTheme === 'dark' || savedTheme === 'light') theme = savedTheme;
		else if (window.matchMedia('(prefers-color-scheme: dark)').matches) theme = 'dark';
		// ...
	});

	function toggleTheme() {
		theme = theme === 'light' ? 'dark' : 'light';
		localStorage.setItem('shipyard-admin-theme', theme);
	}
```

with:

```typescript
	let theme = $state<'light' | 'dark'>('light');

	onMount(async () => {
		// Same theme storage key and mechanism as IconSidebar.svelte — one
		// shared theme state for the whole app, not a separate admin one.
		const saved = localStorage.getItem('shipyard_theme');
		theme = saved === 'dark' ? 'dark' : 'light';
		document.documentElement.setAttribute('data-theme', theme === 'dark' ? 'dark' : '');
		// ... (rest of the existing onMount body — the auth/superadmin check — is unchanged)
	});

	function toggleTheme() {
		theme = theme === 'light' ? 'dark' : 'light';
		localStorage.setItem('shipyard_theme', theme);
		document.documentElement.setAttribute('data-theme', theme === 'dark' ? 'dark' : '');
	}
```

The template's `<div class="shell" data-theme={theme}>` stays as-is — `.shell` no longer keys any CSS off its own `data-theme` attribute (that block is gone from Step 1), but leaving the attribute on the element is harmless and costs nothing to remove later; leave it for now to minimize the diff.

- [ ] **Step 3: Verify**

Run: `cd frontend && npm run check` — expect 0 new errors.
Run: `cd frontend && npm run build` — expect success (this is the highest-risk step in this task: every one of the 32 admin pages depends on these var names resolving to *something* sensible, and a build failure here would mean a typo in the alias block).

- [ ] **Step 4: Manual visual check**

Open any two admin pages (e.g. the admin root and `docker/containers`) in a real browser. Toggle theme via admin's existing toggle button. Confirm:
- Colors still look sane in both modes (nothing renders as transparent/black-on-black — a sign an alias is missing or misspelled).
- Toggling theme on an admin page and then navigating to a *non-admin* page (e.g. `/orgs`) shows the same theme, and vice versa — confirming the two apps now share one theme state instead of two independent ones.

- [ ] **Step 5: Commit**

```bash
cd frontend && git add src/routes/admin/+layout.svelte
git commit -m "refactor(admin): alias admin's tokens onto the shared design system, retire separate theme toggle"
```

---

### Task 3: `Button`

The first and most-used component — every other component and every page migration depends on it existing. Formalizes the `.btn`/`.btn-primary`/`.btn-secondary`/`.btn-ghost`/`.btn-danger` global classes already documented as the main app's convention (see `frontend/src/routes/layout.css`'s `.btn*` rules) into a real Svelte component, and adds the `danger-outline` variant and M3-style state layers.

**Files:**
- Create: `frontend/src/lib/components/ui/Button.svelte`

**Interfaces:**
- Consumes: `layout.css`'s `--accent`, `--accent-hover`, `--accent-red`, `--border`, `--text-primary/-secondary`, `--radius-md`, `--transition-fast`.
- Produces: `Button` component with props `variant: 'primary' | 'secondary' | 'ghost' | 'danger' | 'danger-outline'` (default `'primary'`), `size: 'sm' | 'md' | 'icon'` (default `'md'`), `type: 'button' | 'submit'` (default `'button'`), `disabled: boolean` (default `false`), `onclick: (e: MouseEvent) => void` (optional), and a default `children` snippet for its label/icon content. Every later component and page-migration task that renders a button uses exactly this component and these prop names.

- [ ] **Step 1: Create the component**

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		variant?: 'primary' | 'secondary' | 'ghost' | 'danger' | 'danger-outline';
		size?: 'sm' | 'md' | 'icon';
		type?: 'button' | 'submit';
		disabled?: boolean;
		onclick?: (e: MouseEvent) => void;
		children: Snippet;
	}

	let {
		variant = 'primary',
		size = 'md',
		type = 'button',
		disabled = false,
		onclick,
		children
	}: Props = $props();
</script>

<button
	{type}
	{disabled}
	class="ui-btn ui-btn--{variant} ui-btn--{size}"
	{onclick}
>
	{@render children()}
</button>

<style>
	.ui-btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 6px;
		border-radius: var(--radius-md);
		font-family: var(--font-sans);
		font-weight: 600;
		cursor: pointer;
		border: 1px solid transparent;
		transition: background var(--transition-fast), border-color var(--transition-fast), opacity var(--transition-fast);
		white-space: nowrap;
		position: relative;
	}
	.ui-btn:disabled { opacity: 0.5; cursor: not-allowed; }

	/* Sizes */
	.ui-btn--sm { padding: 5px 12px; font-size: 12px; height: 30px; }
	.ui-btn--md { padding: 7px 16px; font-size: 13px; height: 36px; }
	.ui-btn--icon { padding: 0; width: 32px; height: 32px; }

	/* Variants — state layers via a pseudo-element overlay (M3 pattern: a
	   semi-transparent layer on top of the base color, not a hue shift) so
	   hover/pressed feedback is consistent across every variant. */
	.ui-btn::after {
		content: '';
		position: absolute;
		inset: 0;
		border-radius: inherit;
		background: currentColor;
		opacity: 0;
		transition: opacity var(--transition-fast);
		pointer-events: none;
	}
	.ui-btn:hover:not(:disabled)::after { opacity: 0.08; }
	.ui-btn:active:not(:disabled)::after { opacity: 0.12; }

	.ui-btn--primary {
		background: var(--accent);
		color: #fff;
		border-color: var(--accent);
	}
	.ui-btn--primary:hover:not(:disabled) { background: var(--accent-hover); border-color: var(--accent-hover); }

	.ui-btn--secondary {
		background: var(--bg-surface);
		color: var(--text-secondary);
		border-color: var(--border);
	}
	.ui-btn--secondary:hover:not(:disabled) { background: var(--bg-hover); }

	.ui-btn--ghost {
		background: transparent;
		color: var(--text-secondary);
	}
	.ui-btn--ghost:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }

	.ui-btn--danger {
		background: var(--accent-red);
		color: #fff;
		border-color: var(--accent-red);
	}

	.ui-btn--danger-outline {
		background: transparent;
		color: var(--accent-red);
		border-color: var(--accent-red);
	}
	.ui-btn--danger-outline:hover:not(:disabled) { background: var(--accent-red-muted); }
</style>
```

- [ ] **Step 2: Verify**

Run: `cd frontend && npm run check` — expect 0 errors on this new file.

- [ ] **Step 3: Manual visual check**

Create a scratch route (or use the Svelte REPL pattern from earlier design work in this session — a temporary `frontend/src/routes/_test-ui/+page.svelte` that imports and renders one of each variant/size, deleted after checking) and confirm all 5 variants × 3 sizes render correctly in both light and dark theme, including hover/active state-layer feedback. Delete the scratch route when done.

- [ ] **Step 4: Commit**

```bash
cd frontend && git add src/lib/components/ui/Button.svelte
git commit -m "feat(design-system): add Button component"
```

---

### Task 4: `Badge` and `StatusDot`

Formalizes the already-documented `.badge`/`.badge-*` and `.status-dot` global classes (see `layout.css`) into real components.

**Files:**
- Create: `frontend/src/lib/components/ui/Badge.svelte`
- Create: `frontend/src/lib/components/ui/StatusDot.svelte`

**Interfaces:**
- Produces: `Badge` with props `tone: 'green' | 'red' | 'yellow' | 'blue' | 'neutral'` (default `'neutral'`) and a `children` snippet. `StatusDot` with prop `status: 'running' | 'pending' | 'deploying' | 'failed' | 'stopped'`.

- [ ] **Step 1: Create `Badge.svelte`**

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		tone?: 'green' | 'red' | 'yellow' | 'blue' | 'neutral';
		children: Snippet;
	}

	let { tone = 'neutral', children }: Props = $props();
</script>

<span class="ui-badge ui-badge--{tone}">{@render children()}</span>

<style>
	.ui-badge {
		display: inline-flex;
		align-items: center;
		padding: 2px 9px;
		border-radius: 999px;
		font-size: 11px;
		font-weight: 600;
		font-family: var(--font-sans);
		letter-spacing: 0.01em;
		white-space: nowrap;
	}
	.ui-badge--green   { background: var(--accent-green-muted);  color: var(--accent-green); }
	.ui-badge--red     { background: var(--accent-red-muted);    color: var(--accent-red); }
	.ui-badge--yellow  { background: var(--accent-yellow-muted); color: var(--accent-yellow); }
	.ui-badge--blue    { background: var(--accent-muted);        color: var(--accent); }
	.ui-badge--neutral { background: var(--bg-hover);            color: var(--text-muted); }
</style>
```

- [ ] **Step 2: Create `StatusDot.svelte`**

```svelte
<script lang="ts">
	interface Props {
		status: 'running' | 'pending' | 'deploying' | 'failed' | 'stopped';
	}

	let { status }: Props = $props();
</script>

<span class="ui-dot ui-dot--{status}" aria-hidden="true"></span>

<style>
	.ui-dot {
		display: inline-block;
		width: 7px;
		height: 7px;
		border-radius: 50%;
		flex-shrink: 0;
	}
	.ui-dot--running   { background: var(--accent-green); }
	.ui-dot--pending   { background: var(--accent-yellow); animation: ui-dot-pulse 1.6s ease-in-out infinite; }
	.ui-dot--deploying { background: var(--accent); animation: ui-dot-pulse 1.2s ease-in-out infinite; }
	.ui-dot--failed    { background: var(--accent-red); }
	.ui-dot--stopped   { background: var(--text-dim); }

	@keyframes ui-dot-pulse {
		0%, 100% { opacity: 1; }
		50%      { opacity: 0.35; }
	}
</style>
```

- [ ] **Step 3: Verify**

Run: `cd frontend && npm run check`.

- [ ] **Step 4: Manual visual check**

Render all 5 badge tones and all 5 status-dot states in a scratch route, both themes.

- [ ] **Step 5: Commit**

```bash
cd frontend && git add src/lib/components/ui/Badge.svelte src/lib/components/ui/StatusDot.svelte
git commit -m "feat(design-system): add Badge and StatusDot components"
```

---

### Task 5: `Avatar` and `IconBadge`

The circular soft-tint containers used throughout the approved visual direction — table row leading icons, org/user avatars, stat-card icons.

**Files:**
- Create: `frontend/src/lib/components/ui/Avatar.svelte`
- Create: `frontend/src/lib/components/ui/IconBadge.svelte`

**Interfaces:**
- Produces: `Avatar` with props `initials: string`, `tone: 'blue' | 'green' | 'red' | 'yellow' | 'purple'` (default `'blue'`), `size: number` (default `30`). `IconBadge` with props `tone: 'blue' | 'green' | 'red' | 'yellow'` (default `'blue'`), `size: number` (default `32`), and a `children` snippet (the icon element, e.g. a `@lucide/svelte` icon).

- [ ] **Step 1: Create `Avatar.svelte`**

```svelte
<script lang="ts">
	interface Props {
		initials: string;
		tone?: 'blue' | 'green' | 'red' | 'yellow' | 'purple';
		size?: number;
	}

	let { initials, tone = 'blue', size = 30 }: Props = $props();
</script>

<span
	class="ui-avatar ui-avatar--{tone}"
	style="width:{size}px;height:{size}px;font-size:{Math.round(size * 0.38)}px"
>
	{initials.slice(0, 2).toUpperCase()}
</span>

<style>
	.ui-avatar {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		border-radius: 50%;
		font-weight: 700;
		font-family: var(--font-sans);
		flex-shrink: 0;
	}
	.ui-avatar--blue   { background: var(--accent-muted);        color: var(--accent); }
	.ui-avatar--green  { background: var(--accent-green-muted);  color: var(--accent-green); }
	.ui-avatar--red    { background: var(--accent-red-muted);    color: var(--accent-red); }
	.ui-avatar--yellow { background: var(--accent-yellow-muted); color: var(--accent-yellow); }
	.ui-avatar--purple { background: color-mix(in srgb, #7c3bc2 14%, transparent); color: #7c3bc2; }
</style>
```

(`purple` is the one deliberate exception to "tokens only" — it exists solely as an *additional category tag color* for avatars representing arbitrary entities like organization names, per the spec's "category/type tagging may draw from a small fixed palette of additional soft tones" allowance. It is never used for semantic status — status always uses blue/green/red/yellow.)

- [ ] **Step 2: Create `IconBadge.svelte`**

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		tone?: 'blue' | 'green' | 'red' | 'yellow';
		size?: number;
		children: Snippet;
	}

	let { tone = 'blue', size = 32, children }: Props = $props();
</script>

<span class="ui-icon-badge ui-icon-badge--{tone}" style="width:{size}px;height:{size}px">
	{@render children()}
</span>

<style>
	.ui-icon-badge {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		border-radius: 50%;
		flex-shrink: 0;
	}
	.ui-icon-badge--blue   { background: var(--accent-muted);        color: var(--accent); }
	.ui-icon-badge--green  { background: var(--accent-green-muted);  color: var(--accent-green); }
	.ui-icon-badge--red    { background: var(--accent-red-muted);    color: var(--accent-red); }
	.ui-icon-badge--yellow { background: var(--accent-yellow-muted); color: var(--accent-yellow); }
</style>
```

- [ ] **Step 3: Verify**

Run: `cd frontend && npm run check`.

- [ ] **Step 4: Manual visual check**

Render `Avatar` (all 5 tones) and `IconBadge` (all 4 tones, with a `@lucide/svelte` icon like `Server` or `Users` passed as children) in a scratch route, both themes.

- [ ] **Step 5: Commit**

```bash
cd frontend && git add src/lib/components/ui/Avatar.svelte src/lib/components/ui/IconBadge.svelte
git commit -m "feat(design-system): add Avatar and IconBadge components"
```

---

### Task 6: `Spinner` and `Skeleton`

Formalizes the already-documented `.btn-spinner`/`.spinner-sm` pattern and the `sk`/`sk-row`/`sk-card` skeleton-loader pattern duplicated across nearly every admin page's loading state.

**Files:**
- Create: `frontend/src/lib/components/ui/Spinner.svelte`
- Create: `frontend/src/lib/components/ui/Skeleton.svelte`

**Interfaces:**
- Produces: `Spinner` with prop `size: number` (default `16`). `Skeleton` with props `variant: 'row' | 'card' | 'text'` (default `'text'`), `width: string` (default `'100%'`), `height: string` (default depends on variant: `'14px'` for text, `'52px'` for row, `'96px'` for card).

- [ ] **Step 1: Create `Spinner.svelte`**

```svelte
<script lang="ts">
	interface Props {
		size?: number;
	}

	let { size = 16 }: Props = $props();
</script>

<span
	class="ui-spinner"
	style="width:{size}px;height:{size}px;border-width:{Math.max(2, Math.round(size / 8))}px"
	role="status"
	aria-label="Loading"
></span>

<style>
	.ui-spinner {
		display: inline-block;
		border-style: solid;
		border-color: var(--border);
		border-top-color: var(--accent);
		border-radius: 50%;
		animation: ui-spin 0.7s linear infinite;
		flex-shrink: 0;
	}
	@keyframes ui-spin {
		to { transform: rotate(360deg); }
	}
</style>
```

- [ ] **Step 2: Create `Skeleton.svelte`**

```svelte
<script lang="ts">
	interface Props {
		variant?: 'row' | 'card' | 'text';
		width?: string;
		height?: string;
	}

	let { variant = 'text', width = '100%', height }: Props = $props();

	let resolvedHeight = $derived(
		height ?? (variant === 'row' ? '52px' : variant === 'card' ? '96px' : '14px')
	);
</script>

<div
	class="ui-skeleton ui-skeleton--{variant}"
	style="width:{width};height:{resolvedHeight}"
></div>

<style>
	.ui-skeleton {
		background: var(--border);
		border-radius: var(--radius-sm);
		animation: ui-skeleton-pulse 1.3s ease-in-out infinite;
	}
	.ui-skeleton--card { border-radius: var(--radius-lg); }
	.ui-skeleton--row { border-radius: var(--radius-md); }

	@keyframes ui-skeleton-pulse {
		0%, 100% { opacity: 0.5; }
		50%      { opacity: 1; }
	}
</style>
```

- [ ] **Step 3: Verify**

Run: `cd frontend && npm run check`.

- [ ] **Step 4: Manual visual check**

Render `Spinner` at a couple of sizes and `Skeleton` in all 3 variants in a scratch route, both themes — confirm the pulse animation is visible and not jarring.

- [ ] **Step 5: Commit**

```bash
cd frontend && git add src/lib/components/ui/Spinner.svelte src/lib/components/ui/Skeleton.svelte
git commit -m "feat(design-system): add Spinner and Skeleton components"
```

---

### Task 7: `Divider` and `Tooltip`

**Files:**
- Create: `frontend/src/lib/components/ui/Divider.svelte`
- Create: `frontend/src/lib/components/ui/Tooltip.svelte`

**Interfaces:**
- Produces: `Divider` with no required props, optional `margin: string` (default `'16px 0'`). `Tooltip` with props `text: string` and a `children` snippet (the trigger element it wraps).

- [ ] **Step 1: Create `Divider.svelte`**

```svelte
<script lang="ts">
	interface Props {
		margin?: string;
	}

	let { margin = '16px 0' }: Props = $props();
</script>

<div class="ui-divider" style="margin:{margin}"></div>

<style>
	.ui-divider {
		height: 1px;
		background: var(--border);
		flex-shrink: 0;
	}
</style>
```

- [ ] **Step 2: Create `Tooltip.svelte`**

Follows M3's tooltip anatomy: a small dark/elevated surface, appears on hover/focus of its wrapped trigger, positioned above by default.

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		text: string;
		children: Snippet;
	}

	let { text, children }: Props = $props();
</script>

<span class="ui-tooltip-wrap">
	{@render children()}
	<span class="ui-tooltip" role="tooltip">{text}</span>
</span>

<style>
	.ui-tooltip-wrap {
		position: relative;
		display: inline-flex;
	}
	.ui-tooltip {
		position: absolute;
		bottom: calc(100% + 6px);
		left: 50%;
		transform: translateX(-50%);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		color: var(--text-primary);
		font-size: 11px;
		font-weight: 500;
		padding: 4px 9px;
		border-radius: var(--radius-sm);
		white-space: nowrap;
		pointer-events: none;
		opacity: 0;
		transition: opacity var(--transition-fast);
		box-shadow: var(--shadow-md);
		z-index: 100;
	}
	.ui-tooltip-wrap:hover .ui-tooltip,
	.ui-tooltip-wrap:focus-within .ui-tooltip {
		opacity: 1;
	}
</style>
```

- [ ] **Step 3: Verify**

Run: `cd frontend && npm run check`.

- [ ] **Step 4: Manual visual check**

Wrap a `Button` in `Tooltip` in a scratch route, confirm it appears on hover in both themes and doesn't clip at viewport edges in a normal page context.

- [ ] **Step 5: Commit**

```bash
cd frontend && git add src/lib/components/ui/Divider.svelte src/lib/components/ui/Tooltip.svelte
git commit -m "feat(design-system): add Divider and Tooltip components"
```

---

### Task 8: `TextField` and `Textarea`

**Files:**
- Create: `frontend/src/lib/components/ui/TextField.svelte`
- Create: `frontend/src/lib/components/ui/Textarea.svelte`

**Interfaces:**
- Produces: `TextField` with `value: string` (`$bindable`), `type: string` (default `'text'`), `placeholder: string` (optional), `disabled: boolean` (default `false`), `id: string` (optional, for label association), `icon: Snippet` (optional, rendered as a left-aligned prefix icon). `Textarea` with `value: string` (`$bindable`), `rows: number` (default `3`), `placeholder: string` (optional), `id: string` (optional).

- [ ] **Step 1: Create `TextField.svelte`**

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		value: string;
		type?: string;
		placeholder?: string;
		disabled?: boolean;
		id?: string;
		icon?: Snippet;
	}

	let {
		value = $bindable(),
		type = 'text',
		placeholder,
		disabled = false,
		id,
		icon
	}: Props = $props();
</script>

{#if icon}
	<div class="ui-textfield-icon-wrap">
		<span class="ui-textfield-icon">{@render icon()}</span>
		<input
			{id}
			{type}
			{placeholder}
			{disabled}
			bind:value
			class="ui-textfield ui-textfield--with-icon"
		/>
	</div>
{:else}
	<input
		{id}
		{type}
		{placeholder}
		{disabled}
		bind:value
		class="ui-textfield"
	/>
{/if}

<style>
	.ui-textfield {
		width: 100%;
		box-sizing: border-box;
		height: 36px;
		padding: 0 11px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-primary);
		font-size: 13px;
		font-family: var(--font-sans);
		outline: none;
		transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
	}
	.ui-textfield::placeholder { color: var(--text-dim); }
	.ui-textfield:focus { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-muted); }
	.ui-textfield:disabled { opacity: 0.5; cursor: not-allowed; }

	.ui-textfield-icon-wrap { position: relative; display: flex; align-items: center; }
	.ui-textfield-icon {
		position: absolute;
		left: 11px;
		display: flex;
		color: var(--text-dim);
		pointer-events: none;
	}
	.ui-textfield--with-icon { padding-left: 32px; }
</style>
```

- [ ] **Step 2: Create `Textarea.svelte`**

```svelte
<script lang="ts">
	interface Props {
		value: string;
		rows?: number;
		placeholder?: string;
		id?: string;
	}

	let { value = $bindable(), rows = 3, placeholder, id }: Props = $props();
</script>

<textarea {id} {rows} {placeholder} bind:value class="ui-textarea"></textarea>

<style>
	.ui-textarea {
		width: 100%;
		box-sizing: border-box;
		padding: 9px 11px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-primary);
		font-size: 13px;
		font-family: var(--font-sans);
		line-height: 1.5;
		outline: none;
		resize: vertical;
		transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
	}
	.ui-textarea::placeholder { color: var(--text-dim); }
	.ui-textarea:focus { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-muted); }
</style>
```

- [ ] **Step 3: Verify**

Run: `cd frontend && npm run check`.

- [ ] **Step 4: Manual visual check**

Render both, with and without an icon on `TextField`, confirm focus ring and placeholder color in both themes.

- [ ] **Step 5: Commit**

```bash
cd frontend && git add src/lib/components/ui/TextField.svelte src/lib/components/ui/Textarea.svelte
git commit -m "feat(design-system): add TextField and Textarea components"
```

---

### Task 9: `Select` and `Toggle`

**Files:**
- Create: `frontend/src/lib/components/ui/Select.svelte`
- Create: `frontend/src/lib/components/ui/Toggle.svelte`

**Interfaces:**
- Produces: `Select` with `value: string` (`$bindable`), `options: { value: string; label: string }[]`, `id: string` (optional). `Toggle` with `checked: boolean` (`$bindable`), `disabled: boolean` (default `false`), `label: string` (optional, used as `aria-label` when there's no visible adjacent label).

- [ ] **Step 1: Create `Select.svelte`**

```svelte
<script lang="ts">
	interface Option {
		value: string;
		label: string;
	}

	interface Props {
		value: string;
		options: Option[];
		id?: string;
	}

	let { value = $bindable(), options, id }: Props = $props();
</script>

<select {id} bind:value class="ui-select">
	{#each options as opt (opt.value)}
		<option value={opt.value}>{opt.label}</option>
	{/each}
</select>

<style>
	.ui-select {
		width: 100%;
		box-sizing: border-box;
		height: 36px;
		padding: 0 11px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-primary);
		font-size: 13px;
		font-family: var(--font-sans);
		outline: none;
		cursor: pointer;
		transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
	}
	.ui-select:focus { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-muted); }
</style>
```

- [ ] **Step 2: Create `Toggle.svelte`**

M3 anatomy: a track + a thumb that slides, not a plain checkbox — matches the pattern already hand-rolled in `smtp/+page.svelte` (`.toggle`/`.toggle-thumb`), formalized here.

```svelte
<script lang="ts">
	interface Props {
		checked: boolean;
		disabled?: boolean;
		label?: string;
	}

	let { checked = $bindable(), disabled = false, label }: Props = $props();
</script>

<button
	type="button"
	role="switch"
	aria-checked={checked}
	aria-label={label}
	{disabled}
	class="ui-toggle"
	class:ui-toggle--on={checked}
	onclick={() => (checked = !checked)}
>
	<span class="ui-toggle-thumb"></span>
</button>

<style>
	.ui-toggle {
		width: 38px;
		height: 22px;
		border-radius: 999px;
		border: none;
		padding: 0;
		background: var(--border);
		position: relative;
		cursor: pointer;
		transition: background var(--transition-fast);
		flex-shrink: 0;
	}
	.ui-toggle:disabled { opacity: 0.5; cursor: not-allowed; }
	.ui-toggle--on { background: var(--accent); }

	.ui-toggle-thumb {
		position: absolute;
		top: 3px;
		left: 3px;
		width: 16px;
		height: 16px;
		border-radius: 50%;
		background: #fff;
		transition: transform var(--transition-fast);
		box-shadow: var(--shadow-sm);
	}
	.ui-toggle--on .ui-toggle-thumb { transform: translateX(16px); }
</style>
```

- [ ] **Step 3: Verify**

Run: `cd frontend && npm run check`.

- [ ] **Step 4: Manual visual check**

Render both, toggle the switch and confirm the thumb slides and the track color changes, in both themes.

- [ ] **Step 5: Commit**

```bash
cd frontend && git add src/lib/components/ui/Select.svelte src/lib/components/ui/Toggle.svelte
git commit -m "feat(design-system): add Select and Toggle components"
```

---

### Task 10: `Checkbox` and `RadioGroup`

**Files:**
- Create: `frontend/src/lib/components/ui/Checkbox.svelte`
- Create: `frontend/src/lib/components/ui/RadioGroup.svelte`

**Interfaces:**
- Produces: `Checkbox` with `checked: boolean` (`$bindable`), `label: string` (optional, rendered inline next to the box). `RadioGroup` with `value: string` (`$bindable`), `options: { value: string; label: string }[]`, `name: string` (for the native radio `name` grouping).

- [ ] **Step 1: Create `Checkbox.svelte`**

```svelte
<script lang="ts">
	interface Props {
		checked: boolean;
		label?: string;
	}

	let { checked = $bindable(), label }: Props = $props();
</script>

<label class="ui-checkbox">
	<input type="checkbox" bind:checked />
	<span class="ui-checkbox-box"></span>
	{#if label}<span class="ui-checkbox-label">{label}</span>{/if}
</label>

<style>
	.ui-checkbox {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		cursor: pointer;
		font-size: 13px;
		color: var(--text-secondary);
	}
	.ui-checkbox input {
		position: absolute;
		opacity: 0;
		width: 0;
		height: 0;
	}
	.ui-checkbox-box {
		width: 16px;
		height: 16px;
		border-radius: 4px;
		border: 1.5px solid var(--border);
		background: var(--bg-elevated);
		display: inline-flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		transition: background var(--transition-fast), border-color var(--transition-fast);
	}
	.ui-checkbox input:checked + .ui-checkbox-box {
		background: var(--accent);
		border-color: var(--accent);
	}
	.ui-checkbox input:checked + .ui-checkbox-box::after {
		content: '';
		width: 4px;
		height: 8px;
		border: solid #fff;
		border-width: 0 1.5px 1.5px 0;
		transform: rotate(45deg) translate(-1px, -1px);
	}
	.ui-checkbox-label { color: var(--text-secondary); }
</style>
```

- [ ] **Step 2: Create `RadioGroup.svelte`**

```svelte
<script lang="ts">
	interface Option {
		value: string;
		label: string;
	}

	interface Props {
		value: string;
		options: Option[];
		name: string;
	}

	let { value = $bindable(), options, name }: Props = $props();
</script>

<div class="ui-radio-group">
	{#each options as opt (opt.value)}
		<label class="ui-radio">
			<input type="radio" {name} value={opt.value} bind:group={value} />
			<span class="ui-radio-dot"></span>
			<span class="ui-radio-label">{opt.label}</span>
		</label>
	{/each}
</div>

<style>
	.ui-radio-group { display: flex; flex-direction: column; gap: 8px; }
	.ui-radio {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		cursor: pointer;
		font-size: 13px;
	}
	.ui-radio input {
		position: absolute;
		opacity: 0;
		width: 0;
		height: 0;
	}
	.ui-radio-dot {
		width: 16px;
		height: 16px;
		border-radius: 50%;
		border: 1.5px solid var(--border);
		background: var(--bg-elevated);
		display: inline-flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		transition: border-color var(--transition-fast);
	}
	.ui-radio input:checked + .ui-radio-dot { border-color: var(--accent); border-width: 5px; }
	.ui-radio-label { color: var(--text-secondary); }
</style>
```

- [ ] **Step 3: Verify**

Run: `cd frontend && npm run check`.

- [ ] **Step 4: Manual visual check**

Render both, confirm checked/selected states render correctly in both themes.

- [ ] **Step 5: Commit**

```bash
cd frontend && git add src/lib/components/ui/Checkbox.svelte src/lib/components/ui/RadioGroup.svelte
git commit -m "feat(design-system): add Checkbox and RadioGroup components"
```

---

### Task 11: `FormField` and `SearchInput`

**Files:**
- Create: `frontend/src/lib/components/ui/FormField.svelte`
- Create: `frontend/src/lib/components/ui/SearchInput.svelte`

**Interfaces:**
- Produces: `FormField` with `label: string`, `hint: string` (optional), `error: string` (optional), `for: string` (optional, `htmlFor` passthrough), and a `children` snippet (the input element). `SearchInput` with `value: string` (`$bindable`), `placeholder: string` (default `'Search…'`).

- [ ] **Step 1: Create `FormField.svelte`**

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		label: string;
		hint?: string;
		error?: string;
		for?: string;
		children: Snippet;
	}

	let { label, hint, error, for: htmlFor, children }: Props = $props();
</script>

<div class="ui-form-field">
	<label class="ui-form-label" for={htmlFor}>{label}</label>
	{@render children()}
	{#if error}
		<span class="ui-form-error">{error}</span>
	{:else if hint}
		<span class="ui-form-hint">{hint}</span>
	{/if}
</div>

<style>
	.ui-form-field { display: flex; flex-direction: column; gap: 6px; }
	.ui-form-label {
		font-size: 11px;
		font-weight: 600;
		color: var(--text-dim);
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}
	.ui-form-hint { font-size: 11px; color: var(--text-dim); line-height: 1.5; }
	.ui-form-error { font-size: 11px; color: var(--accent-red); line-height: 1.5; }
</style>
```

- [ ] **Step 2: Create `SearchInput.svelte`**

Uses `@lucide/svelte`'s `Search` icon — the first component in this plan to import a lucide icon, establishing the pattern every later component and page migration follows.

```svelte
<script lang="ts">
	import { Search } from '@lucide/svelte';

	interface Props {
		value: string;
		placeholder?: string;
	}

	let { value = $bindable(), placeholder = 'Search…' }: Props = $props();
</script>

<div class="ui-search">
	<Search size={13} class="ui-search-icon" />
	<input type="text" {placeholder} bind:value class="ui-search-input" />
</div>

<style>
	.ui-search {
		position: relative;
		display: flex;
		align-items: center;
	}
	:global(.ui-search-icon) {
		position: absolute;
		left: 11px;
		color: var(--text-dim);
		pointer-events: none;
	}
	.ui-search-input {
		height: 34px;
		padding: 0 12px 0 30px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 999px;
		font-size: 12.5px;
		color: var(--text-primary);
		outline: none;
		width: 100%;
		box-sizing: border-box;
		font-family: var(--font-sans);
		transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
	}
	.ui-search-input::placeholder { color: var(--text-dim); }
	.ui-search-input:focus { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-muted); }
</style>
```

(`SearchInput` is the one component that's deliberately pill-shaped (`border-radius: 999px`) rather than `--radius-sm` — matching the approved reference direction's search-field treatment specifically, not a general exception to the radius scale.)

- [ ] **Step 3: Verify**

Run: `cd frontend && npm run check`.

- [ ] **Step 4: Manual visual check**

Render `FormField` wrapping a `TextField` with both a hint and (separately) an error state, and `SearchInput` standalone, both themes.

- [ ] **Step 5: Commit**

```bash
cd frontend && git add src/lib/components/ui/FormField.svelte src/lib/components/ui/SearchInput.svelte
git commit -m "feat(design-system): add FormField and SearchInput components"
```

---

### Task 12: `PageHeader` and `SectionLabel`

**Files:**
- Create: `frontend/src/lib/components/ui/PageHeader.svelte`
- Create: `frontend/src/lib/components/ui/SectionLabel.svelte`

**Interfaces:**
- Produces: `PageHeader` with `eyebrow: string` (optional), `title: string`, `subtitle: string` (optional), and an `actions` snippet (optional, right-aligned). `SectionLabel` with a `children` snippet (its text content).

- [ ] **Step 1: Create `PageHeader.svelte`**

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		eyebrow?: string;
		title: string;
		subtitle?: string;
		actions?: Snippet;
	}

	let { eyebrow, title, subtitle, actions }: Props = $props();
</script>

<header class="ui-page-header">
	<div class="ui-page-header-text">
		{#if eyebrow}<div class="ui-page-header-eyebrow">{eyebrow}</div>{/if}
		<h1 class="ui-page-header-title">{title}</h1>
		{#if subtitle}<p class="ui-page-header-subtitle">{subtitle}</p>{/if}
	</div>
	{#if actions}
		<div class="ui-page-header-actions">{@render actions()}</div>
	{/if}
</header>

<style>
	.ui-page-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 16px;
		margin-bottom: 24px;
	}
	.ui-page-header-eyebrow {
		font-size: 11px;
		font-weight: 700;
		color: var(--accent);
		text-transform: uppercase;
		letter-spacing: 0.08em;
		margin-bottom: 6px;
	}
	.ui-page-header-title {
		font-size: 20px;
		font-weight: 700;
		color: var(--text-primary);
		letter-spacing: -0.02em;
		margin: 0;
		line-height: 1.25;
	}
	.ui-page-header-subtitle {
		font-size: 12.5px;
		color: var(--text-muted);
		margin: 4px 0 0;
	}
	.ui-page-header-actions {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-shrink: 0;
	}
</style>
```

- [ ] **Step 2: Create `SectionLabel.svelte`**

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		children: Snippet;
	}

	let { children }: Props = $props();
</script>

<div class="ui-section-label">{@render children()}</div>

<style>
	.ui-section-label {
		font-size: 11px;
		font-weight: 700;
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.07em;
		margin-bottom: 12px;
	}
</style>
```

- [ ] **Step 3: Verify**

Run: `cd frontend && npm run check`.

- [ ] **Step 4: Manual visual check**

Render `PageHeader` with an eyebrow, title, subtitle, and a `Button` in its actions slot, both themes.

- [ ] **Step 5: Commit**

```bash
cd frontend && git add src/lib/components/ui/PageHeader.svelte src/lib/components/ui/SectionLabel.svelte
git commit -m "feat(design-system): add PageHeader and SectionLabel components"
```

---

### Task 13: `HeroCard`

The gradient summary banner from the approved visual direction — built from `color-mix()` so it auto-adapts between light and dark theme rather than using hardcoded hex stops.

**Files:**
- Create: `frontend/src/lib/components/ui/HeroCard.svelte`

**Interfaces:**
- Produces: `HeroCard` with `eyebrow: string` (optional), `title: string`, `subtitle: string` (optional), and an `actions` snippet (optional).

- [ ] **Step 1: Create the component**

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		eyebrow?: string;
		title: string;
		subtitle?: string;
		actions?: Snippet;
	}

	let { eyebrow, title, subtitle, actions }: Props = $props();
</script>

<div class="ui-hero">
	<div class="ui-hero-content">
		{#if eyebrow}<div class="ui-hero-eyebrow">{eyebrow}</div>{/if}
		<h2 class="ui-hero-title">{title}</h2>
		{#if subtitle}<p class="ui-hero-subtitle">{subtitle}</p>{/if}
		{#if actions}
			<div class="ui-hero-actions">{@render actions()}</div>
		{/if}
	</div>
</div>

<style>
	.ui-hero {
		position: relative;
		overflow: hidden;
		border-radius: var(--radius-lg);
		padding: 22px 24px;
		background: linear-gradient(
			135deg,
			color-mix(in srgb, var(--accent) 14%, var(--bg-surface)) 0%,
			var(--bg-surface) 65%
		);
		border: 1px solid var(--border);
	}
	.ui-hero::before {
		content: '';
		position: absolute;
		right: -30px;
		top: -40px;
		width: 150px;
		height: 150px;
		border-radius: 50%;
		background: color-mix(in srgb, var(--accent) 18%, transparent);
	}
	.ui-hero-content { position: relative; }
	.ui-hero-eyebrow {
		font-size: 10.5px;
		font-weight: 700;
		color: var(--accent);
		text-transform: uppercase;
		letter-spacing: 0.08em;
	}
	.ui-hero-title {
		font-size: 21px;
		font-weight: 700;
		color: var(--text-primary);
		margin: 6px 0 0;
		letter-spacing: -0.02em;
	}
	.ui-hero-subtitle {
		font-size: 12px;
		color: var(--text-muted);
		margin: 4px 0 0;
	}
	.ui-hero-actions {
		display: flex;
		gap: 8px;
		margin-top: 16px;
	}
</style>
```

- [ ] **Step 2: Verify**

Run: `cd frontend && npm run check`.

- [ ] **Step 3: Manual visual check**

Render with an eyebrow, title, subtitle, and two `Button`s in actions, both themes — confirm the gradient/blob decoration is subtle (not overpowering) in both light and dark.

- [ ] **Step 4: Commit**

```bash
cd frontend && git add src/lib/components/ui/HeroCard.svelte
git commit -m "feat(design-system): add HeroCard component"
```

---

### Task 14: `StatCard`

**Files:**
- Create: `frontend/src/lib/components/ui/StatCard.svelte`

**Interfaces:**
- Consumes: `IconBadge` (Task 5).
- Produces: `StatCard` with `icon: Snippet` (the icon content, passed through to an internal `IconBadge`), `tone: 'blue' | 'green' | 'red' | 'yellow'` (default `'blue'`), `value: string | number`, `label: string`.

- [ ] **Step 1: Create the component**

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';
	import IconBadge from './IconBadge.svelte';

	interface Props {
		icon: Snippet;
		tone?: 'blue' | 'green' | 'red' | 'yellow';
		value: string | number;
		label: string;
	}

	let { icon, tone = 'blue', value, label }: Props = $props();
</script>

<div class="ui-stat-card">
	<IconBadge {tone} size={32}>{@render icon()}</IconBadge>
	<span class="ui-stat-value">{value}</span>
	<span class="ui-stat-label">{label}</span>
</div>

<style>
	.ui-stat-card {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		padding: 14px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.ui-stat-value {
		font-size: 19px;
		font-weight: 700;
		color: var(--text-primary);
		letter-spacing: -0.01em;
		font-variant-numeric: tabular-nums;
	}
	.ui-stat-label {
		font-size: 11px;
		color: var(--text-muted);
		margin-top: -4px;
	}
</style>
```

- [ ] **Step 2: Verify**

Run: `cd frontend && npm run check`.

- [ ] **Step 3: Manual visual check**

Render 3-4 `StatCard`s in a grid with different tones/icons, both themes.

- [ ] **Step 4: Commit**

```bash
cd frontend && git add src/lib/components/ui/StatCard.svelte
git commit -m "feat(design-system): add StatCard component"
```

---

### Task 15: `Card`

**Files:**
- Create: `frontend/src/lib/components/ui/Card.svelte`

**Interfaces:**
- Produces: `Card` with `padding: string` (default `'16px'`) and a `children` snippet. This is the generic rounded-surface container every other layout component (and most page migrations) wraps content in — the formalized replacement for every admin page's hand-rolled `.card { background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); ... }`.

- [ ] **Step 1: Create the component**

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		padding?: string;
		children: Snippet;
	}

	let { padding = '16px', children }: Props = $props();
</script>

<div class="ui-card" style="padding:{padding}">
	{@render children()}
</div>

<style>
	.ui-card {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
	}
</style>
```

- [ ] **Step 2: Verify**

Run: `cd frontend && npm run check`.

- [ ] **Step 3: Manual visual check**

Render nested content inside `Card`, both themes.

- [ ] **Step 4: Commit**

```bash
cd frontend && git add src/lib/components/ui/Card.svelte
git commit -m "feat(design-system): add Card component"
```

---

### Task 16: `EmptyState` and `InlineAlert`

Formalizes the already-documented `.empty-state` pattern and the `.err-msg`/`.ok-msg` pattern (extended to all four semantic tones).

**Files:**
- Create: `frontend/src/lib/components/ui/EmptyState.svelte`
- Create: `frontend/src/lib/components/ui/InlineAlert.svelte`

**Interfaces:**
- Produces: `EmptyState` with `icon: Snippet` (optional), `message: string`, `sub: string` (optional). `InlineAlert` with `tone: 'error' | 'success' | 'warning' | 'info'` and a `children` snippet (the message content).

- [ ] **Step 1: Create `EmptyState.svelte`**

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		icon?: Snippet;
		message: string;
		sub?: string;
	}

	let { icon, message, sub }: Props = $props();
</script>

<div class="ui-empty-state">
	{#if icon}<span class="ui-empty-icon">{@render icon()}</span>{/if}
	<span>{message}</span>
	{#if sub}<span class="ui-empty-sub">{sub}</span>{/if}
</div>

<style>
	.ui-empty-state {
		font-size: 13px;
		color: var(--text-dim);
		text-align: center;
		padding: 40px 24px;
		line-height: 1.6;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 6px;
	}
	:global(.ui-empty-icon) { color: var(--text-dim); }
	.ui-empty-sub { font-size: 11px; }
</style>
```

- [ ] **Step 2: Create `InlineAlert.svelte`**

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		tone: 'error' | 'success' | 'warning' | 'info';
		children: Snippet;
	}

	let { tone, children }: Props = $props();
</script>

<div class="ui-alert ui-alert--{tone}">{@render children()}</div>

<style>
	.ui-alert {
		font-size: 12px;
		padding: 9px 12px;
		border-radius: var(--radius-sm);
		border: 1px solid;
		line-height: 1.5;
	}
	.ui-alert--error {
		background: var(--accent-red-muted);
		border-color: color-mix(in srgb, var(--accent-red) 30%, transparent);
		color: var(--accent-red);
	}
	.ui-alert--success {
		background: var(--accent-green-muted);
		border-color: color-mix(in srgb, var(--accent-green) 30%, transparent);
		color: var(--accent-green);
	}
	.ui-alert--warning {
		background: var(--accent-yellow-muted);
		border-color: color-mix(in srgb, var(--accent-yellow) 30%, transparent);
		color: var(--accent-yellow);
	}
	.ui-alert--info {
		background: var(--accent-muted);
		border-color: color-mix(in srgb, var(--accent) 30%, transparent);
		color: var(--accent);
	}
</style>
```

- [ ] **Step 3: Verify**

Run: `cd frontend && npm run check`.

- [ ] **Step 4: Manual visual check**

Render `EmptyState` with and without an icon/sub, and all 4 `InlineAlert` tones, both themes.

- [ ] **Step 5: Commit**

```bash
cd frontend && git add src/lib/components/ui/EmptyState.svelte src/lib/components/ui/InlineAlert.svelte
git commit -m "feat(design-system): add EmptyState and InlineAlert components"
```

---

### Task 17: `Pagination` (Material 3 data-table footer)

The M3-specified pagination control: rows-per-page selector, "X–Y of Z" range label, first/previous/next/last icon buttons. Built standalone here so `DataTable` (Task 19) can use it internally, and so a page can use it alone if it ever needs pagination without the rest of `DataTable`.

**Files:**
- Create: `frontend/src/lib/components/ui/Pagination.svelte`

**Interfaces:**
- Consumes: `Button` (Task 3) for the icon buttons, `@lucide/svelte`'s `ChevronsLeft`, `ChevronLeft`, `ChevronRight`, `ChevronsRight`.
- Produces: `Pagination` with props `page: number` (0-indexed, current page), `pageSize: number`, `total: number`, `pageSizeOptions: number[]` (default `[10, 25, 50, 100]`), `onPageChange: (page: number) => void`, `onPageSizeChange: (size: number) => void`. This exact signature is what `DataTable` (Task 19) wires internally — later tasks never need to construct a `Pagination` directly unless a page uses it standalone.

- [ ] **Step 1: Create the component**

```svelte
<script lang="ts">
	import { ChevronsLeft, ChevronLeft, ChevronRight, ChevronsRight } from '@lucide/svelte';
	import Button from './Button.svelte';
	import Select from './Select.svelte';

	interface Props {
		page: number;
		pageSize: number;
		total: number;
		pageSizeOptions?: number[];
		onPageChange: (page: number) => void;
		onPageSizeChange: (size: number) => void;
	}

	let {
		page,
		pageSize,
		total,
		pageSizeOptions = [10, 25, 50, 100],
		onPageChange,
		onPageSizeChange
	}: Props = $props();

	let lastPage = $derived(Math.max(0, Math.ceil(total / pageSize) - 1));
	let rangeStart = $derived(total === 0 ? 0 : page * pageSize + 1);
	let rangeEnd = $derived(Math.min(total, (page + 1) * pageSize));

	let pageSizeOpts = $derived(pageSizeOptions.map((n) => ({ value: String(n), label: String(n) })));
	let pageSizeValue = $derived(String(pageSize));

	function handlePageSizeChange(v: string) {
		onPageSizeChange(Number(v));
	}
</script>

<div class="ui-pagination">
	<div class="ui-pagination-size">
		<span class="ui-pagination-size-label">Rows per page</span>
		<Select
			value={pageSizeValue}
			options={pageSizeOpts}
			id="ui-pagination-size-select"
		/>
	</div>
	<span class="ui-pagination-range">{rangeStart}–{rangeEnd} of {total}</span>
	<div class="ui-pagination-nav">
		<Button variant="ghost" size="icon" disabled={page === 0} onclick={() => onPageChange(0)}>
			<ChevronsLeft size={15} />
		</Button>
		<Button variant="ghost" size="icon" disabled={page === 0} onclick={() => onPageChange(page - 1)}>
			<ChevronLeft size={15} />
		</Button>
		<Button variant="ghost" size="icon" disabled={page >= lastPage} onclick={() => onPageChange(page + 1)}>
			<ChevronRight size={15} />
		</Button>
		<Button variant="ghost" size="icon" disabled={page >= lastPage} onclick={() => onPageChange(lastPage)}>
			<ChevronsRight size={15} />
		</Button>
	</div>
</div>

<script module>
	// Re-export the change handler signature so Select's onchange can call it —
	// Select itself has no onchange prop (Task 9 kept it minimal, bind:value
	// only), so Pagination listens for changes via a wrapper. See Step 2 note.
</script>

<style>
	.ui-pagination {
		display: flex;
		align-items: center;
		gap: 20px;
		padding: 10px 4px;
		font-size: 12px;
		color: var(--text-muted);
	}
	.ui-pagination-size { display: flex; align-items: center; gap: 8px; margin-left: auto; }
	.ui-pagination-size-label { white-space: nowrap; }
	.ui-pagination-size :global(select) { width: 68px; height: 30px; }
	.ui-pagination-range { font-variant-numeric: tabular-nums; white-space: nowrap; }
	.ui-pagination-nav { display: flex; gap: 2px; }
</style>
```

- [ ] **Step 2: Fix the page-size change wiring**

Task 9's `Select` only supports `bind:value`, with no `onchange` callback prop — but `Pagination` needs to call `onPageSizeChange` when the user picks a different page size, not just silently update a local binding. Rather than adding an `onchange` prop to `Select` (which no other consumer needs), wire this with a local bindable variable and a `$effect`:

Replace the `<Select value={pageSizeValue} ... />` line and remove the stray `<script module>` block above (it was a placeholder — delete it), and instead add to the main `<script>` block:

```typescript
	let pageSizeStr = $state(String(pageSize));

	$effect(() => {
		pageSizeStr = String(pageSize);
	});

	$effect(() => {
		const n = Number(pageSizeStr);
		if (n !== pageSize) onPageSizeChange(n);
	});
```

and change the template line to:

```svelte
<Select bind:value={pageSizeStr} options={pageSizeOpts} id="ui-pagination-size-select" />
```

Delete the now-unused `pageSizeValue` derived value and the `handlePageSizeChange` function from Step 1's draft — they're superseded by this.

- [ ] **Step 3: Verify**

Run: `cd frontend && npm run check` — expect 0 errors after Step 2's fix is applied (Step 1's draft alone would not compile cleanly against `Select`'s real interface, which is exactly why Step 2 exists).

- [ ] **Step 4: Manual visual check**

Render `Pagination` with `total: 237, pageSize: 25, page: 0` and click through next/prev/first/last, and change the rows-per-page selector — confirm the range label updates correctly and boundary buttons disable at page 0 and the last page. Both themes.

- [ ] **Step 5: Commit**

```bash
cd frontend && git add src/lib/components/ui/Pagination.svelte
git commit -m "feat(design-system): add Pagination component (Material 3 data-table footer)"
```

---

### Task 18: `ActivityList` and `ListRow`

The flat icon+title+meta+trailing-content row pattern from the approved reference, for simple lists that aren't dense tabular data (the audit log migration in Part B uses this).

**Files:**
- Create: `frontend/src/lib/components/ui/ListRow.svelte`
- Create: `frontend/src/lib/components/ui/ActivityList.svelte`

**Interfaces:**
- Consumes: `IconBadge` (Task 5).
- Produces: `ListRow` with `icon: Snippet`, `iconTone: 'blue' | 'green' | 'red' | 'yellow'` (default `'blue'`), `title: string`, `meta: string` (optional), and a `trailing` snippet (optional, right-aligned — a status badge, a value, an action button). `ActivityList` is a thin wrapper providing the card + divider-between-rows chrome around a set of `ListRow`s, via a `children` snippet.

- [ ] **Step 1: Create `ListRow.svelte`**

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';
	import IconBadge from './IconBadge.svelte';

	interface Props {
		icon: Snippet;
		iconTone?: 'blue' | 'green' | 'red' | 'yellow';
		title: string;
		meta?: string;
		trailing?: Snippet;
	}

	let { icon, iconTone = 'blue', title, meta, trailing }: Props = $props();
</script>

<div class="ui-list-row">
	<IconBadge tone={iconTone} size={30}>{@render icon()}</IconBadge>
	<div class="ui-list-row-text">
		<span class="ui-list-row-title">{title}</span>
		{#if meta}<span class="ui-list-row-meta">{meta}</span>{/if}
	</div>
	{#if trailing}
		<div class="ui-list-row-trailing">{@render trailing()}</div>
	{/if}
</div>

<style>
	.ui-list-row {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 10px 0;
		border-bottom: 1px solid var(--border);
	}
	.ui-list-row:last-child { border-bottom: none; }
	.ui-list-row-text {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.ui-list-row-title {
		font-size: 12.5px;
		font-weight: 600;
		color: var(--text-primary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.ui-list-row-meta {
		font-size: 10.5px;
		color: var(--text-dim);
		margin-top: 1px;
	}
	.ui-list-row-trailing { margin-left: auto; flex-shrink: 0; }
</style>
```

- [ ] **Step 2: Create `ActivityList.svelte`**

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';
	import Card from './Card.svelte';

	interface Props {
		children: Snippet;
	}

	let { children }: Props = $props();
</script>

<Card padding="6px 16px">
	{@render children()}
</Card>
```

(No own `<style>` block — it's a thin composition of `Card`, which already provides the surface/border/radius. Rows are passed as children, each a `ListRow`.)

- [ ] **Step 3: Verify**

Run: `cd frontend && npm run check`.

- [ ] **Step 4: Manual visual check**

Render an `ActivityList` with 3 `ListRow`s (different icon tones, one with a `Badge` in `trailing`, one with plain text in `trailing`), both themes.

- [ ] **Step 5: Commit**

```bash
cd frontend && git add src/lib/components/ui/ListRow.svelte src/lib/components/ui/ActivityList.svelte
git commit -m "feat(design-system): add ListRow and ActivityList components"
```

---

### Task 19: `DataTable` (client and server modes)

The highest-value component in this plan — consolidates the pattern duplicated across ~12+ admin pages. Supports two modes (see Global Constraints and the spec's "DataTable: client and server modes" section): `items` for an in-memory array, or `fetchPage` for a page/size/search-driven callback. Uses Svelte 5's `generics` attribute so it's typed per-consumer.

**Files:**
- Create: `frontend/src/lib/components/ui/DataTable.svelte`

**Interfaces:**
- Consumes: `SearchInput` (Task 11), `Pagination` (Task 17), `Skeleton` (Task 6), `EmptyState` (Task 16), `InlineAlert` (Task 16).
- Produces: `DataTable<T>` with props:
  - `columns: { key: string; label: string; width?: string }[]`
  - `rowKey: (row: T) => string`
  - `items?: T[]` — client mode
  - `fetchPage?: (params: { page: number; pageSize: number; search: string }) => Promise<{ rows: T[]; total: number }>` — server mode (exactly one of `items`/`fetchPage` is provided; the component throws a dev-time console error if both or neither are given, since that's a real caller bug worth surfacing loudly rather than silently picking one)
  - `searchable?: boolean` (default `true`) — if `false`, hides the search box entirely (e.g. for a table whose columns aren't meaningfully text-searchable)
  - `searchFields?: (keyof T)[]` — in client mode, which fields the built-in search filters against (required if `items` is given and `searchable` is `true`; ignored in server mode, where the consumer's `fetchPage` implements search itself)
  - `pageSize?: number` (default `25`)
  - `emptyMessage?: string` (default `'No results.'`)
  - `row: Snippet<[T]>` — renders one row's cells, given the row data (the consumer is responsible for wrapping their cell content per-column; `DataTable` provides the `<tr>`/layout, not per-cell markup, since columns' content varies too much per page to genericize further without losing real flexibility)

- [ ] **Step 1: Create the component**

```svelte
<script lang="ts" generics="T">
	import type { Snippet } from 'svelte';
	import SearchInput from './SearchInput.svelte';
	import Pagination from './Pagination.svelte';
	import Skeleton from './Skeleton.svelte';
	import EmptyState from './EmptyState.svelte';
	import InlineAlert from './InlineAlert.svelte';

	interface Column {
		key: string;
		label: string;
		width?: string;
	}

	interface FetchPageParams {
		page: number;
		pageSize: number;
		search: string;
	}

	interface FetchPageResult<T> {
		rows: T[];
		total: number;
	}

	interface Props {
		columns: Column[];
		rowKey: (row: T) => string;
		items?: T[];
		fetchPage?: (params: FetchPageParams) => Promise<FetchPageResult<T>>;
		searchable?: boolean;
		searchFields?: (keyof T)[];
		pageSize?: number;
		emptyMessage?: string;
		row: Snippet<[T]>;
	}

	let {
		columns,
		rowKey,
		items,
		fetchPage,
		searchable = true,
		searchFields,
		pageSize = 25,
		emptyMessage = 'No results.',
		row
	}: Props = $props();

	if (import.meta.env.DEV) {
		if ((items && fetchPage) || (!items && !fetchPage)) {
			console.error('DataTable: pass exactly one of `items` (client mode) or `fetchPage` (server mode), not both or neither.');
		}
		if (items && searchable && !searchFields) {
			console.error('DataTable: `searchFields` is required in client mode when `searchable` is true.');
		}
	}

	let search = $state('');
	let page = $state(0);
	let currentPageSize = $state(pageSize);

	// Server-mode state
	let serverRows = $state<T[]>([]);
	let serverTotal = $state(0);
	let loading = $state(false);
	let error = $state('');

	async function loadServerPage() {
		if (!fetchPage) return;
		loading = true;
		error = '';
		try {
			const result = await fetchPage({ page, pageSize: currentPageSize, search });
			serverRows = result.rows;
			serverTotal = result.total;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load data.';
		}
		loading = false;
	}

	$effect(() => {
		// Re-fetch whenever page, page size, or (debounced) search changes.
		page; currentPageSize; search;
		if (fetchPage) loadServerPage();
	});

	// Client-mode derived state
	let clientFiltered = $derived.by(() => {
		if (!items) return [];
		if (!search || !searchFields) return items;
		const q = search.toLowerCase();
		return items.filter((it) =>
			searchFields!.some((f) => String(it[f] ?? '').toLowerCase().includes(q))
		);
	});
	let clientTotal = $derived(items ? clientFiltered.length : 0);
	let clientPageRows = $derived(
		items ? clientFiltered.slice(page * currentPageSize, (page + 1) * currentPageSize) : []
	);

	// Reset to page 0 whenever the search term changes, in either mode.
	$effect(() => {
		search;
		page = 0;
	});

	let displayRows = $derived(items ? clientPageRows : serverRows);
	let displayTotal = $derived(items ? clientTotal : serverTotal);
	let isEmpty = $derived(!loading && !error && displayRows.length === 0);
</script>

<div class="ui-data-table">
	{#if searchable}
		<div class="ui-data-table-toolbar">
			<SearchInput bind:value={search} />
		</div>
	{/if}

	{#if loading && displayRows.length === 0}
		<div class="ui-data-table-skeleton">
			{#each Array(5) as _}
				<Skeleton variant="row" />
			{/each}
		</div>
	{:else if error}
		<InlineAlert tone="error">{error}</InlineAlert>
	{:else if isEmpty}
		<EmptyState message={emptyMessage} />
	{:else}
		<div class="ui-data-table-scroll">
			<table class="ui-data-table-el">
				<thead>
					<tr>
						{#each columns as col (col.key)}
							<th style={col.width ? `width:${col.width}` : undefined}>{col.label}</th>
						{/each}
					</tr>
				</thead>
				<tbody>
					{#each displayRows as r (rowKey(r))}
						{@render row(r)}
					{/each}
				</tbody>
			</table>
		</div>
		<Pagination
			{page}
			pageSize={currentPageSize}
			total={displayTotal}
			onPageChange={(p) => (page = p)}
			onPageSizeChange={(s) => (currentPageSize = s)}
		/>
	{/if}
</div>

<style>
	.ui-data-table {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		overflow: hidden;
	}
	.ui-data-table-toolbar {
		display: flex;
		justify-content: flex-end;
		padding: 14px 16px 6px;
	}
	.ui-data-table-toolbar :global(.ui-search) { max-width: 220px; }
	.ui-data-table-skeleton {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px 16px;
	}
	.ui-data-table-scroll { overflow-x: auto; }
	.ui-data-table-el {
		width: 100%;
		border-collapse: collapse;
		font-size: 12.5px;
	}
	.ui-data-table-el thead th {
		text-align: left;
		padding: 9px 16px;
		background: var(--bg-elevated);
		border-bottom: 1px solid var(--border);
		font-size: 10.5px;
		font-weight: 700;
		color: var(--text-dim);
		text-transform: uppercase;
		letter-spacing: 0.06em;
		white-space: nowrap;
	}
	.ui-data-table-el :global(tbody tr) {
		border-bottom: 1px solid var(--border);
	}
	.ui-data-table-el :global(tbody tr:last-child) {
		border-bottom: none;
	}
	.ui-data-table-el :global(tbody tr:hover) {
		background: var(--bg-hover);
	}
	.ui-data-table-el :global(td) {
		padding: 10px 16px;
		color: var(--text-secondary);
	}
	:global(.ui-data-table) :global(.ui-pagination) {
		padding: 10px 16px;
		border-top: 1px solid var(--border);
	}
</style>
```

- [ ] **Step 2: Verify**

Run: `cd frontend && npm run check` — this is the most complex component in the library; pay particular attention to the `generics="T"` script attribute compiling correctly and the `Snippet<[T]>` type for `row`.

- [ ] **Step 3: Manual visual check — client mode**

In a scratch route, render `DataTable` with `items` set to ~40 fake rows (e.g. `{ id: string, name: string, status: string }[]`), `searchFields={['name']}`, and a `row` snippet rendering `<tr><td>{r.name}</td><td>{r.status}</td></tr>`. Confirm: search filters correctly, pagination works and resets to page 0 on search, empty state shows when a search matches nothing. Both themes.

- [ ] **Step 4: Manual visual check — server mode**

In the same scratch route, render a second `DataTable` with a `fetchPage` that simulates a network call (`await new Promise(r => setTimeout(r, 300))` then returns a slice of the same fake data + a total). Confirm: the skeleton shows during the simulated delay, pagination/search trigger new `fetchPage` calls, and the loading state doesn't flash empty/error incorrectly between pages. Delete the scratch route when done with both checks.

- [ ] **Step 5: Commit**

```bash
cd frontend && git add src/lib/components/ui/DataTable.svelte
git commit -m "feat(design-system): add DataTable component with client and server modes"
```

---

### Task 20: `DonutChart`

Lightweight inline SVG, no charting library — matches the reference direction's data-viz style (seen in the approved mockup's container-status donut).

**Files:**
- Create: `frontend/src/lib/components/ui/DonutChart.svelte`

**Interfaces:**
- Produces: `DonutChart` with `segments: { label: string; value: number; color: string }[]` and `size: number` (default `120`). `color` is passed as a literal CSS color value by the consumer (typically `var(--accent-green)` etc.) rather than a tone name, since a donut can have more segments than the fixed tone palette covers.

- [ ] **Step 1: Create the component**

```svelte
<script lang="ts">
	interface Segment {
		label: string;
		value: number;
		color: string;
	}

	interface Props {
		segments: Segment[];
		size?: number;
	}

	let { segments, size = 120 }: Props = $props();

	const RADIUS = 15.9; // circumference works out to ~100 for easy percentage math
	let total = $derived(segments.reduce((sum, s) => sum + s.value, 0));

	let arcs = $derived.by(() => {
		let offset = 0;
		return segments.map((s) => {
			const pct = total > 0 ? (s.value / total) * 100 : 0;
			const arc = { ...s, pct, dasharray: `${pct} ${100 - pct}`, dashoffset: 25 - offset };
			offset += pct;
			return arc;
		});
	});
</script>

<div class="ui-donut-wrap" style="width:{size}px;height:{size}px">
	<svg viewBox="0 0 42 42" width={size} height={size}>
		<circle cx="21" cy="21" r={RADIUS} fill="transparent" stroke="var(--border)" stroke-width="6" />
		{#each arcs as arc}
			<circle
				cx="21" cy="21" r={RADIUS}
				fill="transparent"
				stroke={arc.color}
				stroke-width="6"
				stroke-dasharray={arc.dasharray}
				stroke-dashoffset={arc.dashoffset}
			/>
		{/each}
	</svg>
</div>

<style>
	.ui-donut-wrap { display: inline-flex; }
</style>
```

- [ ] **Step 2: Verify**

Run: `cd frontend && npm run check`.

- [ ] **Step 3: Manual visual check**

Render with 3 segments (e.g. running/stopped/failed using `var(--accent-green)`/`var(--accent-yellow)`/`var(--accent-red)`), confirm the arcs sum correctly and don't overlap/gap incorrectly. Both themes.

- [ ] **Step 4: Commit**

```bash
cd frontend && git add src/lib/components/ui/DonutChart.svelte
git commit -m "feat(design-system): add DonutChart component"
```

---

### Task 21: `BarChart`

**Files:**
- Create: `frontend/src/lib/components/ui/BarChart.svelte`

**Interfaces:**
- Produces: `BarChart` with `data: { label: string; value: number }[]`, `height: number` (default `120`), `color: string` (default `'var(--accent)'`).

- [ ] **Step 1: Create the component**

```svelte
<script lang="ts">
	interface Point {
		label: string;
		value: number;
	}

	interface Props {
		data: Point[];
		height?: number;
		color?: string;
	}

	let { data, height = 120, color = 'var(--accent)' }: Props = $props();

	let max = $derived(Math.max(1, ...data.map((d) => d.value)));
</script>

<div class="ui-bar-chart" style="height:{height}px">
	{#each data as d}
		<div class="ui-bar-col" title="{d.label}: {d.value}">
			<div class="ui-bar" style="height:{(d.value / max) * 100}%;background:{color}"></div>
		</div>
	{/each}
</div>

<style>
	.ui-bar-chart {
		display: flex;
		align-items: flex-end;
		gap: 3px;
	}
	.ui-bar-col {
		flex: 1;
		height: 100%;
		display: flex;
		align-items: flex-end;
		min-width: 0;
	}
	.ui-bar {
		width: 100%;
		border-radius: 2px 2px 0 0;
		min-height: 2px;
		transition: height var(--transition-normal);
	}
</style>
```

- [ ] **Step 2: Verify**

Run: `cd frontend && npm run check`.

- [ ] **Step 3: Manual visual check**

Render with ~15 data points of varying magnitude, confirm bars scale correctly relative to the max value. Both themes.

- [ ] **Step 4: Commit**

```bash
cd frontend && git add src/lib/components/ui/BarChart.svelte
git commit -m "feat(design-system): add BarChart component"
```

---

### Task 22: `AreaChart`

**Files:**
- Create: `frontend/src/lib/components/ui/AreaChart.svelte`

**Interfaces:**
- Produces: `AreaChart` with `data: { label: string; value: number }[]`, `height: number` (default `120`), `color: string` (default `'var(--accent)'`).

- [ ] **Step 1: Create the component**

```svelte
<script lang="ts">
	interface Point {
		label: string;
		value: number;
	}

	interface Props {
		data: Point[];
		height?: number;
		color?: string;
	}

	let { data, height = 120, color = 'var(--accent)' }: Props = $props();

	const WIDTH = 300;
	let max = $derived(Math.max(1, ...data.map((d) => d.value)));

	let points = $derived(
		data.map((d, i) => {
			const x = data.length > 1 ? (i / (data.length - 1)) * WIDTH : 0;
			const y = height - (d.value / max) * height;
			return `${x.toFixed(1)},${y.toFixed(1)}`;
		})
	);

	let linePath = $derived(`M ${points.join(' L ')}`);
	let areaPath = $derived(`${linePath} L ${WIDTH},${height} L 0,${height} Z`);
</script>

<svg class="ui-area-chart" viewBox="0 0 {WIDTH} {height}" preserveAspectRatio="none">
	<path d={areaPath} fill={color} opacity="0.12" />
	<path d={linePath} fill="none" stroke={color} stroke-width="2" stroke-linejoin="round" stroke-linecap="round" />
</svg>

<style>
	.ui-area-chart {
		width: 100%;
		display: block;
	}
</style>
```

- [ ] **Step 2: Verify**

Run: `cd frontend && npm run check`.

- [ ] **Step 3: Manual visual check**

Render with ~10 data points, confirm the smooth line + filled area render correctly and scale to the container width. Both themes.

- [ ] **Step 4: Commit**

```bash
cd frontend && git add src/lib/components/ui/AreaChart.svelte
git commit -m "feat(design-system): add AreaChart component"
```

---

### Task 23: `ProgressBar`

**Files:**
- Create: `frontend/src/lib/components/ui/ProgressBar.svelte`

**Interfaces:**
- Produces: `ProgressBar` with `value: number` (0-100), `tone: 'blue' | 'green' | 'red' | 'yellow'` (default `'blue'`).

- [ ] **Step 1: Create the component**

```svelte
<script lang="ts">
	interface Props {
		value: number;
		tone?: 'blue' | 'green' | 'red' | 'yellow';
	}

	let { value, tone = 'blue' }: Props = $props();

	let clamped = $derived(Math.max(0, Math.min(100, value)));
</script>

<div class="ui-progress" role="progressbar" aria-valuenow={clamped} aria-valuemin={0} aria-valuemax={100}>
	<div class="ui-progress-fill ui-progress-fill--{tone}" style="width:{clamped}%"></div>
</div>

<style>
	.ui-progress {
		width: 100%;
		height: 6px;
		border-radius: 999px;
		background: var(--border);
		overflow: hidden;
	}
	.ui-progress-fill {
		height: 100%;
		border-radius: 999px;
		transition: width var(--transition-normal);
	}
	.ui-progress-fill--blue   { background: var(--accent); }
	.ui-progress-fill--green  { background: var(--accent-green); }
	.ui-progress-fill--red    { background: var(--accent-red); }
	.ui-progress-fill--yellow { background: var(--accent-yellow); }
</style>
```

- [ ] **Step 2: Verify**

Run: `cd frontend && npm run check`.

- [ ] **Step 3: Manual visual check**

Render at a few values (0, 45, 100) and all 4 tones. Both themes.

- [ ] **Step 4: Commit**

```bash
cd frontend && git add src/lib/components/ui/ProgressBar.svelte
git commit -m "feat(design-system): add ProgressBar component"
```

---

### Task 24: `Modal` and `ConfirmDialog`

Replaces the four admin pages' native `confirm()` calls and the five pages' hand-rolled modal markup. Follows M3's modal anatomy (scrim, container, header, body, action row) and reuses the confirmation-modal pattern already documented for the main app (see the design-system memory's "Confirmation Modal (Danger Delete)" section) rather than inventing new interaction patterns.

**Files:**
- Create: `frontend/src/lib/components/ui/Modal.svelte`
- Create: `frontend/src/lib/components/ui/ConfirmDialog.svelte`

**Interfaces:**
- Consumes: `Button` (Task 3), `TextField` (Task 8).
- Produces: `Modal` with `open: boolean` (`$bindable`), `title: string`, a `children` snippet (body), and a `footer` snippet (optional, action buttons — if omitted, no footer row renders). `ConfirmDialog` with `open: boolean` (`$bindable`), `title: string`, `message: string`, `confirmLabel: string` (default `'Delete'`), `danger: boolean` (default `true`), `confirmText: string` (optional — if set, requires the user to type this exact text before the confirm button enables, matching the existing "type to confirm" delete pattern), `onConfirm: () => void | Promise<void>`.

- [ ] **Step 1: Create `Modal.svelte`**

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		open: boolean;
		title: string;
		children: Snippet;
		footer?: Snippet;
	}

	let { open = $bindable(), title, children, footer }: Props = $props();

	function close() {
		open = false;
	}

	function onBackdropKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') close();
	}
</script>

{#if open}
	<div class="ui-modal-scrim" role="presentation" onclick={close}>
		<div
			class="ui-modal"
			role="dialog"
			aria-modal="true"
			aria-labelledby="ui-modal-title"
			tabindex="-1"
			onclick={(e) => e.stopPropagation()}
			onkeydown={onBackdropKeydown}
		>
			<div class="ui-modal-header">
				<span id="ui-modal-title">{title}</span>
			</div>
			<div class="ui-modal-body">
				{@render children()}
			</div>
			{#if footer}
				<div class="ui-modal-footer">
					{@render footer()}
				</div>
			{/if}
		</div>
	</div>
{/if}

<style>
	.ui-modal-scrim {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.55);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 500;
		padding: 16px;
	}
	.ui-modal {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		width: 100%;
		max-width: 420px;
		box-shadow: var(--shadow-lg);
	}
	.ui-modal-header {
		padding: 14px 16px;
		border-bottom: 1px solid var(--border);
		font-size: 13px;
		font-weight: 700;
		color: var(--text-primary);
	}
	.ui-modal-body {
		padding: 16px;
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
	.ui-modal-footer {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		padding: 12px 16px;
		border-top: 1px solid var(--border);
	}
</style>
```

- [ ] **Step 2: Create `ConfirmDialog.svelte`**

```svelte
<script lang="ts">
	import Modal from './Modal.svelte';
	import Button from './Button.svelte';
	import TextField from './TextField.svelte';

	interface Props {
		open: boolean;
		title: string;
		message: string;
		confirmLabel?: string;
		danger?: boolean;
		confirmText?: string;
		onConfirm: () => void | Promise<void>;
	}

	let {
		open = $bindable(),
		title,
		message,
		confirmLabel = 'Delete',
		danger = true,
		confirmText,
		onConfirm
	}: Props = $props();

	let typedConfirm = $state('');
	let confirming = $state(false);

	let canConfirm = $derived(!confirmText || typedConfirm === confirmText);

	async function handleConfirm() {
		if (!canConfirm || confirming) return;
		confirming = true;
		await onConfirm();
		confirming = false;
		typedConfirm = '';
		open = false;
	}

	function handleCancel() {
		typedConfirm = '';
		open = false;
	}
</script>

<Modal bind:open {title}>
	<p class="ui-confirm-message">{message}</p>
	{#if confirmText}
		<div class="ui-confirm-type-field">
			<label class="ui-confirm-type-label" for="ui-confirm-type-input">
				Type <code class="ui-confirm-code">{confirmText}</code> to confirm
			</label>
			<TextField id="ui-confirm-type-input" bind:value={typedConfirm} />
		</div>
	{/if}
	{#snippet footer()}
		<Button variant="ghost" onclick={handleCancel}>Cancel</Button>
		<Button variant={danger ? 'danger' : 'primary'} disabled={!canConfirm || confirming} onclick={handleConfirm}>
			{confirming ? 'Working…' : confirmLabel}
		</Button>
	{/snippet}
</Modal>

<style>
	.ui-confirm-message {
		font-size: 12.5px;
		color: var(--text-primary);
		line-height: 1.5;
		margin: 0;
	}
	.ui-confirm-type-field { display: flex; flex-direction: column; gap: 6px; }
	.ui-confirm-type-label { font-size: 11px; color: var(--text-muted); }
	.ui-confirm-code {
		font-family: var(--font-mono);
		font-size: 11px;
		background: var(--bg-elevated);
		padding: 1px 5px;
		border-radius: 3px;
		border: 1px solid var(--border);
	}
</style>
```

Note: `Modal`'s `footer` prop type is `Snippet`, and `ConfirmDialog` passes it via the `{#snippet footer()}...{/snippet}` block syntax — this is the correct Svelte 5 way to pass a named snippet as a prop, matching how `children` is passed implicitly by wrapping content in a component's tags.

- [ ] **Step 3: Verify**

Run: `cd frontend && npm run check`.

- [ ] **Step 4: Manual visual check**

Render a `ConfirmDialog` both with and without `confirmText` set — confirm the confirm button stays disabled until the typed text matches exactly when `confirmText` is set, and confirm Escape/backdrop-click both close it. Both themes.

- [ ] **Step 5: Commit**

```bash
cd frontend && git add src/lib/components/ui/Modal.svelte src/lib/components/ui/ConfirmDialog.svelte
git commit -m "feat(design-system): add Modal and ConfirmDialog components"
```

---

### Task 25: `Dropdown`

The checkmark-style multi-item selection list from the approved reference (its category filter).

**Files:**
- Create: `frontend/src/lib/components/ui/Dropdown.svelte`

**Interfaces:**
- Consumes: `@lucide/svelte`'s `Check`.
- Produces: `Dropdown` with `options: { value: string; label: string }[]`, `value: string` (`$bindable`, single-select), `trigger: Snippet` (the clickable element that opens it — typically a `Button`).

- [ ] **Step 1: Create the component**

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';
	import { Check } from '@lucide/svelte';

	interface Option {
		value: string;
		label: string;
	}

	interface Props {
		options: Option[];
		value: string;
		trigger: Snippet;
	}

	let { options, value = $bindable(), trigger }: Props = $props();

	let open = $state(false);

	function select(v: string) {
		value = v;
		open = false;
	}

	function onWindowClick() {
		open = false;
	}
</script>

<svelte:window onclick={onWindowClick} />

<div class="ui-dropdown">
	<button type="button" class="ui-dropdown-trigger" onclick={(e) => { e.stopPropagation(); open = !open; }}>
		{@render trigger()}
	</button>
	{#if open}
		<div class="ui-dropdown-menu" onclick={(e) => e.stopPropagation()} role="menu">
			{#each options as opt (opt.value)}
				<button type="button" class="ui-dropdown-item" onclick={() => select(opt.value)} role="menuitem">
					<span class="ui-dropdown-check">{#if opt.value === value}<Check size={13} />{/if}</span>
					{opt.label}
				</button>
			{/each}
		</div>
	{/if}
</div>

<style>
	.ui-dropdown { position: relative; display: inline-block; }
	.ui-dropdown-trigger { background: none; border: none; padding: 0; cursor: pointer; }
	.ui-dropdown-menu {
		position: absolute;
		top: calc(100% + 6px);
		left: 0;
		min-width: 180px;
		max-height: 280px;
		overflow-y: auto;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-md);
		z-index: 50;
		padding: 4px;
	}
	.ui-dropdown-item {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 7px 9px;
		background: none;
		border: none;
		border-radius: var(--radius-sm);
		font-size: 12.5px;
		color: var(--text-secondary);
		text-align: left;
		cursor: pointer;
	}
	.ui-dropdown-item:hover { background: var(--bg-hover); color: var(--text-primary); }
	.ui-dropdown-check { width: 13px; display: flex; color: var(--accent); flex-shrink: 0; }
</style>
```

- [ ] **Step 2: Verify**

Run: `cd frontend && npm run check`.

- [ ] **Step 3: Manual visual check**

Render with a `Button` as `trigger` and ~6 options, confirm the checkmark shows on the currently-selected option, and clicking outside the menu closes it. Both themes.

- [ ] **Step 4: Commit**

```bash
cd frontend && git add src/lib/components/ui/Dropdown.svelte
git commit -m "feat(design-system): add Dropdown component"
```

---

### Task 26: `NavRail`

The icon rail half of the new admin nav shell — see the spec's "Navigation: new NavRail + NavDrawer" section and the interactive prototype already approved by the user (`.superpowers/brainstorm/84350-1790624694/content/nav-rail-drawer.html`).

**Files:**
- Create: `frontend/src/lib/components/ui/NavRail.svelte`

**Interfaces:**
- Consumes: `@lucide/svelte`'s `Anchor` (the existing Shipyard brand mark, unchanged from `IconSidebar.svelte`).
- Produces: `NavRail` with `groups: { key: string; icon: Snippet; label: string }[]`, `activeGroup: string | null` (`$bindable` — which group's drawer, if any, is currently open), `onSelectGroup: (key: string) => void`.

- [ ] **Step 1: Create the component**

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';
	import { Anchor } from '@lucide/svelte';

	interface Group {
		key: string;
		icon: Snippet;
		label: string;
	}

	interface Props {
		groups: Group[];
		activeGroup: string | null;
		onSelectGroup: (key: string) => void;
	}

	let { groups, activeGroup = $bindable(), onSelectGroup }: Props = $props();

	function handleClick(key: string) {
		onSelectGroup(activeGroup === key ? null : key);
	}
</script>

<aside class="ui-nav-rail">
	<div class="ui-nav-rail-logo">
		<Anchor size={16} strokeWidth={2.5} />
	</div>
	<nav class="ui-nav-rail-items">
		{#each groups as g (g.key)}
			<div class="ui-nav-rail-group">
				<button
					type="button"
					class="ui-nav-rail-btn"
					class:ui-nav-rail-btn--active={activeGroup === g.key}
					onclick={() => handleClick(g.key)}
					aria-label={g.label}
					aria-expanded={activeGroup === g.key}
				>
					{@render g.icon()}
				</button>
				<span class="ui-nav-rail-label">{g.label}</span>
			</div>
		{/each}
	</nav>
</aside>

<style>
	.ui-nav-rail {
		width: 60px;
		flex-shrink: 0;
		background: var(--bg-surface);
		border-right: 1px solid var(--border);
		display: flex;
		flex-direction: column;
		align-items: center;
		padding: 14px 0;
		gap: 4px;
		height: 100vh;
		position: relative;
		z-index: 5;
	}
	.ui-nav-rail-logo {
		width: 30px;
		height: 30px;
		border-radius: var(--radius-sm);
		background: var(--accent);
		color: #fff;
		margin-bottom: 14px;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}
	.ui-nav-rail-items {
		display: flex;
		flex-direction: column;
		gap: 6px;
		width: 100%;
		align-items: center;
	}
	.ui-nav-rail-group {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 2px;
	}
	.ui-nav-rail-btn {
		width: 40px;
		height: 40px;
		border-radius: var(--radius-md);
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--text-muted);
		background: none;
		border: none;
		cursor: pointer;
		transition: background var(--transition-fast), color var(--transition-fast);
	}
	.ui-nav-rail-btn:hover { background: var(--bg-hover); color: var(--accent); }
	.ui-nav-rail-btn--active { background: var(--accent-muted); color: var(--accent); }
	.ui-nav-rail-label {
		font-size: 8.5px;
		font-weight: 600;
		color: var(--text-dim);
	}
</style>
```

- [ ] **Step 2: Verify**

Run: `cd frontend && npm run check`.

- [ ] **Step 3: Manual visual check**

Render with 3 groups (icons from `@lucide/svelte`), click each and confirm the active state highlights correctly. Both themes. (`NavDrawer`, the piece that actually shows content when a group is active, is Task 27 — at this point clicking just toggles `activeGroup`, nothing visibly opens yet; that's expected.)

- [ ] **Step 4: Commit**

```bash
cd frontend && git add src/lib/components/ui/NavRail.svelte
git commit -m "feat(design-system): add NavRail component"
```

---

### Task 27: `NavDrawer`

The expanding overlay half of the nav shell — shows the active group's full labeled sub-items.

**Files:**
- Create: `frontend/src/lib/components/ui/NavDrawer.svelte`

**Interfaces:**
- Consumes: nothing from earlier UI tasks (plain markup + tokens).
- Produces: `NavDrawer` with `open: boolean`, `title: string`, `items: { href: string; icon: Snippet; label: string; active: boolean }[]`, `onNavigate: () => void` (called when any item is clicked — the consumer uses this to close the drawer after navigation, matching the approved prototype's "stays open until dismissed... or navigate to an item" behavior).

- [ ] **Step 1: Create the component**

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Item {
		href: string;
		icon: Snippet;
		label: string;
		active: boolean;
	}

	interface Props {
		open: boolean;
		title: string;
		items: Item[];
		onNavigate: () => void;
	}

	let { open, title, items, onNavigate }: Props = $props();
</script>

<div class="ui-nav-drawer" class:ui-nav-drawer--open={open}>
	<div class="ui-nav-drawer-inner">
		<div class="ui-nav-drawer-title">{title}</div>
		{#each items as item (item.href)}
			<a
				href={item.href}
				class="ui-nav-drawer-item"
				class:ui-nav-drawer-item--active={item.active}
				onclick={onNavigate}
			>
				<span class="ui-nav-drawer-icon">{@render item.icon()}</span>
				{item.label}
			</a>
		{/each}
	</div>
</div>

<style>
	.ui-nav-drawer {
		position: absolute;
		top: 0;
		left: 60px;
		bottom: 0;
		width: 0;
		background: var(--bg-surface);
		border-right: 1px solid var(--border);
		overflow: hidden;
		transition: width var(--transition-normal);
		z-index: 4;
		box-shadow: var(--shadow-lg);
	}
	.ui-nav-drawer--open { width: 230px; }
	.ui-nav-drawer-inner {
		width: 230px;
		padding: 18px 12px;
	}
	.ui-nav-drawer-title {
		font-size: 11px;
		font-weight: 700;
		color: var(--text-primary);
		text-transform: uppercase;
		letter-spacing: 0.06em;
		padding: 4px 10px 10px;
	}
	.ui-nav-drawer-item {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 10px;
		border-radius: var(--radius-md);
		font-size: 13px;
		font-weight: 500;
		color: var(--text-secondary);
		text-decoration: none;
		cursor: pointer;
	}
	.ui-nav-drawer-item:hover { background: var(--bg-hover); color: var(--text-primary); }
	.ui-nav-drawer-item--active { background: var(--accent-muted); color: var(--accent); font-weight: 600; }
	.ui-nav-drawer-icon {
		width: 16px;
		display: flex;
		justify-content: center;
		opacity: 0.85;
	}
</style>
```

- [ ] **Step 2: Verify**

Run: `cd frontend && npm run check`.

- [ ] **Step 3: Manual visual check**

Combine with `NavRail` from Task 26 in a scratch route (parent component holds `activeGroup` state, passes `open={activeGroup === 'platform'}` etc. to a `NavDrawer` per group, or one `NavDrawer` whose `items`/`title` swap based on `activeGroup`) — click through and confirm it matches the already-approved interactive prototype's behavior (click-to-open, overlays content, stays open until dismissed or an item is clicked). Both themes.

- [ ] **Step 4: Commit**

```bash
cd frontend && git add src/lib/components/ui/NavDrawer.svelte
git commit -m "feat(design-system): add NavDrawer component"
```

---

### Task 28: Barrel export

**Files:**
- Create: `frontend/src/lib/components/ui/index.ts`

**Interfaces:**
- Produces: a single import surface, `import { Button, Badge, ... } from '$lib/components/ui';`, used by every page-migration task in Part B instead of importing each component from its own file path.

- [ ] **Step 1: Create the barrel file**

```typescript
export { default as Button } from './Button.svelte';
export { default as Badge } from './Badge.svelte';
export { default as StatusDot } from './StatusDot.svelte';
export { default as Avatar } from './Avatar.svelte';
export { default as IconBadge } from './IconBadge.svelte';
export { default as Spinner } from './Spinner.svelte';
export { default as Skeleton } from './Skeleton.svelte';
export { default as Divider } from './Divider.svelte';
export { default as Tooltip } from './Tooltip.svelte';
export { default as TextField } from './TextField.svelte';
export { default as Textarea } from './Textarea.svelte';
export { default as Select } from './Select.svelte';
export { default as Toggle } from './Toggle.svelte';
export { default as Checkbox } from './Checkbox.svelte';
export { default as RadioGroup } from './RadioGroup.svelte';
export { default as FormField } from './FormField.svelte';
export { default as SearchInput } from './SearchInput.svelte';
export { default as PageHeader } from './PageHeader.svelte';
export { default as SectionLabel } from './SectionLabel.svelte';
export { default as HeroCard } from './HeroCard.svelte';
export { default as StatCard } from './StatCard.svelte';
export { default as Card } from './Card.svelte';
export { default as EmptyState } from './EmptyState.svelte';
export { default as InlineAlert } from './InlineAlert.svelte';
export { default as Pagination } from './Pagination.svelte';
export { default as ListRow } from './ListRow.svelte';
export { default as ActivityList } from './ActivityList.svelte';
export { default as DataTable } from './DataTable.svelte';
export { default as DonutChart } from './DonutChart.svelte';
export { default as BarChart } from './BarChart.svelte';
export { default as AreaChart } from './AreaChart.svelte';
export { default as ProgressBar } from './ProgressBar.svelte';
export { default as Modal } from './Modal.svelte';
export { default as ConfirmDialog } from './ConfirmDialog.svelte';
export { default as Dropdown } from './Dropdown.svelte';
export { default as NavRail } from './NavRail.svelte';
export { default as NavDrawer } from './NavDrawer.svelte';
```

- [ ] **Step 2: Verify**

Run: `cd frontend && npm run check` — confirms every one of the 34 files referenced actually exists with a default export by this point in the plan (Tasks 1-27 create all of them; Task 1 was `Toast`, which is intentionally *not* in this barrel since it's a page-level singleton mounted once in the root layout, not a per-use component like the other 34).

- [ ] **Step 3: Commit**

```bash
cd frontend && git add src/lib/components/ui/index.ts
git commit -m "feat(design-system): add ui component barrel export"
```

---

### Task 29: Wire `NavRail`/`NavDrawer` into admin's layout shell

Replaces admin's old always-expanded 220px sidebar with the new rail+drawer shell, using the exact same nav groups/links already defined in `admin/+layout.svelte`'s `navGroups` — this task changes *how* the nav renders, not *what* it links to.

**Files:**
- Modify: `frontend/src/routes/admin/+layout.svelte`

**Interfaces:**
- Consumes: `NavRail` (Task 26), `NavDrawer` (Task 27), and `@lucide/svelte` icons (this task also does the icon-inlining cleanup for the nav specifically — the rest of the raw-SVG cleanup across page *content* happens per-page in Part B).

- [ ] **Step 1: Map admin's 3 existing nav groups onto `@lucide/svelte` icons**

Read the current `navGroups` array in `admin/+layout.svelte` (7 items in "Platform", 7 in "Infrastructure", 6 in "Services" — each currently an inline SVG `d` path string). Replace the whole `navGroups` structure and its raw-SVG rendering with `@lucide/svelte` icons. Add this import at the top of the `<script>` block:

```typescript
	import {
		Anchor, Home, Building2, Users, ShieldCheck, FolderKanban, Rocket, Server,
		Cpu, Container, Waypoints, Package, HardDrive, Radio, Globe,
		Mail, Database, ClipboardList, CreditCard, Wallet, RefreshCw, SlidersHorizontal,
		Sun, Moon, PanelLeftClose, PanelLeftOpen, LogOut
	} from '@lucide/svelte';
	import { NavRail, NavDrawer } from '$lib/components/ui';
```

Replace the `NavItem`/`NavGroup` types and `navGroups` array with:

```typescript
	type NavItem  = { href: string; label: string; icon: typeof Home };
	type NavGroup = { key: string; label: string; icon: typeof Home; items: NavItem[] };

	const navGroups: NavGroup[] = [
		{
			key: 'platform', label: 'Platform', icon: Home,
			items: [
				{ href: '/admin',              label: 'Overview',      icon: Home },
				{ href: '/admin/orgs',         label: 'Organizations', icon: Building2 },
				{ href: '/admin/users',        label: 'Users',         icon: Users },
				{ href: '/admin/staff',        label: 'Staff',         icon: ShieldCheck },
				{ href: '/admin/projects',     label: 'Projects',      icon: FolderKanban },
				{ href: '/admin/deployments',  label: 'Deployments',   icon: Rocket },
				{ href: '/admin/deployments/provisioning', label: 'Provisioning', icon: Waypoints },
				{ href: '/admin/nodes',        label: 'Compute',       icon: Server },
			]
		},
		{
			key: 'infra', label: 'Infrastructure', icon: Cpu,
			items: [
				{ href: '/admin/infra',    label: 'System',       icon: Cpu },
				{ href: '/admin/docker',   label: 'Docker',       icon: Container },
				{ href: '/admin/traefik',  label: 'Traefik',      icon: Waypoints },
				{ href: '/admin/registry', label: 'Registry',     icon: Package },
				{ href: '/admin/storage',  label: 'Storage (S3)', icon: HardDrive },
				{ href: '/admin/mqtt',     label: 'MQTT',         icon: Radio },
				{ href: '/admin/static',   label: 'Static Sites', icon: Globe },
			]
		},
		{
			key: 'services', label: 'Services', icon: Mail,
			items: [
				{ href: '/admin/smtp',     label: 'SMTP',       icon: Mail },
				{ href: '/admin/database', label: 'Database',   icon: Database },
				{ href: '/admin/audit',    label: 'Audit Log',  icon: ClipboardList },
				{ href: '/admin/plan',     label: 'Plans',      icon: CreditCard },
				{ href: '/admin/payments', label: 'Payments',   icon: Wallet },
				{ href: '/admin/updates',  label: 'Updates',    icon: RefreshCw },
				{ href: '/admin/config',   label: 'Config',     icon: SlidersHorizontal },
			]
		},
	];

	let activeGroup = $state<string | null>(null);

	function isActive(href: string): boolean {
		if (href === '/admin') return $page.url.pathname === '/admin';
		return $page.url.pathname.startsWith(href + '/') || $page.url.pathname === href;
	}

	// Open whichever group contains the current route on first load, so the
	// active section is visible without the user having to click into it.
	$effect(() => {
		if (activeGroup !== null) return;
		const current = navGroups.find((g) => g.items.some((i) => isActive(i.href)));
		if (current) activeGroup = current.key;
	});
```

- [ ] **Step 2: Replace the `<aside class="sidebar">` markup**

Replace the entire `<aside class="sidebar" ...>...</aside>` block (brand + nav-scroll + sidebar-footer) with:

```svelte
	<NavRail
		groups={navGroups.map((g) => ({ key: g.key, label: g.label, icon: () => g.icon }))}
		bind:activeGroup
		onSelectGroup={(key) => (activeGroup = activeGroup === key ? null : key)}
	/>
	{#each navGroups as group (group.key)}
		<NavDrawer
			open={activeGroup === group.key}
			title={group.label}
			items={group.items.map((i) => ({ href: i.href, label: i.label, icon: () => i.icon, active: isActive(i.href) }))}
			onNavigate={() => (activeGroup = null)}
		/>
	{/each}
```

Note the `icon: () => g.icon` pattern: `NavRail`/`NavDrawer`'s `icon` prop is a `Snippet`, but a lucide icon is a *component*, not a snippet — `() => g.icon` doesn't actually satisfy `Snippet`'s call signature either. Fix this properly instead: `Snippet` expects to be invoked as `{@render icon()}` and render markup, so wrap each lucide icon in an actual snippet at the call site. Replace the two `.map(...)` calls above with snippet blocks instead — since `NavRail`/`NavDrawer` iterate `groups`/`items` arrays of plain objects, and Svelte snippets can't be created dynamically inside a `.map()`, restructure `NavRail`'s and `NavDrawer`'s prop shape from "icon: Snippet" to "icon: IconComponent" (a component reference, not a snippet) and render it with `<svelte:component>`-style dynamic component syntax instead. Go back and amend Task 26 and Task 27 now, before continuing this task: change `Group`/`Item`'s `icon: Snippet` to `icon: typeof Anchor` (any lucide icon component's type) in both `NavRail.svelte` and `NavDrawer.svelte`, and change their template usage from `{@render g.icon()}` to `{@const Icon = g.icon}<Icon size={20} />` (and similarly `{@const Icon = item.icon}<Icon size={16} />` in `NavDrawer`). Re-run Task 26/27's verification after this amendment before proceeding here.

With that amendment in place, this task's mapping calls become simply:

```svelte
	<NavRail
		groups={navGroups.map((g) => ({ key: g.key, label: g.label, icon: g.icon }))}
		bind:activeGroup
		onSelectGroup={(key) => (activeGroup = activeGroup === key ? null : key)}
	/>
	{#each navGroups as group (group.key)}
		<NavDrawer
			open={activeGroup === group.key}
			title={group.label}
			items={group.items.map((i) => ({ href: i.href, label: i.label, icon: i.icon, active: isActive(i.href) }))}
			onNavigate={() => (activeGroup = null)}
		/>
	{/each}
```

- [ ] **Step 3: Update the footer controls to use lucide icons instead of inline SVG**

Replace the theme-toggle/exit/collapse `<button>`s' inline `<svg>` bodies with `@lucide/svelte`'s `Sun`/`Moon`/`LogOut` (the collapse button and its whole concept go away — there's no "collapsed" state anymore now that the rail is always the same 60px width; delete `collapsed`, `toggleSidebar`, `ftr-collapse`, and the collapse button entirely). Keep the theme toggle button and the "back to dashboard" link, now using `<Sun size={14} />`/`<Moon size={14} />`/`<LogOut size={14}/ >` in place of their raw `<svg>` bodies, positioned in a small footer area under `NavRail` (or fold them into `NavRail` itself as a `footer` snippet — implementer's judgment on whichever is cleaner given how the rest of the file is shaped by this point; either is an acceptable interpretation of "keep these two actions accessible from the rail").

- [ ] **Step 4: Delete now-dead CSS**

Remove the `.sidebar`, `.sidebar.collapsed`, `.brand*`, `.nav-scroll`, `.nav-section-label`, `.nav`, `.nav-item*`, `.nav-icon`, `.pip`, `.sidebar-footer`, `.user*`, `.footer-btns`, `.ftr-btn*` rules and their mobile-media-query counterparts — all superseded by `NavRail`/`NavDrawer`'s own styles. Keep `.gate`/`.gate-ring` (unrelated — the auth-check loading state), `.main`, `.mob-topbar` and friends (mobile support is out of scope for this task; leave the existing mobile topbar as-is for now, noting in a code comment that it still references the old sidebar toggle and will need a follow-up once mobile nav-rail behavior is designed — this plan's mockups and prototype were desktop-only, and mobile admin nav is not a decision this plan has made).

- [ ] **Step 5: Verify**

Run: `cd frontend && npm run check` — expect 0 new errors.
Run: `cd frontend && npm run build` — expect success.

- [ ] **Step 6: Manual visual check**

Open `/admin` in a real browser. Confirm: the rail shows 3 icons, clicking one opens its drawer with the right labeled sub-items, the current route's group auto-opens on load, clicking a drawer item navigates and closes the drawer, the theme toggle still works, both light and dark theme render correctly, and every admin page is still reachable (spot-check 5-6 links across all three groups).

- [ ] **Step 7: Commit**

```bash
cd frontend && git add src/routes/admin/+layout.svelte src/lib/components/ui/NavRail.svelte src/lib/components/ui/NavDrawer.svelte
git commit -m "feat(admin): replace admin sidebar with shared NavRail/NavDrawer shell"
```

---

## Part B — Page migrations

Every task below is a **pure presentation migration**: the existing `<script>` block's data-fetching, state, and business logic (API calls, form validation, SSE handling, everything) is **unchanged** unless a step explicitly says otherwise (this happens in exactly two tasks, both because `DataTable`'s server mode requires slightly reshaping how the existing `fetch` call is invoked, not because any new behavior is added). What changes is the `<template>` markup — replacing hand-rolled `<div class="tbl">`/`<div class="card">`/raw `<svg>` markup with the Task 1-29 components — and deleting the page's local `<style>` block once nothing in the template references its classes anymore.

Three tasks (30, 31, 32) are written in full detail as the worked exemplar for each of the three dominant patterns (dashboard, data table, settings form) found during investigation. Every later task in the same family follows that exemplar's exact recipe, adapted to that page's own specific columns/fields/one-offs (named concretely in each task — never "same as Task N" with no detail).

Three pages need no migration task at all: `admin/docker/+page.svelte`, `admin/mqtt/+page.svelte`, and `admin/traefik/+page.svelte` are 7-line redirect stubs with no UI of their own (confirmed during investigation) — Task 29's Step 6 already verified they still redirect correctly after the nav shell change.

### Task 30: Migrate the admin dashboard (dashboard exemplar)

**Files:**
- Modify: `frontend/src/routes/admin/+page.svelte`

**Interfaces:**
- Consumes: `PageHeader`, `StatCard`, `Card`, `SectionLabel`, `Skeleton`, `InlineAlert` (all from `$lib/components/ui`), plus `@lucide/svelte`'s `Building2`, `Users`, `CreditCard`, `Server`, `ArrowRight` (replacing the four inline stat-card SVG icon bodies and the nav-card arrow icon).

- [ ] **Step 1: Replace the template**

The existing script block (the `stats`/`loading`/`error`/`cards`/`links`/`today` state and the `onMount` fetch) is unchanged. Replace everything in the `<div class="p">...</div>` template with:

```svelte
<PageHeader title="Platform Overview" subtitle={today} />

{#if loading}
	<div class="dash-grid4">
		{#each Array(4) as _}
			<Skeleton variant="card" height="96px" />
		{/each}
	</div>
{:else if error}
	<InlineAlert tone="error">{error}</InlineAlert>
{:else if stats}
	<div class="dash-grid4">
		{#each cards as c}
			<StatCard
				tone="blue"
				value={c.value}
				label={c.label}
			>
				{#snippet icon()}
					{#if c.icon === 'org'}<Building2 size={16} />
					{:else if c.icon === 'user'}<Users size={16} />
					{:else if c.icon === 'paid'}<CreditCard size={16} />
					{:else}<Server size={16} />{/if}
				{/snippet}
			</StatCard>
		{/each}
	</div>

	<div class="dash-divider"></div>

	<section>
		<SectionLabel>Jump to</SectionLabel>
		<div class="dash-grid4">
			{#each links as lk}
				<a href={lk.href} class="dash-nav-card">
					<span class="dash-nav-card-lbl">{lk.label}</span>
					<span class="dash-nav-card-desc">{lk.desc}</span>
					<ArrowRight size={12} class="dash-nav-card-arrow" />
				</a>
			{/each}
		</div>
	</section>
{/if}
```

Note `StatCard`'s `icon` prop is a `Snippet`, so it's passed via the `{#snippet icon()}...{/snippet}` block syntax shown above, not a plain expression.

- [ ] **Step 2: Replace `<style>` with only what's left over**

`StatCard`/`PageHeader`/`SectionLabel` now own the stat-card and header styling entirely. What's left in this page's own `<style>` block is only the parts those components don't cover — the 4-column grid, the divider, and the nav-card (which has no dedicated component of its own, since it's a one-off "card that's also a link" not reused elsewhere):

```css
<style>
	.dash-grid4 {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
		gap: 12px;
	}
	@media (max-width: 640px) {
		.dash-grid4 { grid-template-columns: 1fr 1fr; }
	}
	@media (max-width: 420px) {
		.dash-grid4 { grid-template-columns: 1fr; }
	}

	.dash-divider {
		height: 1px;
		background: var(--border);
		margin: 32px 0 28px;
	}

	.dash-nav-card {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 3px;
		padding: 15px 16px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		text-decoration: none;
		cursor: pointer;
		transition: border-color var(--transition-fast), background var(--transition-fast);
	}
	.dash-nav-card:hover { border-color: var(--accent); background: var(--accent-muted); }
	.dash-nav-card-lbl { font-size: 13px; font-weight: 600; color: var(--text-primary); padding-right: 18px; }
	.dash-nav-card-desc { font-size: 11.5px; color: var(--text-muted); }
	:global(.dash-nav-card-arrow) {
		position: absolute;
		top: 50%;
		right: 14px;
		transform: translateY(-50%);
		color: var(--text-dim);
		transition: color var(--transition-fast), right var(--transition-fast);
	}
	.dash-nav-card:hover :global(.dash-nav-card-arrow) { color: var(--accent); right: 12px; }
</style>
```

Also remove the entire `<div class="p"> { padding: 40px 36px; }` wrapper and its responsive padding media queries — `PageHeader` and the admin layout's own content area now own page padding consistently (add `padding: 24px;` to admin's `.main` content area in `admin/+layout.svelte` once, in this task, rather than have every migrated page re-specify its own page padding — this is the one small addition to `admin/+layout.svelte` any page-migration task makes, and only the first page-migration task that runs needs to add it).

- [ ] **Step 2b: Add shared page padding to the admin layout (one-time)**

In `frontend/src/routes/admin/+layout.svelte`, find `.main { flex:1; overflow-y:auto; background:var(--bg); ... }` and add a nested content wrapper's padding — the simplest correct fix is to add `padding: 24px 28px;` directly to `.main`'s rule (every admin page's root element already renders directly inside `<main class="main">{@render children()}</main>`, so this applies uniformly). Skip this step if a later task discovers it was already done by an earlier one (check for `padding: 24px 28px;` on `.main` first).

- [ ] **Step 3: Verify**

Run: `cd frontend && npm run check`.
Run: `cd frontend && npm run build`.

- [ ] **Step 4: Manual visual check**

Load `/admin`, confirm stat cards, the "Jump to" nav-card grid, loading skeleton (throttle network in devtools to see it), and error state (temporarily break the API call to check) all render correctly in both themes, and the page has consistent padding matching other admin pages once more are migrated.

- [ ] **Step 5: Commit**

```bash
cd frontend && git add src/routes/admin/+page.svelte src/routes/admin/+layout.svelte
git commit -m "refactor(admin): migrate dashboard page to shared design system"
```

---

### Task 31: Migrate `docker/containers` (data-table exemplar)

**Files:**
- Modify: `frontend/src/routes/admin/docker/containers/+page.svelte`

**Interfaces:**
- Consumes: `DataTable`, `Badge`, `StatusDot`, `Button` (all from `$lib/components/ui`), `@lucide/svelte`'s `Trash2`, `RefreshCw`, `ChevronRight`.

- [ ] **Step 1: Replace the template**

Existing script logic (`load`, `prune`, `containerName`, `stateColor`, all `$state`) is unchanged, **except**: delete the `search`/`page`/`filtered`/`totalPages`/`paged` state and the `$effect` that resets `page` on search — `DataTable`'s client mode now owns search/pagination entirely, so the page component no longer needs its own copies of that state.

```svelte
<div class="ct-toolbar">
	<Button variant="danger-outline" size="sm" disabled={pruning} onclick={prune}>
		<Trash2 size={12} />
		{pruning ? 'Pruning…' : 'Prune Unused'}
	</Button>
	{#if pruneMsg}<span class="ct-prune-msg">{pruneMsg}</span>{/if}
	<div class="ct-toolbar-right">
		<Button variant="secondary" size="icon" onclick={load}>
			<RefreshCw size={13} />
		</Button>
	</div>
</div>

{#if error}
	<InlineAlert tone="error">{error}</InlineAlert>
{:else}
	<DataTable
		items={containers}
		rowKey={(c) => c.id}
		searchFields={['image']}
		columns={[
			{ key: 'name', label: 'Name', width: '30%' },
			{ key: 'image', label: 'Image', width: '38%' },
			{ key: 'state', label: 'State', width: '12%' },
			{ key: 'status', label: 'Status', width: '20%' }
		]}
		emptyMessage="No containers found."
	>
		{#snippet row(c)}
			<tr>
				<td class="ct-mono ct-trunc">{containerName(c)}</td>
				<td class="ct-mono ct-trunc">{c.image}</td>
				<td><StatusDot status={c.state === 'running' ? 'running' : c.state === 'exited' || c.state === 'dead' ? 'stopped' : 'pending'} /> {c.state}</td>
				<td class="ct-trunc">{c.status}</td>
			</tr>
		{/snippet}
	</DataTable>
{/if}
```

Note: `containers.filter(...)` for the custom multi-field search (name *and* image) that the old page had doesn't map onto `DataTable`'s single-`searchFields`-array client-mode filter directly, since `containerName(c)` is a computed value, not a raw field on `c`. Two acceptable fixes — pick whichever is cleaner once looking at the real data: (a) precompute a `name` field onto each container object before passing `items` to `DataTable` (e.g. `let tableItems = $derived(containers.map(c => ({ ...c, name: containerName(c) })));`, then `items={tableItems}` and `searchFields={['name', 'image']}`), or (b) accept that search only matches on `image` for now (as shown above) since that's the more common lookup (searching by image tag). Prefer (a) — it preserves the old page's exact search behavior.

The removed inline expandable-row detail panel (container ID, created date, ports, labels) does not have a `DataTable` equivalent yet — `DataTable`'s `row` snippet renders one `<tr>`, and expand-to-reveal-a-second-row isn't part of its Task 19 API. Preserve this by rendering the detail as a second conditionally-visible `<tr>` immediately after the row's own `<tr>` inside the same `row` snippet (a `<tr>` with a single `<td colspan="4">` spanning the full width), toggled by a local `expanded: Set<string>` exactly as the old page already did — this state and its `toggleExpand` function are unchanged from the original.

- [ ] **Step 2: Replace `<style>` with only the leftover custom bits**

```css
<style>
	.ct-toolbar { display: flex; align-items: center; gap: 12px; margin-bottom: 14px; }
	.ct-toolbar-right { margin-left: auto; }
	.ct-prune-msg { font-size: 11.5px; color: var(--accent-green); font-weight: 500; }
	.ct-mono { font-family: var(--font-mono); }
	.ct-trunc { text-overflow: ellipsis; white-space: nowrap; overflow: hidden; max-width: 0; }
</style>
```

Everything else (table chrome, pagination, skeleton, empty state) is now `DataTable`'s responsibility — delete the old `.tbl`/`.thead`/`.trow*`/`.pager`/`.pg-*`/`.sk*`/`.card-list`/`.m-card*`/the entire mobile-card-list `@media` block. `DataTable` already has its own responsive behavior (horizontal scroll on the `<table>`, per Task 19) rather than a separate hand-duplicated mobile card list — this is an intentional simplification the spec calls for (one table implementation, not two).

- [ ] **Step 3: Verify**

Run: `cd frontend && npm run check`.
Run: `cd frontend && npm run build`.

- [ ] **Step 4: Manual visual check**

Load `/admin/docker/containers`, confirm search (by name and image), pagination, row expand/collapse (container details), prune button, and the empty/error states all still work exactly as before, visually restyled. Both themes.

- [ ] **Step 5: Commit**

```bash
cd frontend && git add src/routes/admin/docker/containers/+page.svelte
git commit -m "refactor(admin): migrate docker containers page to DataTable"
```

---

### Task 32: Migrate `smtp` (settings-form exemplar)

**Files:**
- Modify: `frontend/src/routes/admin/smtp/+page.svelte`

**Interfaces:**
- Consumes: `PageHeader`, `Card`, `FormField`, `TextField`, `Textarea`, `Select`, `Toggle`, `Button`, `InlineAlert`, `Skeleton` (all from `$lib/components/ui`).

- [ ] **Step 1: Replace the template**

All script logic (`load`, `save`, `testSmtp`, all `$state`) is unchanged.

```svelte
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
```

- [ ] **Step 2: Replace `<style>` with only the leftover custom bits**

```css
<style>
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
```

Delete the old `.p`/`.hdr`/`.ttl`/`.sub`/`.card`/`.field*`/`.row2`/`.lbl`/`.inp*`/`.toggle*`/`.err-msg`/`.result-msg*`/`.btn-primary`/`.btn-ghost` rules — all superseded.

Note the max-width constraint the original had (`.p { max-width:680px; margin:0 auto; ... }`) — settings forms should stay narrower than full-width data tables for readability. Add `max-width: 640px;` directly on this page's root wrapping element (wrap the whole template in a `<div class="smtp-page">` with `.smtp-page { max-width: 640px; }` in the style block) rather than relying on `PageHeader`/`Card` to impose it, since data-table pages legitimately want full width and settings-form pages don't — this is a per-page-family choice, not something the shared components should hardcode.

- [ ] **Step 3: Verify**

Run: `cd frontend && npm run check`.
Run: `cd frontend && npm run build`.

- [ ] **Step 4: Manual visual check**

Load `/admin/smtp`, confirm the toggle, password show/hide, save flow, test-email flow, and both success/error inline alerts all work as before. Both themes.

- [ ] **Step 5: Commit**

```bash
cd frontend && git add src/routes/admin/smtp/+page.svelte
git commit -m "refactor(admin): migrate SMTP settings page to shared design system"
```

---

### Task 33: Migrate `docker/images`

**Files:** Modify `frontend/src/routes/admin/docker/images/+page.svelte`

Follow Task 31's exact `DataTable` recipe. Columns: `ID` (mono, truncated), `Tags` (comma-joined or "—"), `Size` (using the existing byte-formatting helper, unchanged). No expandable-row detail on this page (unlike containers) — a plain `row` snippet with 3 `<td>`s is sufficient. Keep the existing "Prune Unused" button (same `Trash2`-icon `Button` treatment as Task 31), still calling the existing `confirm()`-gated `prune()` function for now — **do not** swap this one's `confirm()` for `ConfirmDialog` in this task (that's Task 43's staff page and the other three `confirm()` sites' job specifically, tracked separately so this task stays scoped to table-shape migration only... actually, on reflection: since `ConfirmDialog` already exists by this point (Task 24) and this page already has a "Prune Unused" destructive action, replace its `confirm()` with a `ConfirmDialog` here too — there's no reason to defer it artificially. Add local state `let showPruneConfirm = $state(false);`, wrap the existing `prune()` body's logic into what `ConfirmDialog`'s `onConfirm` calls, and render `<ConfirmDialog bind:open={showPruneConfirm} title="Prune unused images" message="This cannot be undone." confirmLabel="Prune" onConfirm={prune} />` triggered by the button instead of calling `prune` directly.

- [ ] Steps 1-5: same structure as Task 31 (replace template, replace style, `npm run check`, `npm run build`, manual check in both themes), applied to this page's specific columns/actions above.
- [ ] **Commit:** `git commit -m "refactor(admin): migrate docker images page to DataTable"`

---

### Task 34: Migrate `docker/networks`

**Files:** Modify `frontend/src/routes/admin/docker/networks/+page.svelte`

Follow Task 31's `DataTable` recipe. Columns: `Name`, `Driver`, `Scope`, `Subnet` (mono), `Containers` (count). No prune button on this page (confirmed — unlike images/volumes/containers, networks has no destructive bulk action today; don't add one). No expandable row.

- [ ] Steps 1-5: same structure as Task 31.
- [ ] **Commit:** `git commit -m "refactor(admin): migrate docker networks page to DataTable"`

---

### Task 35: Migrate `docker/services`

**Files:** Modify `frontend/src/routes/admin/docker/services/+page.svelte`

Follow Task 31's `DataTable` recipe, including its expandable-row-as-second-`<tr>` pattern (this page has one too). Columns: `Name`, `Image` (mono), `Mode`, `Replicas` (render as a `Badge` — `tone="green"` when running count equals desired, `tone="red"` otherwise, replacing the old ad hoc red/green pill). Expandable detail row shows: ID, Mode, Image, Ports, Created, Updated, Labels — same fields, same toggle-on-row-click interaction, unchanged logic.

- [ ] Steps 1-5: same structure as Task 31.
- [ ] **Commit:** `git commit -m "refactor(admin): migrate docker services page to DataTable"`

---

### Task 36: Migrate `docker/volumes`

**Files:** Modify `frontend/src/routes/admin/docker/volumes/+page.svelte`

Follow Task 31's `DataTable` recipe plus Task 33's `ConfirmDialog`-for-prune treatment (this page also has a native-`confirm()` prune button — replace it the same way). Columns: `Name`, `Driver`, `Scope`, `Mountpoint` (mono, truncated). Expandable row detail: labels box (same as containers/services' expand pattern).

- [ ] Steps 1-5: same structure as Task 31.
- [ ] **Commit:** `git commit -m "refactor(admin): migrate docker volumes page to DataTable"`

---

### Task 37: Migrate `mqtt/clients`

**Files:** Modify `frontend/src/routes/admin/mqtt/clients/+page.svelte`

Follow Task 31's `DataTable` recipe. Columns: `Client ID` (mono), `Username`, `Address` (mono), `Protocol`, `Connected at`. The existing loose `any[]`-typed data with fallback field-name lookups (the backend response shape isn't strictly typed) is unchanged — `DataTable`'s generic `<T>` can be `any` here exactly as the page's existing type already is; this is a pre-existing looseness in the data layer, not something this presentation migration fixes.

- [ ] Steps 1-5: same structure as Task 31.
- [ ] **Commit:** `git commit -m "refactor(admin): migrate MQTT clients page to DataTable"`

---

### Task 38: Migrate `mqtt/subscriptions`

**Files:** Modify `frontend/src/routes/admin/mqtt/subscriptions/+page.svelte`

Follow Task 31's `DataTable` recipe. Columns: `Topic` (mono), `Client ID` (mono), `QoS` (render as a small `Badge`, `tone="blue"`).

- [ ] Steps 1-5: same structure as Task 31.
- [ ] **Commit:** `git commit -m "refactor(admin): migrate MQTT subscriptions page to DataTable"`

---

### Task 39: Migrate `mqtt/topics`

**Files:** Modify `frontend/src/routes/admin/mqtt/topics/+page.svelte`

The simplest table in the app — a single `Topic` (mono) column. Follow Task 31's `DataTable` recipe with `columns={[{ key: 'topic', label: 'Topic' }]}` and a one-`<td>` row snippet.

- [ ] Steps 1-5: same structure as Task 31.
- [ ] **Commit:** `git commit -m "refactor(admin): migrate MQTT topics page to DataTable"`

---

### Task 40: Migrate `users`

**Files:** Modify `frontend/src/routes/admin/users/+page.svelte`

Follow Task 31's `DataTable` recipe. Columns: `User` (an `Avatar` — initials from email, `tone` picked deterministically from the email's char code exactly as the existing page already computes it, reuse that exact function — plus email and truncated ID stacked next to it), `Role` (`Badge`, `tone="blue"` for admin/superadmin, `tone="neutral"` for member), `Orgs` (count), `Joined`, `Actions` (`Button`s: Suspend/Unsuspend, and Revoke admin — superadmin-gated exactly as today). Delete the page's own separate mobile card-list entirely (same simplification as Task 31 — `DataTable` doesn't need a hand-duplicated mobile fallback). Also delete the page's own two separate pager instances (desktop + mobile) — one `DataTable` replaces both. Keep the existing admin-count/total-count header pills, but render them as two `Badge`s in the `PageHeader`'s `actions` snippet instead of custom markup.

- [ ] Steps 1-5: same structure as Task 31.
- [ ] **Commit:** `git commit -m "refactor(admin): migrate users page to DataTable"`

---

### Task 41: Migrate `orgs`

**Files:** Modify `frontend/src/routes/admin/orgs/+page.svelte`

Follow Task 31's `DataTable` recipe. Columns: `Organization` (`Avatar` with the org's first letter + name + slug stacked), `Tier` (`Badge`), `Status` (`StatusDot` + text), `Members` (count), `Nodes` (count), `Created`, `Actions` (`Button`s: Open, Quota, Suspend/Restore).

The existing "Edit Quota" modal (9 numeric fields: `max_projects`, `max_members`, `max_replicas`, `max_parallel_deployments`, `max_git_providers`, `max_orgs`, `node_count`, `cpu_cores`, `memory_mb`, each with a "-1 = unlimited" hint) is rebuilt using `Modal` (Task 24) as the container, with each field as a `FormField` wrapping a `TextField type="number"`, laid out in a 2-column CSS grid inside the modal body (reuse `smtp`'s `.row2`-style 2-col grid pattern for this, defined locally in this page's remaining `<style>` block — it's specific to this one modal's 9-field layout, not generic enough to be its own shared component). The existing save/validation logic for this modal is unchanged; only its markup moves into `Modal`'s `children`/`footer` snippets.

- [ ] Steps 1-5: same structure as Task 31, plus rebuilding the Edit Quota modal on `Modal`.
- [ ] **Commit:** `git commit -m "refactor(admin): migrate organizations page to DataTable and Modal"`

---

### Task 42: Migrate `projects` (first `DataTable` server-mode wiring)

**Files:** Modify `frontend/src/routes/admin/projects/+page.svelte`

This page's backend (`GET /admin/projects`, `page`/`limit`/`q` query params, response `{ items, total, page, limit }`) already matches `DataTable`'s server-mode contract exactly (see the spec's corrected "DataTable: client and server modes" section) — this is the **first of two tasks that wires `DataTable` in server mode instead of client mode**, and existing fetch logic *does* change here, unlike every table task before it.

Columns: `Project` (name + slug stacked), `Organization` (`Avatar` + org name + slug stacked), `Services` (count), `Created`, `Actions` (`Button`: Open — no destructive action on this page).

- [ ] **Step 1: Replace the existing manual fetch with a `fetchPage` callback**

Wherever the page currently calls its own `fetch`/`api.get('/admin/projects', ...)` with manually-constructed `page`/`limit`/`q` params and stores the result in local state, replace that whole flow with a single function matching `DataTable`'s server-mode contract:

```typescript
	async function fetchProjectsPage(params: { page: number; pageSize: number; search: string }) {
		const res = await api.get<{ items: AdminProject[]; total: number }>(
			`/admin/projects?page=${params.page}&limit=${params.pageSize}&q=${encodeURIComponent(params.search)}`
		);
		if (!res.data) throw new Error(res.error?.message ?? 'Failed to load projects');
		return { rows: res.data.items, total: res.data.total };
	}
```

(Adjust the exact `api.get` call shape to match whatever thin wrapper this codebase's `$lib/api/client` already exposes — the existing page already calls this endpoint somehow; reuse that exact call mechanism, just reshape its inputs/outputs to `DataTable`'s `{page,pageSize,search} -> {rows,total}` contract as shown.)

- [ ] **Step 2: Replace the template**

```svelte
<DataTable
	fetchPage={fetchProjectsPage}
	rowKey={(p) => p.id}
	columns={[
		{ key: 'project', label: 'Project' },
		{ key: 'org', label: 'Organization' },
		{ key: 'services', label: 'Services' },
		{ key: 'created', label: 'Created' },
		{ key: 'actions', label: '' }
	]}
	emptyMessage="No projects found."
>
	{#snippet row(p)}
		<tr>
			<td>
				<div class="pr-stack">
					<span class="pr-name">{p.name}</span>
					<span class="pr-sub">{p.slug}</span>
				</div>
			</td>
			<td>
				<div class="pr-org">
					<Avatar initials={p.org_name} size={22} />
					<div class="pr-stack">
						<span class="pr-name">{p.org_name}</span>
						<span class="pr-sub">{p.org_slug}</span>
					</div>
				</div>
			</td>
			<td>{p.service_count}</td>
			<td>{new Date(p.created_at).toLocaleDateString()}</td>
			<td><Button variant="ghost" size="sm" onclick={() => goto(`/orgs/${p.org_slug}/projects/${p.slug}`)}>Open</Button></td>
		</tr>
	{/snippet}
</DataTable>
```

- [ ] **Step 3: Replace `<style>` with the leftover `.pr-*` stack/org styles**

```css
<style>
	.pr-stack { display: flex; flex-direction: column; }
	.pr-name { font-size: 12.5px; font-weight: 600; color: var(--text-primary); }
	.pr-sub { font-size: 10.5px; color: var(--text-dim); }
	.pr-org { display: flex; align-items: center; gap: 8px; }
</style>
```

- [ ] **Step 4: Verify**

Run: `cd frontend && npm run check`.
Run: `cd frontend && npm run build`.

- [ ] **Step 5: Manual visual check**

Load `/admin/projects`, confirm search and pagination now genuinely trigger new network requests (check the browser's network tab — this is the key behavioral difference from every client-mode table before it) rather than filtering an already-fetched array, and that the loading skeleton shows between page loads. Both themes.

- [ ] **Step 6: Commit**

```bash
cd frontend && git add src/routes/admin/projects/+page.svelte
git commit -m "refactor(admin): migrate projects page to DataTable in server mode"
```

---

### Task 43: Migrate `staff`

**Files:** Modify `frontend/src/routes/admin/staff/+page.svelte`

Follow Task 31's `DataTable` recipe (client mode — no server pagination on this endpoint). Columns: `User` (`Avatar` + email + id stacked), `Permissions` (a row of small `Badge`s, truncated with a "+N more" `Badge` when the list is long — reuse the existing truncation-count logic, just restyle its output as `Badge`s), `Joined`, `Action` (`Button`: Revoke, gated behind `ConfirmDialog` instead of the existing native `confirm()` — same treatment as Task 33).

The "Promote to Admin" modal (20 permission groups × View/Manage = 40 `Checkbox`es) is rebuilt on `Modal`, with each of the 40 checkboxes as a `Checkbox` (Task 10) arranged in the same grouped layout the existing modal already uses (one group heading per permission category, two `Checkbox`es — View, Manage — per row under it). This is the single largest markup transformation in Part B by checkbox count, but mechanically identical to every other `Checkbox` usage — there is no new interaction pattern here, just volume. Existing validation/save logic for this modal is unchanged.

- [ ] Steps 1-5: same structure as Task 31, plus rebuilding the Promote modal on `Modal`+`Checkbox`, plus the `ConfirmDialog` swap for Revoke.
- [ ] **Commit:** `git commit -m "refactor(admin): migrate staff page to DataTable and Modal"`

---

### Task 44: Migrate `nodes`

**Files:** Modify `frontend/src/routes/admin/nodes/+page.svelte`

Follow Task 31's `DataTable` recipe. Columns: `Node`, `Organization`, `Provider` (keep its existing color-coding, expressed as inline `style="color:..."` using the closest matching token rather than the provider-specific hex it currently hardcodes — e.g. map each known provider name to one of `var(--accent)`/`var(--accent-green)`/`var(--accent-yellow)` rather than inventing new hardcoded provider colors), `Region`, `Status` (`StatusDot` — use the `pending`/`deploying` pulsing variants for transitional states exactly as the old page's custom pulse logic already distinguishes them), `Public IP` (mono), `Created`.

The compact status-summary bar above the table (active/provisioning/degraded/stopped counts with pulsing dots) has no existing shared-component equivalent — keep it as page-local markup using `StatusDot` for each count's dot (replacing its own hand-rolled dot CSS) but no other component, since it's a one-off summary strip not reused elsewhere.

- [ ] Steps 1-5: same structure as Task 31.
- [ ] **Commit:** `git commit -m "refactor(admin): migrate nodes page to DataTable"`

---

### Task 45: Migrate `deployments` (second `DataTable` server-mode wiring)

**Files:** Modify `frontend/src/routes/admin/deployments/+page.svelte`

Same server-mode treatment as Task 42 — `GET /admin/deployments/app` already accepts `page`/`limit` (confirmed in the backend). Columns: `Org`, `Service`, `Status` (`StatusDot` + text, using the existing status-to-dot-state mapping), `Triggered` (relative time, existing formatting helper unchanged), `Started` (relative time). The existing org/status filter controls stay as page-local `Select`s above the table, and — since `DataTable`'s server-mode `fetchPage` signature only threads `search` through, not arbitrary extra filters — capture the current filter `Select` values in the `fetchPage` closure (read the component's own `$state` filter variables directly inside `fetchProjectsPage`-equivalent function body, the same way Task 42's function closes over nothing external because projects had no extra filters; this page's `fetchPage` closes over the two filter `$state`s in addition to the params `DataTable` passes it) and call `loadServerPage`-equivalent re-fetch (this happens automatically — `DataTable`'s own `$effect` re-runs `fetchPage` on `page`/`pageSize`/`search` changes, but changing an *external* filter `Select` won't trigger that `$effect` on its own; add a small local `$effect` in this page that watches the filter state and forces a table refresh — since `DataTable` doesn't expose an imperative "refetch now" method, the simplest correct fix is to give `DataTable` a `key` attribute that changes when the filters change, e.g. `{#key orgFilter + statusFilter}<DataTable ... />{/key}`, which remounts it and re-triggers its internal effect from page 0).

- [ ] Steps 1-6: same structure as Task 42 (fetchPage callback + template + style + verify + manual check including confirming the org/status filters correctly reset to page 0 and refetch).
- [ ] **Commit:** `git commit -m "refactor(admin): migrate deployments page to DataTable in server mode"`

---

### Task 46: Migrate `deployments/provisioning`

**Files:** Modify `frontend/src/routes/admin/deployments/provisioning/+page.svelte`

Same server-mode treatment as Task 45 (its backend also has `page`/`limit`). Columns: `Organization`, `Node`, `Provider`, `Region`, `Status` (`StatusDot`), `Started`. No extra filter controls on this page (unlike deployments) — simpler than Task 45, closer to Task 42's shape.

- [ ] Steps 1-6: same structure as Task 42.
- [ ] **Commit:** `git commit -m "refactor(admin): migrate provisioning page to DataTable in server mode"`

---

### Task 47: Migrate `payments`

**Files:** Modify `frontend/src/routes/admin/payments/+page.svelte`

This page's backend already paginates (`page`/`per_page`) but with a **segmented-control status filter**, not a plain `Select` — keep the segmented control as page-local markup (small pill-button group, `Button variant="ghost"`/`Button variant="primary"` per option depending on whether it's the active filter, laid out in a row) rather than forcing it into `Dropdown` or `Select`, since a segmented control is visually and interactionally distinct from both. Wire `DataTable` in server mode following Task 45's pattern (filter changes force a refetch via the `{#key}` remount technique). Columns: `Organization`, `Amount` (currency-formatted, existing helper unchanged), `Status` (`Badge`), `Description`, `Payment ID` (mono, truncated), `Date`.

- [ ] Steps 1-6: same structure as Task 45.
- [ ] **Commit:** `git commit -m "refactor(admin): migrate payments page to DataTable in server mode"`

---

### Task 48: Migrate `audit` (ActivityList + cursor exemplar)

**Files:** Modify `frontend/src/routes/admin/audit/+page.svelte`

Per the spec, this page does **not** use `DataTable` — its backend is cursor-paginated (`cursor`/`limit`/`org_id`, no `total`), which doesn't fit the M3 page-number footer. It uses `ActivityList`/`ListRow` instead (a better semantic fit for a chronological feed anyway), with a "Load more" button driven by the existing cursor logic, unchanged.

**Interfaces:**
- Consumes: `ActivityList`, `ListRow`, `Button`, `FormField`, `TextField` (all `$lib/components/ui`), `@lucide/svelte`'s `Activity` (or another suitable generic action icon), `ChevronDown`.

- [ ] **Step 1: Replace the template**

Existing `load`/`loadMore`/cursor state is unchanged.

```svelte
<PageHeader title="Audit Log" subtitle="Organization activity history." />

<FormField label="Filter by organization ID">
	<TextField bind:value={orgIdFilter} placeholder="org UUID (optional)" />
</FormField>

<ActivityList>
	{#each logs as log (log.id)}
		<div class="audit-row-wrap">
			<button type="button" class="audit-row-toggle" onclick={() => toggleExpand(log.id)}>
				<ListRow
					iconTone="blue"
					title={log.action}
					meta="{log.resource_type ?? ''} · {log.user_id ?? 'system'} · {log.ip ?? ''}"
				>
					{#snippet icon()}<Activity size={13} />{/snippet}
					{#snippet trailing()}<span class="audit-time">{new Date(log.created_at).toLocaleString()}</span>{/snippet}
				</ListRow>
			</button>
			{#if expanded.has(log.id)}
				<pre class="audit-detail">{JSON.stringify(log.metadata, null, 2)}</pre>
			{/if}
		</div>
	{/each}
</ActivityList>

{#if hasMore}
	<div class="audit-load-more">
		<Button variant="secondary" onclick={loadMore} disabled={loadingMore}>
			<ChevronDown size={13} />
			{loadingMore ? 'Loading…' : 'Load more'}
		</Button>
	</div>
{/if}
```

`toggleExpand`/`expanded` (a `Set<string>` of expanded log IDs) is the same pattern already used in Task 31/35/36's tables — reuse that exact logic here, just applied to `ListRow`s instead of `<tr>`s.

- [ ] **Step 2: Replace `<style>` with the leftover audit-specific bits**

```css
<style>
	.audit-row-toggle { display: block; width: 100%; background: none; border: none; padding: 0; cursor: pointer; text-align: left; }
	.audit-time { font-size: 10.5px; color: var(--text-dim); font-variant-numeric: tabular-nums; white-space: nowrap; }
	.audit-detail {
		margin: 0 0 8px 42px;
		padding: 10px 12px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--text-secondary);
		overflow-x: auto;
		max-height: 240px;
	}
	.audit-load-more { display: flex; justify-content: center; margin-top: 16px; }
</style>
```

- [ ] **Step 3: Verify**

Run: `cd frontend && npm run check`.
Run: `cd frontend && npm run build`.

- [ ] **Step 4: Manual visual check**

Load `/admin/audit`, confirm org-ID filtering, row expand-to-show-JSON, and "Load more" (cursor-driven fetch of the next batch) all still work. Both themes.

- [ ] **Step 5: Commit**

```bash
cd frontend && git add src/routes/admin/audit/+page.svelte
git commit -m "refactor(admin): migrate audit log page to ActivityList"
```

---

### Task 49: Migrate `infra` and `system`

**Files:**
- Modify: `frontend/src/routes/admin/infra/+page.svelte`
- Modify: `frontend/src/routes/admin/system/+page.svelte`

Both are dashboard/table hybrids fed by live data (`infra` via SSE metrics stream, `system` via a Swarm Nodes table plus the dynamic-JSONB config editor also seen in Task 53). The SSE wiring, metric-card animated bar-fills, and Swarm Join Tokens reveal/copy logic are **entirely unchanged** — this task only restyles:

- `infra`'s CPU/memory/swap/uptime/disk/network metric cards → `StatCard` (with a `ProgressBar`, Task 23, added beneath each metric's value to replace its existing ad hoc animated bar-fill — same live-updating value, now driven through `ProgressBar`'s `value` prop instead of a hand-rolled width style).
- `infra`'s "Core Services" table and "Swarm Nodes" table → `DataTable` in client mode (both fully in-memory today, no backend pagination needed), following Task 31's recipe. Core Services columns: `Service`, `State` (`StatusDot`), `CPU`, `Memory`, `Status`. Swarm Nodes columns: `Node ID` (mono), `Hostname`, `IP` (mono), `Role` (`Badge`), `State` (`StatusDot`), `Availability`, `Engine`.
- `infra`'s Join Tokens reveal/copy card → `Card` as the container, `Button` for reveal/copy actions, unchanged masking logic.
- `system`'s Swarm Nodes table → same `DataTable` treatment as `infra`'s (identical columns — these are likely the same underlying data shown on two pages; if so, this is also an opportunity to confirm both pages now render it identically since they share the same component, not an opportunity to *deduplicate the pages themselves*, which is out of scope here).
- `system`'s dynamic-JSONB "System Config" editor section → see Task 53's `config` migration for the exact per-key editor recipe (type badge + auto-sizing textarea + ⌘+Enter save) — apply the identical recipe here, since it's the same UI pattern on both pages.

- [ ] Steps for each file: replace template sections listed above (SSE/data logic untouched), replace `<style>` with only page-specific leftovers, `npm run check`, `npm run build`, manual check in both themes (watch the SSE-driven metrics actually update live after the restyle — this is the one page where "still behaves exactly as before" needs to be checked over a few seconds of real-time updates, not just a static screenshot).
- [ ] **Commit (infra):** `git commit -m "refactor(admin): migrate infra dashboard to shared design system"`
- [ ] **Commit (system):** `git commit -m "refactor(admin): migrate system page to shared design system"`

---

### Task 50: Migrate `plan`

**Files:** Modify `frontend/src/routes/admin/plan/+page.svelte`

Not a `DataTable` page — a grid of plan cards. Each card becomes a `Card` containing: name, price, an `enabled` `Toggle`, an "Edit" `Button`, and the 3×3 mini-stat-grid (CPU Cores/Memory/Max Replicas/Nodes/Members/Projects/Orgs/Parallel Deploys/Git Providers, each `-1 = ∞`) rendered as a simple local CSS grid of label+value pairs (not `StatCard` — these are too small/dense for `StatCard`'s icon-badge treatment; a plain 2-line label/value stack per cell, styled locally in this page, same as `orgs`' quota-modal field list in spirit).

Both existing modals ("Create Plan" — 11 fields, and "Edit Plan" — the same 9 numeric fields as `orgs`' quota modal plus name+enabled) are rebuilt on `Modal`, reusing the exact same 2-column numeric-field grid pattern established in Task 41 for the 9 shared fields, plus 2 additional fields (name as `TextField`, enabled as `Toggle`) for Create/Edit specifically.

- [ ] Steps 1-5: same structure as Task 31's shape (template/style/verify/check/commit), applied to this page's card-grid layout instead of a table, plus the two `Modal`-based forms.
- [ ] **Commit:** `git commit -m "refactor(admin): migrate plans page to shared design system"`

---

### Task 51: Migrate `registry`

**Files:** Modify `frontend/src/routes/admin/registry/+page.svelte`

Already imports `@lucide/svelte` directly — this task's icon work is smaller than most (swap its existing lucide imports for whichever ones are needed to match the new component set, but no raw-`<svg>`-to-lucide conversion needed here specifically, unlike every other page). The 4-stat-card header (Total Blobs/Blob Storage Used/Artifacts/Namespaces) → `StatCard` grid. The expandable Namespaces list with debounced search, lazily loading nested Repos → `ActivityList`/`ListRow` for the namespace rows (each with an expand toggle following the same `Set<string>` pattern as Tasks 31/35/36/48), with the nested repo list rendered as a second, indented `ActivityList` inside the expanded state. The existing delete-confirmation modal (namespace or repo) → `ConfirmDialog` (it already uses `AlertTriangle`/`Trash2` from lucide — those carry over into `ConfirmDialog`'s default danger styling, which already implies a trash/warning treatment, so the page no longer needs to render those icons itself inside the dialog body).

- [ ] Steps 1-5: same structure as Task 31's shape, applied to stat-cards + nested-list + confirm-dialog.
- [ ] **Commit:** `git commit -m "refactor(admin): migrate registry page to shared design system"`

---

### Task 52: Migrate `database`

**Files:** Modify `frontend/src/routes/admin/database/+page.svelte`

The Postgres/Redis tab switcher → a small local tab-bar reusing the exact `.tabs`/`.tab`/`.tab.active` CSS already documented in the design-system memory's "Detail Panel Anatomy" section (copy that pattern verbatim — it's the established main-app tab convention, not something Part A builds as a standalone component since it's a simple enough CSS pattern that every consumer already copy-pastes rather than importing). The table-list sidebar → `SearchInput` + `ActivityList`/`ListRow`. The Columns sub-tab (`Column`/`Type`/`Nullable`/`PK`) and Rows sub-tab (real per-row Edit/Delete, PK-aware) → two separate `DataTable`s — Columns in client mode (small, fixed-per-table dataset), Rows in **server mode**, since this endpoint (`/admin/db/tables/:name/rows`) already supports `page`/`per_page` per the investigation — this is a third `DataTable` server-mode wiring, following Task 42's exact recipe. The edit-row modal and drop-table confirmation modal → `Modal` and `ConfirmDialog` respectively. The Redis flat key/value list → `ActivityList`/`ListRow`.

- [ ] Steps 1-6: same structure as Task 42 (since Rows uses server mode) combined with Task 31's shape for the Columns table and the sidebar list.
- [ ] **Commit:** `git commit -m "refactor(admin): migrate database console to shared design system"`

---

### Task 53: Migrate `config`

**Files:** Modify `frontend/src/routes/admin/config/+page.svelte`

Not a fixed-field settings form — one row per arbitrary JSONB config key, each with a type badge and an auto-sizing JSON-editing `Textarea` (⌘+Enter save, existing keybinding logic unchanged). Establish the recipe Task 49 (`system`) reuses: each config-key row is a `Card` containing a small header (`Badge` showing the value's type: string/number/boolean/object/array/null) plus a `Textarea` (auto-sizing behavior unchanged — this is existing JS, not a new `Textarea` feature; `Textarea`'s `rows` prop just needs to be driven by that existing auto-size calculation instead of a fixed number). The separate "Maintenance Mode" toggle banner at the top → a `Card` with a `Toggle` and its own independent save button, following the same layout as `smtp`'s toggle row.

- [ ] Steps 1-5: same structure as Task 31's shape, applied to the per-key `Card`+`Badge`+`Textarea` list and the maintenance-mode banner.
- [ ] **Commit:** `git commit -m "refactor(admin): migrate config page to shared design system"`

---

### Task 54: Migrate `static`

**Files:** Modify `frontend/src/routes/admin/static/+page.svelte`

Two-pane file browser: file-list sidebar (searchable) + read-only code-viewer pane (with Copy button) for the selected `.conf` file. Sidebar → `SearchInput` + `ActivityList`/`ListRow` (one row per filename, icon via `@lucide/svelte`'s `FileText`). Code viewer pane → `Card` wrapping a `<pre>` (unchanged — `Card` doesn't need to know its content is code) with a `Button` (Copy) positioned in a small header row above it. The existing "Show Log Stream" button opening the already-shared `LogViewerOverlay` component is **completely unchanged** — that component isn't part of this migration's scope (it's not in the Part A component list, and the spec doesn't call for touching it); this task only restyles the button that opens it, not the overlay itself.

- [ ] Steps 1-5: same structure as Task 31's shape, applied to the two-pane file-browser layout.
- [ ] **Commit:** `git commit -m "refactor(admin): migrate static sites page to shared design system"`

---

### Task 55: Migrate `traefik/dynamic`

**Files:** Modify `frontend/src/routes/admin/traefik/dynamic/+page.svelte`

Identical two-pane file-browser shape to Task 54 (`static`) — apply the exact same recipe (file-list sidebar via `SearchInput`+`ActivityList`/`ListRow`, code-viewer pane via `Card`+`<pre>`+Copy `Button`), just for Traefik dynamic config files instead of nginx site confs.

- [ ] Steps 1-5: same structure as Task 54.
- [ ] **Commit:** `git commit -m "refactor(admin): migrate traefik dynamic config page to shared design system"`

---

### Task 56: Migrate `traefik/settings`

**Files:** Modify `frontend/src/routes/admin/traefik/settings/+page.svelte`

Settings-form pattern — follow Task 32's (`smtp`) exact recipe for its fields (Main Domain, Traefik Network, HTTP Entrypoint, HTTPS Entrypoint as a 2-column grid via `FormField`+`TextField`, Cert Resolver as a single `FormField`+`TextField`). The live-generated read-only `traefik.yml` preview block (derived from form field values, existing derivation logic unchanged) → `Card` wrapping a `<pre>`, with a Copy `Button` in a small header row (same treatment as Task 54's code-viewer header). The "Show Log Stream" button → same unchanged-overlay treatment as Task 54.

- [ ] Steps 1-5: same structure as Task 32.
- [ ] **Commit:** `git commit -m "refactor(admin): migrate traefik settings page to shared design system"`

---

### Task 57: Migrate `traefik/static`

**Files:** Modify `frontend/src/routes/admin/traefik/static/+page.svelte`

The simplest remaining page — a single read-only code block with a Copy button, no editing, no sidebar. → `PageHeader` + `Card` wrapping a `<pre>` + a Copy `Button` in a small header row (identical treatment to the code-viewer pane established in Task 54, just without the file-list sidebar since there's only ever one file here).

- [ ] Steps 1-5: same structure as Task 31's shape (much smaller diff than most).
- [ ] **Commit:** `git commit -m "refactor(admin): migrate traefik static config page to shared design system"`

---

### Task 58: Migrate `storage`

**Files:** Modify `frontend/src/routes/admin/storage/+page.svelte`

The largest and most complex admin page. Bucket-grid view (backend/bucket/endpoint) → a grid of `Card`s (one per bucket), each showing backend/bucket/endpoint as a small label/value stack, clickable to drill into the file-browser view. File-browser view (breadcrumbs + folders/files list: Name/Size/Last Modified) → breadcrumbs as a small local nav (not a new shared component — a one-off `/`-separated link trail specific to this page), the folder/file list as a `DataTable` in client mode (columns: `Name` with a folder/file `@lucide/svelte` icon prefix, `Size`, `Last Modified`) with row `onclick` navigating into folders (existing URL-query-param-driven navigation logic unchanged). The "Diagnostics" panel (PUT/EXISTS/LIST/DELETE probe checks with pass/fail icons and a summary banner) → `Card` containing a `ListRow` per probe (icon: `@lucide/svelte`'s `CheckCircle2`/`XCircle` per pass/fail, swapped in per-probe exactly as the existing logic already decides pass/fail) plus an `InlineAlert` for the summary banner (`tone="success"` or `tone="error"` based on overall pass/fail, replacing its existing ad hoc banner). The slide-in file-preview drawer (image inline, text/code via `<pre>` with copy, binary fallback with download link) → keep as a custom slide-in panel (this codebase already has a `SlidePanel`/`uiStore.pushPanel()` mechanism per the design-system memory — use that existing mechanism instead of a new one-off drawer implementation, replacing whatever custom slide-in this page currently hand-rolls) with its internal content (image/`<pre>`/download-link) using `Card`/`Button` for chrome.

Given this page's size and the breadth of distinct sub-features (bucket grid, breadcrumb browser, diagnostics, preview drawer), this is the one task in Part B where the implementer should expect it to take meaningfully longer than any other single page task — that's expected and correct given the source page is 685 lines, the largest in the app.

- [ ] Steps 1-5: same structure as Task 31's shape, applied section by section (bucket grid, then browser table, then diagnostics, then preview drawer) — commit after each section is verified working rather than attempting the whole page as one atomic change, since this task legitimately covers what would be 3-4 separate tasks on a smaller page.
- [ ] **Commit(s):** one or more commits, e.g. `git commit -m "refactor(admin): migrate storage bucket browser to shared design system"`, `git commit -m "refactor(admin): migrate storage diagnostics and preview drawer to shared design system"`.

---

### Task 59: Migrate `updates`

**Files:** Modify `frontend/src/routes/admin/updates/+page.svelte`

The "Current Version" card (git sha, build date, update-available banner vs. up-to-date row, "Check for updates" button) → `Card` + `InlineAlert` (`tone="info"` for update-available, `tone="success"` for up-to-date) + `Button`. The "Pull & Restart" card with its live SSE-streamed scrolling log (color-coded ok/err/warn lines, blinking cursor, unchanged SSE logic) → `Card` wrapping the existing log-line rendering (color-code each line using `var(--accent-green)`/`var(--accent-red)`/`var(--accent-yellow)` instead of whatever hardcoded colors it currently uses — the blinking-cursor CSS animation is kept as page-local, it's a one-off effect not worth generalizing into a shared component). Status badge (Done/Failed/Restarting) → `Badge`. Reconnect-hint banner on expected mid-update disconnect → `InlineAlert tone="warning"`. Clear log button → `Button variant="ghost"`.

- [ ] Steps 1-5: same structure as Task 31's shape, with particular attention in the manual check to confirming the SSE log stream still renders live and the blinking cursor/color-coding survive the restyle.
- [ ] **Commit:** `git commit -m "refactor(admin): migrate updates page to shared design system"`

---

### Task 60: Final cleanup — remove the token alias layer, confirm zero stragglers

**Files:**
- Modify: `frontend/src/routes/admin/+layout.svelte`

**Interfaces:**
- Consumes: nothing new — this task only deletes code.

Task 2 introduced a temporary alias layer (`.shell { --bg: var(--bg-base); ... }`) so admin's old CSS var names kept working while pages migrated one at a time. Every page has now migrated (Tasks 30-59) — this task deletes that bridge, since keeping it around after nothing references it is exactly the kind of forgotten scaffolding the spec's whole premise is against.

- [ ] **Step 1: Confirm nothing still uses the old var names**

```bash
cd frontend && grep -rn "var(--bg)\|var(--surface)\|var(--surface-2)\|var(--border-2)\|var(--text-2)\|var(--text-3)\|var(--text-4)\|var(--accent-soft)\|var(--accent-ring)\|var(--ok)\|var(--ok-soft)\|var(--warn)\|var(--warn-soft)\|var(--danger)\|var(--danger-soft)\|var(--row-hover)" src/routes/admin/
```

Expected: no matches (every page migrated in Tasks 30-59 stopped using these names). If any page still has a match, that page's migration task wasn't actually completed correctly — go fix it there rather than patching around it here.

- [ ] **Step 2: Delete the alias block**

Remove the entire `.shell { --bg: var(--bg-base); ... }` rule from `admin/+layout.svelte` (added in Task 2, Step 1) — every remaining style in `admin/+layout.svelte` at this point should reference the real token names (`--bg-base`, `--bg-surface`, etc.) directly, or belong to `NavRail`/`NavDrawer` (their own component files, untouched by this deletion).

- [ ] **Step 3: Confirm zero inline `<svg>` remains anywhere in admin**

```bash
cd frontend && grep -rln "<svg" src/routes/admin/ | grep -v node_modules
```

Expected: no matches at all — every admin page and `admin/+layout.svelte` itself should now render icons exclusively via `@lucide/svelte` components, per the Global Constraints.

- [ ] **Step 4: Full verification**

Run: `cd frontend && npm run check` — expect 0 errors.
Run: `cd frontend && npm run build` — expect success.

- [ ] **Step 5: Full manual walkthrough**

Click through all 32 admin routes (the 3 redirect stubs plus all 29 migrated pages) in both light and dark theme. This is the final acceptance check for the whole plan — every page should look and behave consistently, share the same nav shell, same button/badge/table/form treatment, and there should be no visual "seam" where one page still looks like the old admin and another looks like the new one.

- [ ] **Step 6: Commit**

```bash
cd frontend && git add src/routes/admin/+layout.svelte
git commit -m "cleanup(admin): remove the temporary token-alias bridge now that every page has migrated"
```

---

## Self-review notes

**Spec coverage:** every spec section has a corresponding task — token unification (Task 2, 60), `NavRail`/`NavDrawer` (Tasks 26-29), the full component inventory (Tasks 1, 3-25), `DataTable` client/server modes and its M3 pagination footer (Tasks 17, 19, wired in both modes across Tasks 31/42/45/46/47/52), the icon convention (every task; verified globally in Task 60), and all 32 admin pages (Tasks 30-59, plus the 3 redirect stubs explicitly noted as needing no task).

**Known within-plan corrections:** Task 29 Step 2 discovers mid-task that `NavRail`/`NavDrawer`'s `icon` prop can't be a `Snippet` given how their consumer (admin's `navGroups` array of plain objects) needs to supply per-item icons dynamically, and directs the implementer back to amend Tasks 26/27 before continuing — this is deliberately left in rather than "fixed" by silently rewriting Tasks 26/27 above, because it's a realistic instance of the "Rulings, not stalls" principle: a plan-level design gap discovered during execution, resolved in place with the reasoning shown, rather than pretending the first draft was already correct.

**Deferred, not forgotten:** mobile nav-rail behavior (Task 29 Step 4 leaves the old mobile topbar wired to a control that no longer exists — flagged with a code comment, not fixed, since this plan's approved mockups and interactive prototype were desktop-only); adding real backend pagination to `/admin/orgs` and `/admin/users` (both stay client-mode `DataTable`, per the spec's explicit scope boundary); migrating the main app (non-admin routes) onto the same component library (explicitly out of scope per the spec's header).
