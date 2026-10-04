# Design System — Main App Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move every non-admin page, layout, slide panel and page component of the Shipyard frontend onto the shared component library in `frontend/src/lib/components/ui/`, with every user flow behaving exactly as it does today.

**Architecture:** Foundation first — extend/add a small set of shared components (Tabs, KeyValueList, Card danger tone, StatusDot warning, Dropdown action menu + keyboard, Modal focus management, NavRail link items + footer + phone bar, NavDrawer header/footer/loading), then rebuild the org shell on NavRail + persistent NavDrawer, then migrate pages area by area, then the 31 slide panels and 19 page components, then delete the main app's old global CSS classes. Script logic is preserved; only markup and styling change.

**Tech Stack:** SvelteKit, Svelte 5 runes, TypeScript, `@lucide/svelte`, CSS custom properties from `frontend/src/routes/layout.css`. No frontend test framework — verification is `svelte-check`, `vite build`, and manual browser checks.

**Spec:** `docs/superpowers/specs/2026-10-04-design-system-main-app-migration-design.md` (read it before Task 1 — it is the binding authority; this plan argues from it).

**Structural precedent:** `docs/superpowers/plans/2026-09-29-design-system-admin-migration.md` and the already-migrated admin pages under `frontend/src/routes/admin/` are the reference implementations for every component's real usage.

## Global Constraints

- No new color, radius, or font tokens — use only tokens already defined in `frontend/src/routes/layout.css` (`--bg-base/-surface/-elevated/-hover`, `--accent` + `-hover`/`-muted`, `--accent-green/-red/-yellow` + `-muted`, `--border`/`-hover`, `--text-primary/-secondary/-muted/-dim`, `--font-sans`/`--font-mono`, `--radius-sm/-md/-lg`, `--shadow-sm/-md/-lg`, `--transition-fast/-normal/-slow`). `color-mix()` over existing tokens is allowed.
- Material 3 governs component structure, states and anatomy only — never a visual reskin with M3 default colors or type scale.
- Icons are `@lucide/svelte` only. Zero inline `<svg>` in migrated code.
- Svelte 5 runes only (`$props`, `$state`, `$derived`, `$bindable`, snippets). Never put a `class:name` directive on a Svelte *component* (native elements are fine) — pass a class string instead.
- Behavior is preserved: same routes, same `uiStore.pushPanel` stacking and callback props, same permission gating, same confirmation steps, same API calls. Script logic changes only where a component API forces it, and every forced change is disclosed in the task report.
- No backend changes. A `DataTable` that would need one uses client mode.
- `DataTable`: client mode (`items` + `searchFields`) unless the endpoint already returns a `total`; set `searchable={false}` unless the data source actually supports searching. Expandable rows are a second `<tr>` with one `<td colspan>` inside the `row` snippet.
- `PageHeader` only on standalone pages — never on a page whose layout already renders the page title (the settings and registry layouts do).
- `TextField type="number"` turns its bound value into a real JS `number` after the user edits it: any helper reading such a value must accept `string | number`.
- Async save/submit handlers must reset their loading flag in `finally`.
- Verification per task: `cd frontend && npm run check` shows no new errors or warnings (baseline before Task 1: `21 ERRORS 117 WARNINGS`, all in files outside this plan's scope or pre-existing), `cd frontend && npm run build` exits 0, and a manual browser check in light and dark theme. Anything touching navigation or slide panels is also checked at phone width (~400px).
- Never run destructive actions (delete, drop, prune, deploy, restart, revoke) against the shared dev environment during verification — open the confirmation, then cancel.
- Work happens on a dedicated branch in a git worktree, never directly on `main`.

## Review Focus

Failure modes the spec implies that per-file migrations are most likely to break. Each line is pinned to a check in the task that owns it.

1. **Phone width (≤639px):** the bottom navigation bar must appear, the project drawer must hide, slide panels must go full width, and page content must not hide behind the bar — pinned in Tasks 7, 8, 11, 85.
2. **Keyboard-only use:** the account menu must open, move and close by keyboard; dialogs must trap Tab and return focus to their trigger; button tabs must move with arrow keys — pinned in Tasks 3, 4, 5, 10.
3. **Permission-gated UI for non-admin members:** admin-only tabs, the update action, the admin-panel link and admin-only buttons must stay hidden exactly as today — pinned in Tasks 10, 32, 36, 37, and every panel task's manual check.
4. **Long text:** long org/project names, long values in key/value lists and long tab labels must truncate or wrap without widening the layout — pinned in Tasks 6, 8, 11.
5. **Empty, loading and error states:** no projects, an API failure, an empty table — each must still render a visible state, never a blank area — pinned in Tasks 8, 11 and every page task's manual check.

## Rulings made while planning

- **`Dropdown` gains an action-menu mode.** The spec says the account menu moves to `Dropdown` and `Dropdown` gains keyboard support, but today's `Dropdown` is a value picker (options + checkmark) and cannot render an account menu. Task 4 adds a `menu` snippet mode (consumer-rendered `role="menuitem"` items) plus `placement`, keeping the value-picker mode unchanged for `admin/payments`.
- **`StatusDot` gains a `warning` status, plus a shared `toDotStatus()` helper.** The global `.status-dot` supports 12 statuses; `StatusDot` has 5. `need_attention` is a static orange warning today — mapping it to `failed` overstates it and `pending` wrongly implies a transition. Task 1 adds a static yellow `warning` status and one mapping function so every file maps statuses identically.
- **`Button` gains a link mode (`href`, `target`, `rel`) and `title`.** The main app uses `<a class="btn …" href>` link-buttons and `title`-tooltip icon buttons throughout; without this, every one would need hand-styled local CSS. Added in Task 2, backward compatible.
- **Form controls forward native attributes.** `TextField` could not carry `autocomplete`/`name`/`required`, which login, register and profile forms depend on for password managers and browser validation; `Select` had no `disabled`, `Toggle` no change callback, `Textarea` no `spellcheck`/`readonly`. Dropping any of these would change a real flow, so Task 2 adds rest-spread passthrough of the standard HTML attribute types (plus `onchange` for `Toggle`/`Checkbox`), keeping every existing consumer working.
- **The New Project button keeps its current (no-op) behavior.** `ContextPanel`'s New Project button has no click handler today; flows must stay identical, so Task 11 keeps it handler-less and the task report flags it.
- **`routes/+error.svelte` is in scope.** It was not in the spec's page count (it is `+error`, not `+page`) but it contains inline `<svg>` and global button classes; Task 12 migrates it.
- **Specialised components get a chrome-only task.** `CodeEditor`, `FileTree`, `SandboxTerminal`, `BrandLogo` and the log body in `LogViewer` keep their core rendering; Task 83 only checks them for global classes and inline `<svg>`.

---

## Part A — Foundation

### Task 1: `StatusDot` warning status + `toDotStatus()` helper

**Files:**
- Modify: `frontend/src/lib/components/ui/StatusDot.svelte`
- Create: `frontend/src/lib/utils/status.ts`

**Interfaces:**
- Produces: `StatusDot` prop `status: 'running' | 'pending' | 'deploying' | 'warning' | 'failed' | 'stopped'`; `export type DotStatus`; `export function toDotStatus(status: string | null | undefined): DotStatus` from `$lib/utils/status`.

- [ ] **Step 1: Add the `warning` status to `StatusDot`**

Replace the `Props` interface and add one style rule:

```svelte
<script lang="ts">
	import type { DotStatus } from '$lib/utils/status';

	interface Props {
		status: DotStatus;
	}

	let { status }: Props = $props();
</script>
```

```css
	.ui-dot--warning   { background: var(--accent-yellow); }
```

(Place the new rule directly after `.ui-dot--deploying`. Leave every other rule unchanged.)

- [ ] **Step 2: Create the helper**

`frontend/src/lib/utils/status.ts`:

```ts
export type DotStatus = 'running' | 'pending' | 'deploying' | 'warning' | 'failed' | 'stopped';

// Maps every status the old global `.status-dot` class supported onto
// StatusDot's states. Pulsing (pending/deploying) is reserved for genuinely
// transitional states.
const DOT_STATUS: Record<string, DotStatus> = {
	running: 'running',
	pending: 'pending',
	preparing: 'pending',
	queued: 'pending',
	stopping: 'pending',
	deploying: 'deploying',
	need_attention: 'warning',
	failed: 'failed',
	rejected: 'failed',
	stopped: 'stopped',
	shutdown: 'stopped',
	complete: 'stopped'
};

export function toDotStatus(status: string | null | undefined): DotStatus {
	return DOT_STATUS[(status ?? '').toLowerCase()] ?? 'stopped';
}
```

- [ ] **Step 3: Verify**

Run: `cd frontend && npm run check` — expected: same error/warning totals as the baseline, none in these two files.
Run: `cd frontend && npm run build` — expected: exit 0.
Open `/admin/infra` and `/admin/nodes` (existing `StatusDot` consumers) in both themes — dots render unchanged.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/components/ui/StatusDot.svelte frontend/src/lib/utils/status.ts
git commit -m "feat(ui): add StatusDot warning status and toDotStatus helper"
```

---

### Task 2: Base component extensions — `Card` tone, `Button` link mode, form-control passthrough

**Files:**
- Modify: `frontend/src/lib/components/ui/Card.svelte`
- Modify: `frontend/src/lib/components/ui/Button.svelte`
- Modify: `frontend/src/lib/components/ui/TextField.svelte`, `Textarea.svelte`, `Select.svelte`, `Toggle.svelte`, `Checkbox.svelte`

**Interfaces:**
- Produces: `Card` prop `tone?: 'default' | 'danger'` (default `'default'`).
- Produces: `Button` props `href?: string` (renders an `<a>` with the same classes; `disabled` is ignored for links), `target?: string`, `rel?: string`, `title?: string`. Existing props unchanged. The main app's `<a class="btn btn-*" href>` link-buttons and `title`-tooltip icon buttons map onto these.

- [ ] **Step 0: Add link mode and `title` to `Button`**

In `Button.svelte`, extend `Props` and the destructure:

```ts
		href?: string;
		target?: string;
		rel?: string;
		title?: string;
```

```ts
		href,
		target,
		rel,
		title,
```

Replace the `<button …>` element with:

```svelte
{#if href}
	<a
		{href}
		{target}
		{rel}
		{title}
		{onclick}
		class="ui-btn ui-btn--{variant} ui-btn--{size}"
		aria-label={ariaLabel}
	>
		{@render children()}
	</a>
{:else}
	<button
		{type}
		{disabled}
		{title}
		class="ui-btn ui-btn--{variant} ui-btn--{size}"
		{onclick}
		aria-label={ariaLabel}
	>
		{@render children()}
	</button>
{/if}
```

Add `text-decoration: none;` to the `.ui-btn` rule. Leave every other style unchanged.

- [ ] **Step 0b: Pass native attributes through the form controls**

The main app's forms rely on native input attributes (`autocomplete`/`name`/`required` for login and password managers, `min`/`max`/`step`, `readonly`, `maxlength`, `oninput`/`onkeydown`/`onblur` handlers, `spellcheck`), on disabled selects/checkboxes, and on reacting to toggle changes. Losing any of these would change a flow, so every control forwards the standard HTML attributes. Existing consumers keep working: every prop they pass today is still accepted.

`TextField.svelte` — replace the script and both `<input>` elements:

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLInputAttributes } from 'svelte/elements';

	interface Props extends Omit<HTMLInputAttributes, 'value' | 'type' | 'class'> {
		value: string;
		type?: string;
		icon?: Snippet;
	}

	let { value = $bindable(), type = 'text', icon, ...rest }: Props = $props();
</script>

{#if icon}
	<div class="ui-textfield-icon-wrap">
		<span class="ui-textfield-icon">{@render icon()}</span>
		<input {...rest} {type} bind:value class="ui-textfield ui-textfield--with-icon" />
	</div>
{:else}
	<input {...rest} {type} bind:value class="ui-textfield" />
{/if}
```

Add `.ui-textfield:read-only { color: var(--text-muted); }` to its styles.

`Textarea.svelte`:

```svelte
<script lang="ts">
	import type { HTMLTextareaAttributes } from 'svelte/elements';

	interface Props extends Omit<HTMLTextareaAttributes, 'value' | 'class'> {
		value: string;
		rows?: number;
	}

	let { value = $bindable(), rows = 3, ...rest }: Props = $props();
</script>

<textarea {...rest} {rows} bind:value class="ui-textarea"></textarea>
```

`Select.svelte`:

```svelte
<script lang="ts">
	import type { HTMLSelectAttributes } from 'svelte/elements';

	interface Option {
		value: string;
		label: string;
		disabled?: boolean;
	}

	interface Props extends Omit<HTMLSelectAttributes, 'value' | 'class'> {
		value: string;
		options: Option[];
	}

	let { value = $bindable(), options, ...rest }: Props = $props();
</script>

<select {...rest} bind:value class="ui-select">
	{#each options as opt (opt.value)}
		<option value={opt.value} disabled={opt.disabled}>{opt.label}</option>
	{/each}
</select>
```

Add `.ui-select:disabled { opacity: 0.5; cursor: not-allowed; }`.

`Toggle.svelte` — add `onchange?: (checked: boolean) => void;` to `Props`, destructure it, and change the click handler to:

```svelte
	onclick={() => { checked = !checked; onchange?.(checked); }}
```

`Checkbox.svelte` — add `disabled?: boolean;` and `onchange?: (checked: boolean) => void;` to `Props`, destructure them (`disabled = false`), change the input to:

```svelte
	<input type="checkbox" bind:checked {disabled} onchange={() => onchange?.(checked)} />
```

and add `.ui-checkbox:has(input:disabled) { opacity: 0.5; cursor: not-allowed; }`.

- [ ] **Step 1: Replace the component**

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		padding?: string;
		tone?: 'default' | 'danger';
		children: Snippet;
	}

	let { padding = '16px', tone = 'default', children }: Props = $props();
</script>

<div class="ui-card ui-card--{tone}" style="padding:{padding}">
	{@render children()}
</div>

<style>
	.ui-card {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
	}
	.ui-card--danger {
		border-color: color-mix(in srgb, var(--accent-red) 45%, var(--border));
	}
</style>
```

- [ ] **Step 2: Verify**

Run: `cd frontend && npm run check` and `cd frontend && npm run build`.
Open `/admin/storage` and `/admin/config` (existing `Card` consumers) and `/admin/orgs` (many `Button` consumers) — unchanged in both themes. Temporarily render `<Button href="/admin" variant="secondary">Link</Button>` on any admin page: it looks identical to a secondary button, navigates on click, and has no underline. Remove the scratch usage.
Form controls: `/admin/smtp` (TextField incl. `type="number"`, Select, Toggle) and `/admin/staff` → **+ Add Staff** (Checkbox) and `/admin/config` (Textarea) still render, bind and save exactly as before.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/components/ui/Card.svelte frontend/src/lib/components/ui/Button.svelte frontend/src/lib/components/ui/TextField.svelte frontend/src/lib/components/ui/Textarea.svelte frontend/src/lib/components/ui/Select.svelte frontend/src/lib/components/ui/Toggle.svelte frontend/src/lib/components/ui/Checkbox.svelte
git commit -m "feat(ui): Card danger tone, Button link mode, native attribute passthrough for form controls"
```

---

### Task 3: `Modal` focus trap, focus restore, unique ids

**Files:**
- Modify: `frontend/src/lib/components/ui/Modal.svelte`
- Modify: `frontend/src/lib/components/ui/ConfirmDialog.svelte`

**Interfaces:**
- Produces: unchanged public props for both components. Behavior added: Tab/Shift+Tab cycle inside the dialog; focus returns to the element that opened it; each instance gets a unique `aria-labelledby` id.

- [ ] **Step 1: Replace `Modal.svelte`'s script and dialog element**

```svelte
<script lang="ts" module>
	let modalCounter = 0;
</script>

<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		open: boolean;
		title: string;
		children: Snippet;
		footer?: Snippet;
	}

	let { open = $bindable(), title, children, footer }: Props = $props();

	const titleId = `ui-modal-title-${++modalCounter}`;
	const FOCUSABLE =
		'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

	let dialogEl: HTMLDivElement | undefined = $state();
	let returnFocusTo: HTMLElement | null = null;

	function close() {
		open = false;
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.stopPropagation();
			close();
			return;
		}
		if (e.key !== 'Tab' || !dialogEl) return;
		const items = [...dialogEl.querySelectorAll<HTMLElement>(FOCUSABLE)];
		if (items.length === 0) {
			e.preventDefault();
			dialogEl.focus();
			return;
		}
		const first = items[0];
		const last = items[items.length - 1];
		const active = document.activeElement;
		if (e.shiftKey && (active === first || active === dialogEl)) {
			e.preventDefault();
			last.focus();
		} else if (!e.shiftKey && active === last) {
			e.preventDefault();
			first.focus();
		}
	}

	$effect(() => {
		if (!open) return;
		returnFocusTo = document.activeElement as HTMLElement | null;
		dialogEl?.focus();
		return () => returnFocusTo?.focus();
	});
</script>

{#if open}
	<div class="ui-modal-scrim" role="presentation" onclick={close}>
		<div
			class="ui-modal"
			role="dialog"
			aria-modal="true"
			aria-labelledby={titleId}
			tabindex="-1"
			bind:this={dialogEl}
			onclick={(e) => e.stopPropagation()}
			onkeydown={onKeydown}
		>
			<div class="ui-modal-header">
				<span id={titleId}>{title}</span>
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
```

Keep the existing `<style>` block unchanged, and add `outline: none;` to `.ui-modal`.

- [ ] **Step 2: Give `ConfirmDialog`'s type-to-confirm input a unique id**

At the top of `ConfirmDialog.svelte` add a module script, and use the id in the label and field:

```svelte
<script lang="ts" module>
	let confirmCounter = 0;
</script>
```

In the instance script, after the props: `const inputId = \`ui-confirm-type-input-${++confirmCounter}\`;`
Replace both occurrences of `ui-confirm-type-input` in the template with `{inputId}` (`for={inputId}` on the label, `id={inputId}` on the `TextField`).

- [ ] **Step 3: Verify**

Run: `cd frontend && npm run check` and `cd frontend && npm run build`.
Manual (Review Focus #2): open `/admin/plan`, click **+ New Plan** → focus lands in the dialog; press Tab repeatedly → focus cycles inside the dialog and never reaches the page behind; press Escape → dialog closes and focus is back on **+ New Plan**. Open `/admin/staff` → **Revoke** on a row (if no staff exist, open `/admin/docker/images` → **Prune Unused**) → Cancel → focus returns to the button. Both themes.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/components/ui/Modal.svelte frontend/src/lib/components/ui/ConfirmDialog.svelte
git commit -m "feat(ui): trap and restore focus in Modal, unique dialog ids"
```

---

### Task 4: `Dropdown` action-menu mode, placement, keyboard

**Files:**
- Modify: `frontend/src/lib/components/ui/Dropdown.svelte`

**Interfaces:**
- Produces:
  - Existing value-picker mode unchanged: `options: { value: string; label: string }[]`, `value` (bindable), `trigger` snippet.
  - New action-menu mode: `menu: Snippet<[close: () => void]>` — consumer renders items as `<button class="ui-dropdown-item" role="menuitem">` / `<a class="ui-dropdown-item" role="menuitem">`; optional classes `ui-dropdown-item--danger`, `ui-dropdown-sep` (divider), `ui-dropdown-header` (non-interactive header block).
  - `placement?: 'bottom-start' | 'bottom-end' | 'top-start' | 'right-end'` (default `'bottom-start'`).
  - `triggerLabel?: string` (accessible name for an icon/avatar trigger).
  - Keyboard: ArrowDown/Enter/Space on the trigger opens and focuses the first item; ArrowUp/ArrowDown/Home/End move; Escape closes and returns focus to the trigger; Tab closes.

- [ ] **Step 1: Replace the component**

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';
	import { Check } from '@lucide/svelte';

	interface Option {
		value: string;
		label: string;
	}

	interface Props {
		trigger: Snippet;
		options?: Option[];
		value?: string;
		menu?: Snippet<[() => void]>;
		placement?: 'bottom-start' | 'bottom-end' | 'top-start' | 'right-end';
		triggerLabel?: string;
	}

	let {
		trigger,
		options = [],
		value = $bindable(''),
		menu,
		placement = 'bottom-start',
		triggerLabel
	}: Props = $props();

	let open = $state(false);
	let rootEl: HTMLDivElement | undefined = $state();
	let triggerEl: HTMLButtonElement | undefined = $state();
	let menuEl: HTMLDivElement | undefined = $state();

	function items(): HTMLElement[] {
		if (!menuEl) return [];
		return [
			...menuEl.querySelectorAll<HTMLElement>('[role="menuitem"]:not([disabled]):not([aria-disabled="true"])')
		];
	}

	function openMenu() {
		open = true;
		queueMicrotask(() => items()[0]?.focus());
	}

	function close(restoreFocus = true) {
		open = false;
		if (restoreFocus) triggerEl?.focus();
	}

	function select(v: string) {
		value = v;
		close();
	}

	function onTriggerKeydown(e: KeyboardEvent) {
		if (e.key === 'ArrowDown' || e.key === 'Enter' || e.key === ' ') {
			e.preventDefault();
			openMenu();
		}
	}

	function onMenuKeydown(e: KeyboardEvent) {
		const list = items();
		const i = list.indexOf(document.activeElement as HTMLElement);
		if (e.key === 'ArrowDown') {
			e.preventDefault();
			list[(i + 1) % list.length]?.focus();
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			list[(i - 1 + list.length) % list.length]?.focus();
		} else if (e.key === 'Home') {
			e.preventDefault();
			list[0]?.focus();
		} else if (e.key === 'End') {
			e.preventDefault();
			list[list.length - 1]?.focus();
		} else if (e.key === 'Escape') {
			e.preventDefault();
			e.stopPropagation();
			close();
		} else if (e.key === 'Tab') {
			close(false);
		}
	}

	function onWindowClick(e: MouseEvent) {
		if (open && rootEl && !rootEl.contains(e.target as Node)) close(false);
	}
</script>

<svelte:window onclick={onWindowClick} />

<div class="ui-dropdown" bind:this={rootEl}>
	<button
		bind:this={triggerEl}
		type="button"
		class="ui-dropdown-trigger"
		aria-haspopup="menu"
		aria-expanded={open}
		aria-label={triggerLabel}
		onclick={() => (open ? close(false) : openMenu())}
		onkeydown={onTriggerKeydown}
	>
		{@render trigger()}
	</button>
	{#if open}
		<div
			bind:this={menuEl}
			class="ui-dropdown-menu ui-dropdown-menu--{placement}"
			role="menu"
			tabindex="-1"
			onkeydown={onMenuKeydown}
		>
			{#if menu}
				{@render menu(() => close(false))}
			{:else}
				{#each options as opt (opt.value)}
					<button type="button" class="ui-dropdown-item" role="menuitem" onclick={() => select(opt.value)}>
						<span class="ui-dropdown-check">{#if opt.value === value}<Check size={13} />{/if}</span>
						{opt.label}
					</button>
				{/each}
			{/if}
		</div>
	{/if}
</div>

<style>
	.ui-dropdown { position: relative; display: inline-block; }
	.ui-dropdown-trigger { background: none; border: none; padding: 0; cursor: pointer; border-radius: var(--radius-md); }
	.ui-dropdown-trigger:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
	.ui-dropdown-menu {
		position: absolute;
		min-width: 180px;
		max-height: 320px;
		overflow-y: auto;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-md);
		z-index: 70;
		padding: 4px;
		outline: none;
	}
	.ui-dropdown-menu--bottom-start { top: calc(100% + 6px); left: 0; }
	.ui-dropdown-menu--bottom-end   { top: calc(100% + 6px); right: 0; }
	.ui-dropdown-menu--top-start    { bottom: calc(100% + 6px); left: 0; }
	.ui-dropdown-menu--right-end    { left: calc(100% + 8px); bottom: 0; }

	.ui-dropdown-menu :global(.ui-dropdown-item) {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 7px 9px;
		background: none;
		border: none;
		border-radius: var(--radius-sm);
		font-size: 12.5px;
		font-family: var(--font-sans);
		color: var(--text-secondary);
		text-align: left;
		text-decoration: none;
		cursor: pointer;
	}
	.ui-dropdown-menu :global(.ui-dropdown-item:hover),
	.ui-dropdown-menu :global(.ui-dropdown-item:focus-visible) {
		background: var(--bg-hover);
		color: var(--text-primary);
		outline: none;
	}
	.ui-dropdown-menu :global(.ui-dropdown-item:disabled) { opacity: 0.5; cursor: not-allowed; }
	.ui-dropdown-menu :global(.ui-dropdown-item--danger) { color: var(--accent-red); }
	.ui-dropdown-menu :global(.ui-dropdown-item--danger:hover),
	.ui-dropdown-menu :global(.ui-dropdown-item--danger:focus-visible) { background: var(--accent-red-muted); color: var(--accent-red); }
	.ui-dropdown-menu :global(.ui-dropdown-sep) { height: 1px; background: var(--border); margin: 4px 0; }
	.ui-dropdown-menu :global(.ui-dropdown-header) { padding: 8px 9px; }
	.ui-dropdown-check { width: 13px; display: flex; color: var(--accent); flex-shrink: 0; }
</style>
```

- [ ] **Step 2: Verify**

Run: `cd frontend && npm run check` — expected: the two pre-existing `Dropdown.svelte` a11y warnings are gone or unchanged; no new ones. Run `npm run build`.
Manual: `/admin/payments` (value-picker consumer) — the status filter still opens, selects, closes on outside click. Keyboard (Review Focus #2): Tab to the trigger, press ArrowDown → first option focused; ArrowDown/ArrowUp move; Escape closes and focus is back on the trigger. Both themes.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/components/ui/Dropdown.svelte
git commit -m "feat(ui): add Dropdown action-menu mode, placement and keyboard support"
```

---

### Task 5: `Tabs` (new)

**Files:**
- Create: `frontend/src/lib/components/ui/Tabs.svelte`
- Modify: `frontend/src/lib/components/ui/index.ts`

**Interfaces:**
- Produces: `Tabs` with props `tabs: TabItem[]`, `value: string` (bindable — the active tab id), `onChange?: (id: string) => void`, `ariaLabel?: string`; `export interface TabItem { id: string; label: string; href?: string; icon?: typeof Anchor; badge?: string; danger?: boolean; disabled?: boolean }`, exported from the barrel as `Tabs` and `type TabItem`.
  - **Link mode** (every tab has `href`): renders `<nav>` of links; the consumer computes `value` from the current route.
  - **Button mode** (no `href`): renders `role="tablist"`; clicking sets `value` and calls `onChange`; ArrowLeft/ArrowRight/Home/End move and activate. Use `bind:value`, or pass `value` + `onChange` when switching tabs must run a loader (e.g. `onChange={(id) => switchTab(id as Tab)}`).

- [ ] **Step 1: Create `Tabs.svelte`**

```svelte
<script lang="ts" module>
	import type { Anchor } from '@lucide/svelte';

	export interface TabItem {
		id: string;
		label: string;
		href?: string;
		icon?: typeof Anchor;
		badge?: string;
		danger?: boolean;
		disabled?: boolean;
	}
</script>

<script lang="ts">
	import Badge from './Badge.svelte';

	interface Props {
		tabs: TabItem[];
		value: string;
		onChange?: (id: string) => void;
		ariaLabel?: string;
	}

	let { tabs, value = $bindable(), onChange, ariaLabel }: Props = $props();

	let isLinkMode = $derived(tabs.length > 0 && tabs.every((t) => !!t.href));
	let listEl: HTMLDivElement | undefined = $state();

	function activate(id: string) {
		if (id === value) return;
		value = id;
		onChange?.(id);
	}

	function onKeydown(e: KeyboardEvent) {
		const enabled = tabs.filter((t) => !t.disabled);
		if (enabled.length === 0) return;
		const i = enabled.findIndex((t) => t.id === value);
		let next = -1;
		if (e.key === 'ArrowRight') next = (i + 1) % enabled.length;
		else if (e.key === 'ArrowLeft') next = (i - 1 + enabled.length) % enabled.length;
		else if (e.key === 'Home') next = 0;
		else if (e.key === 'End') next = enabled.length - 1;
		if (next < 0) return;
		e.preventDefault();
		activate(enabled[next].id);
		listEl?.querySelector<HTMLElement>(`[data-tab-id="${enabled[next].id}"]`)?.focus();
	}
</script>

{#if isLinkMode}
	<nav class="ui-tabs" aria-label={ariaLabel}>
		{#each tabs as tab (tab.id)}
			{@const Icon = tab.icon}
			<a
				href={tab.href}
				class="ui-tab"
				class:ui-tab--active={tab.id === value}
				class:ui-tab--danger={tab.danger}
				aria-current={tab.id === value ? 'page' : undefined}
			>
				{#if Icon}<Icon size={14} />{/if}
				<span>{tab.label}</span>
				{#if tab.badge}<Badge tone="neutral">{tab.badge}</Badge>{/if}
			</a>
		{/each}
	</nav>
{:else}
	<div class="ui-tabs" role="tablist" aria-label={ariaLabel} tabindex="-1" bind:this={listEl} onkeydown={onKeydown}>
		{#each tabs as tab (tab.id)}
			{@const Icon = tab.icon}
			<button
				type="button"
				role="tab"
				data-tab-id={tab.id}
				aria-selected={tab.id === value}
				tabindex={tab.id === value ? 0 : -1}
				disabled={tab.disabled}
				class="ui-tab"
				class:ui-tab--active={tab.id === value}
				class:ui-tab--danger={tab.danger}
				onclick={() => activate(tab.id)}
			>
				{#if Icon}<Icon size={14} />{/if}
				<span>{tab.label}</span>
				{#if tab.badge}<Badge tone="neutral">{tab.badge}</Badge>{/if}
			</button>
		{/each}
	</div>
{/if}

<style>
	.ui-tabs {
		display: flex;
		gap: 2px;
		border-bottom: 1px solid var(--border);
		overflow-x: auto;
		scrollbar-width: none;
		outline: none;
	}
	.ui-tabs::-webkit-scrollbar { display: none; }
	.ui-tab {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 8px 12px;
		font-size: 12.5px;
		font-weight: 500;
		font-family: var(--font-sans);
		color: var(--text-muted);
		background: none;
		border: none;
		border-bottom: 2px solid transparent;
		margin-bottom: -1px;
		cursor: pointer;
		white-space: nowrap;
		text-decoration: none;
		transition: color var(--transition-fast), border-color var(--transition-fast);
	}
	.ui-tab:hover { color: var(--text-primary); }
	.ui-tab:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; border-radius: var(--radius-sm); }
	.ui-tab:disabled { opacity: 0.5; cursor: not-allowed; }
	.ui-tab--active { color: var(--accent); border-bottom-color: var(--accent); font-weight: 600; }
	.ui-tab--danger:hover,
	.ui-tab--danger.ui-tab--active { color: var(--accent-red); }
	.ui-tab--danger.ui-tab--active { border-bottom-color: var(--accent-red); }
</style>
```

- [ ] **Step 2: Export it**

Append to `frontend/src/lib/components/ui/index.ts`:

```ts
export { default as Tabs } from './Tabs.svelte';
export type { TabItem } from './Tabs.svelte';
```

- [ ] **Step 3: Verify**

Run: `cd frontend && npm run check` and `npm run build`.
Manual: temporarily render `<Tabs tabs={[{id:'a',label:'Overview'},{id:'b',label:'Settings'},{id:'c',label:'Danger',danger:true}]} bind:value={x} />` on any admin page in the dev server (do not commit the scratch usage) — click switches; with focus on a tab, ArrowRight/ArrowLeft move and activate; the Danger tab turns red when active; long labels scroll horizontally instead of wrapping. Remove the scratch usage before committing.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/components/ui/Tabs.svelte frontend/src/lib/components/ui/index.ts
git commit -m "feat(ui): add Tabs component (link and button modes, danger tab)"
```

---

### Task 6: `KeyValueList` (new)

**Files:**
- Create: `frontend/src/lib/components/ui/KeyValueList.svelte`
- Modify: `frontend/src/lib/components/ui/index.ts`

**Interfaces:**
- Produces: `KeyValueList` with props `items: KeyValueItem[]`, `keyWidth?: string` (default `'110px'`), `value?: Snippet<[KeyValueItem]>` (custom value rendering, e.g. a `StatusDot` + text or a link), `action?: Snippet<[KeyValueItem]>` (trailing per-row action); `export interface KeyValueItem { key: string; value?: string | number | null; mono?: boolean }`. Empty/null values render `—`.

- [ ] **Step 1: Create `KeyValueList.svelte`**

```svelte
<script lang="ts" module>
	export interface KeyValueItem {
		key: string;
		value?: string | number | null;
		mono?: boolean;
	}
</script>

<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		items: KeyValueItem[];
		keyWidth?: string;
		value?: Snippet<[KeyValueItem]>;
		action?: Snippet<[KeyValueItem]>;
	}

	let { items, keyWidth = '110px', value, action }: Props = $props();

	function display(v: KeyValueItem['value']): string {
		return v === null || v === undefined || v === '' ? '—' : String(v);
	}
</script>

<div class="ui-kv">
	{#each items as item}
		<div class="ui-kv-row">
			<span class="ui-kv-key" style="width:{keyWidth}">{item.key}</span>
			<span
				class="ui-kv-value"
				class:ui-kv-value--mono={item.mono}
				title={value ? undefined : display(item.value)}
			>
				{#if value}{@render value(item)}{:else}{display(item.value)}{/if}
			</span>
			{#if action}<span class="ui-kv-action">{@render action(item)}</span>{/if}
		</div>
	{/each}
</div>

<style>
	.ui-kv {
		display: flex;
		flex-direction: column;
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		overflow: hidden;
		background: var(--bg-surface);
	}
	.ui-kv-row {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 12px;
		font-size: 12.5px;
		border-bottom: 1px solid var(--border);
		min-width: 0;
	}
	.ui-kv-row:last-child { border-bottom: none; }
	.ui-kv-key { color: var(--text-muted); flex-shrink: 0; }
	.ui-kv-value {
		flex: 1;
		min-width: 0;
		color: var(--text-primary);
		font-weight: 500;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.ui-kv-value--mono { font-family: var(--font-mono); font-size: 12px; }
	.ui-kv-action { flex-shrink: 0; display: flex; align-items: center; }
</style>
```

- [ ] **Step 2: Export it**

Append to `frontend/src/lib/components/ui/index.ts`:

```ts
export { default as KeyValueList } from './KeyValueList.svelte';
export type { KeyValueItem } from './KeyValueList.svelte';
```

- [ ] **Step 3: Verify**

Run: `cd frontend && npm run check` and `npm run build`.
Manual (Review Focus #4): temporarily render `<KeyValueList items={[{key:'Image',value:'nginx:alpine'},{key:'Container ID',value:'89e488947b17db15d68646a09c5cfaca97710129691629ecda3826864b548316',mono:true},{key:'Empty',value:null}]} />` inside a 320px-wide wrapper on any page — the long ID truncates with an ellipsis (full value on hover), `Empty` shows `—`, the wrapper does not widen. Remove the scratch usage before committing.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/components/ui/KeyValueList.svelte frontend/src/lib/components/ui/index.ts
git commit -m "feat(ui): add KeyValueList component"
```

---

### Task 7: `NavRail` link items, footer slot, phone bottom bar

**Files:**
- Modify: `frontend/src/lib/components/ui/NavRail.svelte`

**Interfaces:**
- Consumes: current `NavRail` props (`groups`, `activeGroup` bindable, `currentGroup`, `labelOverflow`, `onSelectGroup`) — admin's usage must keep working unchanged.
- Produces:
  - `Group` gains `href?: string` and `active?: boolean`. A group with `href` renders as a link (`aria-current="page"` when `active`) instead of a drawer-toggle button.
  - `onSelectGroup` becomes optional (link-only rails don't need it).
  - `footer?: Snippet` — rendered at the bottom of the rail.
  - `phoneBar?: boolean` (default `false`) — at ≤639px the rail becomes a fixed 56px bottom navigation bar: logo hidden, items spread across, footer laid out in a row. Consumers hide individual footer controls at phone width with their own CSS.

- [ ] **Step 1: Replace the component**

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';
	import { Anchor } from '@lucide/svelte';

	interface Group {
		key: string;
		icon: typeof Anchor;
		label: string;
		href?: string;
		active?: boolean;
	}

	interface Props {
		groups: Group[];
		activeGroup?: string | null;
		/** Group containing the current route; highlighted when no drawer is open. */
		currentGroup?: string | null;
		/** How a label wider than the rail is handled: truncated with "…" or wrapped onto more lines. */
		labelOverflow?: 'ellipsis' | 'wrap';
		onSelectGroup?: (key: string | null) => void;
		footer?: Snippet;
		/** At ≤639px render as a bottom navigation bar instead of a side rail. */
		phoneBar?: boolean;
	}

	let {
		groups,
		activeGroup = $bindable(null),
		currentGroup = null,
		labelOverflow = 'ellipsis',
		onSelectGroup,
		footer,
		phoneBar = false
	}: Props = $props();

	function handleClick(key: string) {
		onSelectGroup?.(activeGroup === key ? null : key);
	}

	function isHighlighted(g: Group): boolean {
		if (g.href) return !!g.active;
		return activeGroup === g.key || (activeGroup === null && currentGroup === g.key);
	}
</script>

<aside class="ui-nav-rail" class:ui-nav-rail--phone-bar={phoneBar}>
	<div class="ui-nav-rail-logo">
		<Anchor size={16} strokeWidth={2.5} />
	</div>
	<nav class="ui-nav-rail-items">
		{#each groups as g (g.key)}
			{@const Icon = g.icon}
			<div class="ui-nav-rail-group">
				{#if g.href}
					<a
						href={g.href}
						class="ui-nav-rail-btn"
						class:ui-nav-rail-btn--active={isHighlighted(g)}
						aria-label={g.label}
						aria-current={g.active ? 'page' : undefined}
					>
						<Icon size={20} />
					</a>
				{:else}
					<button
						type="button"
						class="ui-nav-rail-btn"
						class:ui-nav-rail-btn--active={isHighlighted(g)}
						onclick={() => handleClick(g.key)}
						aria-label={g.label}
						aria-expanded={activeGroup === g.key}
					>
						<Icon size={20} />
					</button>
				{/if}
				<span
					class="ui-nav-rail-label ui-nav-rail-label--{labelOverflow}"
					title={labelOverflow === 'ellipsis' ? g.label : undefined}
				>{g.label}</span>
			</div>
		{/each}
	</nav>
	{#if footer}
		<div class="ui-nav-rail-footer">{@render footer()}</div>
	{/if}
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
		width: 100%;
		padding: 0 3px;
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
		text-decoration: none;
		transition: background var(--transition-fast), color var(--transition-fast);
	}
	.ui-nav-rail-btn:hover { background: var(--bg-hover); color: var(--accent); }
	.ui-nav-rail-btn:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
	.ui-nav-rail-btn--active { background: var(--accent-muted); color: var(--accent); }
	.ui-nav-rail-label {
		font-size: 8.5px;
		font-weight: 600;
		color: var(--text-dim);
		max-width: 100%;
		text-align: center;
	}
	.ui-nav-rail-label--ellipsis {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.ui-nav-rail-label--wrap {
		white-space: normal;
		overflow-wrap: anywhere;
		line-height: 1.2;
	}
	.ui-nav-rail-footer {
		margin-top: auto;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
		width: 100%;
	}

	@media (max-width: 639px) {
		.ui-nav-rail--phone-bar {
			position: fixed;
			left: 0;
			right: 0;
			bottom: 0;
			width: 100%;
			height: 56px;
			flex-direction: row;
			padding: 0 4px;
			gap: 0;
			border-right: none;
			border-top: 1px solid var(--border);
			z-index: 60;
		}
		.ui-nav-rail--phone-bar .ui-nav-rail-logo { display: none; }
		.ui-nav-rail--phone-bar .ui-nav-rail-items {
			flex-direction: row;
			justify-content: space-around;
			flex: 1;
			gap: 0;
		}
		.ui-nav-rail--phone-bar .ui-nav-rail-group { width: auto; padding: 0 6px; }
		.ui-nav-rail--phone-bar .ui-nav-rail-btn { width: 36px; height: 32px; }
		.ui-nav-rail--phone-bar .ui-nav-rail-footer {
			flex-direction: row;
			margin-top: 0;
			width: auto;
			gap: 2px;
		}
	}
</style>
```

- [ ] **Step 2: Verify**

Run: `cd frontend && npm run check` and `npm run build`.
Manual: `/admin/orgs` — admin's rail is unchanged: drawer toggle buttons, `currentGroup` highlight, ellipsis label for "Infrastructure", footer overlay (admin still uses its own fixed `.rail-footer`). Resize to 400px wide — admin's rail is unchanged (it does not pass `phoneBar`). Both themes.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/components/ui/NavRail.svelte
git commit -m "feat(ui): NavRail link items, footer slot and phone bottom-bar mode"
```

---

### Task 8: `NavDrawer` header, footer, loading, empty state, phone hiding

**Files:**
- Modify: `frontend/src/lib/components/ui/NavDrawer.svelte`

**Interfaces:**
- Consumes: current props (`open`, `title`, `items`, `onNavigate`, `persistent`, `onClose`) — admin's usage must keep working unchanged.
- Produces:
  - `title?: string` (optional now; rendered as the section label under the header).
  - Item `icon?` is optional; an item without an icon shows a small dot.
  - `header?: Snippet` (above the title), `footer?: Snippet` (pinned to the drawer's bottom).
  - `loading?: boolean` (shows a loading row instead of items), `emptyText?: string` (shown when not loading and `items` is empty).
  - `hideOnPhone?: boolean` (default `false`) — hidden entirely at ≤639px.

- [ ] **Step 1: Replace the component**

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { Anchor } from '@lucide/svelte';
	import Spinner from './Spinner.svelte';

	interface Item {
		href: string;
		icon?: typeof Anchor;
		label: string;
		active: boolean;
	}

	interface Props {
		open: boolean;
		title?: string;
		items: Item[];
		onNavigate: () => void;
		/** Persistent: docked beside the content, pushes it, stays open. Modal (default): overlays the content and closes on outside click, Escape, or item click. */
		persistent?: boolean;
		onClose?: () => void;
		header?: Snippet;
		footer?: Snippet;
		loading?: boolean;
		emptyText?: string;
		hideOnPhone?: boolean;
	}

	let {
		open,
		title,
		items,
		onNavigate,
		persistent = false,
		onClose,
		header,
		footer,
		loading = false,
		emptyText,
		hideOnPhone = false
	}: Props = $props();

	function handleKeydown(e: KeyboardEvent) {
		if (open && !persistent && e.key === 'Escape') onClose?.();
	}
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open && !persistent}
	<div class="ui-nav-drawer-scrim" role="presentation" onclick={() => onClose?.()}></div>
{/if}

<div
	class="ui-nav-drawer"
	class:ui-nav-drawer--open={open}
	class:ui-nav-drawer--persistent={persistent}
	class:ui-nav-drawer--hide-phone={hideOnPhone}
>
	<div class="ui-nav-drawer-inner">
		{#if header}<div class="ui-nav-drawer-header">{@render header()}</div>{/if}
		{#if title}<div class="ui-nav-drawer-title">{title}</div>{/if}
		<div class="ui-nav-drawer-items">
			{#if loading}
				<div class="ui-nav-drawer-state"><Spinner size={13} /> Loading…</div>
			{:else if items.length === 0 && emptyText}
				<div class="ui-nav-drawer-state">{emptyText}</div>
			{:else}
				{#each items as item (item.href)}
					{@const Icon = item.icon}
					<a
						href={item.href}
						class="ui-nav-drawer-item"
						class:ui-nav-drawer-item--active={item.active}
						aria-current={item.active ? 'page' : undefined}
						onclick={onNavigate}
					>
						<span class="ui-nav-drawer-icon">
							{#if Icon}<Icon size={16} />{:else}<span class="ui-nav-drawer-dot"></span>{/if}
						</span>
						<span class="ui-nav-drawer-label">{item.label}</span>
					</a>
				{/each}
			{/if}
		</div>
		{#if footer}<div class="ui-nav-drawer-footer">{@render footer()}</div>{/if}
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
	.ui-nav-drawer:not(.ui-nav-drawer--open) { border-right-width: 0; box-shadow: none; }
	.ui-nav-drawer--persistent {
		position: relative;
		left: auto;
		height: 100vh;
		flex-shrink: 0;
		box-shadow: none;
		z-index: auto;
	}
	.ui-nav-drawer-scrim {
		position: fixed;
		top: 0;
		bottom: 0;
		left: 60px;
		right: 0;
		z-index: 3;
		background: rgba(0, 0, 0, 0.32);
	}
	.ui-nav-drawer-inner {
		width: 230px;
		height: 100%;
		padding: 18px 12px;
		display: flex;
		flex-direction: column;
	}
	.ui-nav-drawer-header { padding: 0 4px 12px; margin-bottom: 8px; border-bottom: 1px solid var(--border); }
	.ui-nav-drawer-title {
		font-size: 11px;
		font-weight: 700;
		color: var(--text-primary);
		text-transform: uppercase;
		letter-spacing: 0.06em;
		padding: 4px 10px 10px;
	}
	.ui-nav-drawer-items { flex: 1; min-height: 0; overflow-y: auto; }
	.ui-nav-drawer-state {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 10px;
		font-size: 12px;
		color: var(--text-dim);
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
		min-width: 0;
	}
	.ui-nav-drawer-item:hover { background: var(--bg-hover); color: var(--text-primary); }
	.ui-nav-drawer-item:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
	.ui-nav-drawer-item--active { background: var(--accent-muted); color: var(--accent); font-weight: 600; }
	.ui-nav-drawer-icon {
		width: 16px;
		display: flex;
		justify-content: center;
		opacity: 0.85;
		flex-shrink: 0;
	}
	.ui-nav-drawer-dot { width: 6px; height: 6px; border-radius: 50%; background: currentColor; opacity: 0.6; }
	.ui-nav-drawer-label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
	.ui-nav-drawer-footer { padding-top: 10px; border-top: 1px solid var(--border); }

	@media (max-width: 639px) {
		.ui-nav-drawer--hide-phone { display: none; }
	}
</style>
```

- [ ] **Step 2: Verify**

Run: `cd frontend && npm run check` and `npm run build`.
Manual: `/admin/orgs` — admin's modal drawers are unchanged: open from the rail, close on outside click / Escape / item click, item icons render. Both themes.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/components/ui/NavDrawer.svelte
git commit -m "feat(ui): NavDrawer header/footer slots, loading and empty states"
```

---

### Task 9: Slide panel frame

**Files:**
- Modify: `frontend/src/lib/components/SlidePanel.svelte`

**Interfaces:**
- Consumes: `Button` (`$lib/components/ui`, supports `variant="ghost"`, `size="icon"`, `aria-label`).
- Produces: unchanged `SlidePanel` props (`title`, `onClose`, `zIndex`, `children`); `PanelContainer` and `uiStore` are untouched.

- [ ] **Step 1: Replace the close button**

Add `import { Button } from '$lib/components/ui';` and replace the close `<button class="close-btn btn btn-ghost btn-icon" …>` with:

```svelte
		<Button variant="ghost" size="icon" aria-label="Close panel" onclick={onClose}>
			<X size={16} />
		</Button>
```

Delete the `.close-btn` and `.close-btn:hover` rules. Keep every other rule, including the `@media (max-width: 639px)` full-width rule.

- [ ] **Step 2: Verify**

Run: `cd frontend && npm run check` and `npm run build`.
Manual: open `/admin/storage`, drill into a bucket and click a file → the preview panel opens; the close button closes it; the scrim click closes it. Resize to 400px → the panel is full width. Both themes.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/components/SlidePanel.svelte
git commit -m "refactor(app): use shared Button for slide panel close"
```

---

## Part B — Shell

### Task 10: `AppNav` — main-app rail, footer actions, account menu

**Files:**
- Create: `frontend/src/lib/components/AppNav.svelte`

**Interfaces:**
- Consumes: `NavRail` (Task 7: `groups` with `href`/`active`, `footer`, `phoneBar`), `Dropdown` (Task 4: `menu`, `placement`, `triggerLabel`), `Button`; stores `uiStore`, `authStore`, `orgStore`, `versionStore`, `toastStore`; `api`; `clearAuthCookies`; `can`, `perm`.
- Produces: `<AppNav orgSlug={string} />` — renders the whole rail including footer actions. All logic is moved verbatim from `frontend/src/lib/components/IconSidebar.svelte` (lines 1–112 of the pre-migration file).

- [ ] **Step 1: Create `AppNav.svelte`**

```svelte
<script lang="ts">
	import { Home, FolderOpen, Settings, Package, PanelLeftClose, PanelLeftOpen, LogOut, User, RefreshCw, ExternalLink, Moon, Sun, Command, CreditCard, ShieldAlert } from '@lucide/svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { NavRail, Dropdown, Button } from '$lib/components/ui';
	import { uiStore } from '$lib/stores/ui.store';
	import { authStore } from '$lib/stores/auth.store';
	import { orgStore } from '$lib/stores/org.store';
	import { versionStore } from '$lib/stores/version.store';
	import { toastStore } from '$lib/stores/toast.store';
	import { api } from '$lib/api/client';
	import { clearAuthCookies } from '$lib/auth/cookies';
	import { can, perm } from '$lib/auth/permissions';

	interface Props {
		orgSlug: string;
	}

	let { orgSlug }: Props = $props();

	let collapsed = $derived($uiStore.sidebarCollapsed);

	function isActive(href: string, exact = false): boolean {
		if (exact) return page.url.pathname === href;
		return page.url.pathname === href || page.url.pathname.startsWith(href + '/');
	}

	let groups = $derived([
		{ key: 'home',     icon: Home,       label: 'Home',     href: `/orgs/${orgSlug}`,          active: isActive(`/orgs/${orgSlug}`, true) },
		{ key: 'projects', icon: FolderOpen, label: 'Projects', href: `/orgs/${orgSlug}/projects`, active: isActive(`/orgs/${orgSlug}/projects`) },
		{ key: 'registry', icon: Package,    label: 'Registry', href: `/orgs/${orgSlug}/registry`, active: isActive(`/orgs/${orgSlug}/registry`) },
		{ key: 'settings', icon: Settings,   label: 'Settings', href: `/orgs/${orgSlug}/settings`, active: isActive(`/orgs/${orgSlug}/settings`) }
	]);

	let userEmail    = $derived($authStore.user?.email ?? '');
	let isSuperadmin = $derived($authStore.user?.is_superadmin === true);
	let myRole       = $derived($orgStore.myMembership?.role ?? '');
	let myPerms      = $derived($orgStore.myMembership?.permissions ?? []);
	let orgId        = $derived($orgStore.activeOrg?.id ?? '');
	let initials     = $derived(userEmail ? userEmail.slice(0, 1).toUpperCase() : 'U');
	let canUpdate    = $derived(can(myRole as import('$lib/api/types').MemberRole, myPerms, perm(orgId, 'system', 'update')));
	let hasUpdate    = $derived($versionStore.info?.update_available ?? false);
	let updating     = $derived($versionStore.updating);

	const ROLE_LABELS: Record<string, string> = {
		owner: 'Owner', admin: 'Admin', member: 'Member', viewer: 'Viewer'
	};

	async function logout() {
		// Invalidate the HttpOnly refresh token on the server first.
		await api.logout();
		clearAuthCookies();
		authStore.logout();
		api.setToken(null);
		// Hard navigation so the root layout re-reads cookies from scratch.
		window.location.href = '/login';
	}

	let isDark = $state(false);

	$effect(() => {
		isDark = localStorage.getItem('shipyard_theme') === 'dark';
		document.documentElement.setAttribute('data-theme', isDark ? 'dark' : '');
	});

	function toggleTheme() {
		isDark = !isDark;
		localStorage.setItem('shipyard_theme', isDark ? 'dark' : 'light');
		document.documentElement.setAttribute('data-theme', isDark ? 'dark' : '');
	}

	async function runUpdate() {
		if (updating) return;
		versionStore.setUpdating(true);
		toastStore.add({ type: 'info', title: 'Update started', message: 'Pulling latest images…' });
		const res = await api.triggerUpdate();
		if (res.error) {
			versionStore.setUpdateError(res.error.message);
			toastStore.add({ type: 'error', title: 'Update failed', message: res.error.message });
		} else {
			versionStore.clearUpdate();
			// Invalidate local version info so next check reflects new version.
			versionStore.setInfo(null);
			toastStore.add({ type: 'success', title: 'Update complete', message: res.data?.message ?? 'Restart services to apply.' });
		}
	}
</script>

<NavRail {groups} phoneBar>
	{#snippet footer()}
		<span class="app-nav-hide-phone">
			<Button variant="ghost" size="icon" aria-label="Open command palette" onclick={() => uiStore.openCommandPalette()}>
				<Command size={16} />
			</Button>
		</span>
		<Button variant="ghost" size="icon" aria-label={isDark ? 'Switch to light mode' : 'Switch to dark mode'} onclick={toggleTheme}>
			{#if isDark}<Sun size={16} />{:else}<Moon size={16} />{/if}
		</Button>
		<span class="app-nav-hide-phone">
			<Button variant="ghost" size="icon" aria-label={collapsed ? 'Show panel' : 'Hide panel'} onclick={() => uiStore.toggleSidebar()}>
				{#if collapsed}<PanelLeftOpen size={18} />{:else}<PanelLeftClose size={18} />{/if}
			</Button>
		</span>
		<div class="app-nav-account">
			<Dropdown placement="right-end" triggerLabel="Account">
				{#snippet trigger()}
					<span class="app-nav-avatar">
						{initials}
						{#if hasUpdate}<span class="app-nav-update-dot" aria-label="Update available"></span>{/if}
					</span>
				{/snippet}
				{#snippet menu(close)}
					<div class="ui-dropdown-header app-nav-menu-header">
						<span class="app-nav-avatar app-nav-avatar--lg">{initials}</span>
						<span class="app-nav-menu-info">
							<span class="app-nav-menu-email" title={userEmail}>{userEmail}</span>
							{#if myRole}<span class="app-nav-menu-role">{ROLE_LABELS[myRole] ?? myRole}</span>{/if}
						</span>
					</div>
					<div class="ui-dropdown-sep"></div>
					{#if hasUpdate && canUpdate && $versionStore.info}
						<div class="ui-dropdown-header app-nav-update">
							<span class="app-nav-update-label">Update available</span>
							<span class="app-nav-update-version">v{$versionStore.info.latest}</span>
						</div>
						{#if $versionStore.info.release_url}
							<a
								class="ui-dropdown-item"
								role="menuitem"
								href={$versionStore.info.release_url}
								target="_blank"
								rel="noopener noreferrer"
								onclick={close}
							>
								<ExternalLink size={13} /> Release notes
							</a>
						{/if}
						<button class="ui-dropdown-item" role="menuitem" disabled={updating} onclick={() => { close(); runUpdate(); }}>
							<RefreshCw size={13} /> {updating ? 'Updating…' : 'Update now'}
						</button>
						<div class="ui-dropdown-sep"></div>
					{/if}
					<button class="ui-dropdown-item" role="menuitem" onclick={() => { close(); goto(`/orgs/${orgSlug}/profile`); }}>
						<User size={13} /> Profile settings
					</button>
					<button class="ui-dropdown-item" role="menuitem" onclick={() => { close(); goto(`/orgs/${orgSlug}/billing`); }}>
						<CreditCard size={13} /> Billing & Plan
					</button>
					<div class="ui-dropdown-sep"></div>
					{#if isSuperadmin}
						<a class="ui-dropdown-item" role="menuitem" href="/admin" onclick={close}>
							<ShieldAlert size={13} /> Admin Panel
						</a>
						<div class="ui-dropdown-sep"></div>
					{/if}
					<button class="ui-dropdown-item ui-dropdown-item--danger" role="menuitem" onclick={() => { close(); logout(); }}>
						<LogOut size={13} /> Sign out
					</button>
				{/snippet}
			</Dropdown>
		</div>
	{/snippet}
</NavRail>

<style>
	.app-nav-account { margin-top: 6px; }
	.app-nav-avatar {
		position: relative;
		width: 32px;
		height: 32px;
		border-radius: 50%;
		background: var(--accent-muted);
		color: var(--accent);
		font-size: 12px;
		font-weight: 700;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}
	.app-nav-avatar--lg { width: 34px; height: 34px; }
	.app-nav-update-dot {
		position: absolute;
		top: 0;
		right: 0;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--accent-yellow);
		border: 2px solid var(--bg-surface);
	}
	.app-nav-menu-header { display: flex; align-items: center; gap: 10px; min-width: 220px; }
	.app-nav-menu-info { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
	.app-nav-menu-email { font-size: 12.5px; font-weight: 600; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 170px; }
	.app-nav-menu-role { font-size: 11px; color: var(--text-muted); }
	.app-nav-update { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
	.app-nav-update-label { font-size: 12px; font-weight: 600; color: var(--accent-yellow); }
	.app-nav-update-version { font-size: 11px; font-family: var(--font-mono); color: var(--text-muted); }

	@media (max-width: 639px) {
		.app-nav-hide-phone { display: none; }
		.app-nav-account { margin-top: 0; }
		.app-nav-account :global(.ui-dropdown-menu) {
			left: auto;
			right: 0;
			bottom: calc(100% + 10px);
			top: auto;
		}
	}
</style>
```

- [ ] **Step 2: Verify it compiles**

Run: `cd frontend && npm run check` and `npm run build` (the component is not mounted until Task 11; this confirms types).

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/components/AppNav.svelte
git commit -m "feat(app): add AppNav rail with footer actions and account menu"
```

---

### Task 11: `ProjectDrawer` + org shell rewire

**Files:**
- Create: `frontend/src/lib/components/ProjectDrawer.svelte`
- Modify: `frontend/src/routes/orgs/[orgSlug]/+layout.svelte`
- Delete: `frontend/src/lib/components/IconSidebar.svelte`, `frontend/src/lib/components/ContextPanel.svelte` (only after confirming no other importer)

**Interfaces:**
- Consumes: `NavDrawer` (Task 8: `persistent`, `header`, `footer`, `loading`, `emptyText`, `hideOnPhone`, dot fallback), `Button`, `AppNav` (Task 10), `projectStore`, `orgStore`, `uiStore`.
- Produces: `<ProjectDrawer orgSlug={string} />`; the org layout renders `AppNav` + `ProjectDrawer` + `<main>` + `PanelContainer`.

- [ ] **Step 1: Create `ProjectDrawer.svelte`**

```svelte
<script lang="ts">
	import { Plus } from '@lucide/svelte';
	import { page } from '$app/state';
	import { NavDrawer, Button } from '$lib/components/ui';
	import { projectStore } from '$lib/stores/project.store';
	import { orgStore } from '$lib/stores/org.store';
	import { uiStore } from '$lib/stores/ui.store';

	interface Props {
		orgSlug: string;
	}

	let { orgSlug }: Props = $props();

	let org = $derived($orgStore.activeOrg);
	let open = $derived(!$uiStore.sidebarCollapsed);

	let items = $derived(
		$projectStore.projects.map((p) => ({
			href: `/orgs/${orgSlug}/projects/${p.slug}`,
			label: p.name,
			active: page.url.pathname.includes(`/projects/${p.slug}`)
		}))
	);
</script>

<NavDrawer
	{open}
	persistent
	hideOnPhone
	title="Projects"
	{items}
	loading={$projectStore.isLoading}
	emptyText="No projects yet"
	onNavigate={() => {}}
>
	{#snippet header()}
		<div class="pd-org">
			<span class="pd-org-icon">{org ? org.name.charAt(0).toUpperCase() : 'O'}</span>
			<span class="pd-org-info">
				<span class="pd-org-name" title={org?.name ?? 'Organization'}>{org?.name ?? 'Organization'}</span>
				<span class="pd-org-slug">{org?.slug ?? ''}</span>
			</span>
		</div>
	{/snippet}
	{#snippet footer()}
		<!-- Kept handler-less: the pre-migration button had no click handler (see plan ruling). -->
		<Button variant="secondary" size="sm">
			<Plus size={14} /> New Project
		</Button>
	{/snippet}
</NavDrawer>

<style>
	.pd-org { display: flex; align-items: center; gap: 10px; min-width: 0; }
	.pd-org-icon {
		width: 30px;
		height: 30px;
		border-radius: var(--radius-md);
		background: var(--accent-muted);
		color: var(--accent);
		font-size: 13px;
		font-weight: 700;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}
	.pd-org-info { display: flex; flex-direction: column; min-width: 0; }
	.pd-org-name { font-size: 13px; font-weight: 700; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.pd-org-slug { font-size: 11px; color: var(--text-dim); font-family: var(--font-mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.pd-org-info, .pd-org { max-width: 100%; }
</style>
```

- [ ] **Step 2: Rewire the org layout**

In `frontend/src/routes/orgs/[orgSlug]/+layout.svelte`:
- Replace the `IconSidebar` and `ContextPanel` imports with `import AppNav from '$lib/components/AppNav.svelte';` and `import ProjectDrawer from '$lib/components/ProjectDrawer.svelte';`.
- Delete `let collapsed = $derived($uiStore.sidebarCollapsed);` (keep the `uiStore` import only if still used elsewhere in the script; otherwise remove it).
- Keep the whole `onMount` block unchanged.
- Replace the template and `<style>` with:

```svelte
<div class="app-shell">
	<AppNav {orgSlug} />
	<ProjectDrawer {orgSlug} />
	<main class="main-content">
		{@render children()}
	</main>
	<PanelContainer />
</div>

<style>
	.app-shell {
		display: flex;
		height: 100vh;
		overflow: hidden;
		background: var(--bg-base);
	}
	.main-content {
		flex: 1;
		min-width: 0;
		height: 100vh;
		overflow: hidden;
		display: flex;
		flex-direction: column;
	}
	@media (max-width: 639px) {
		.main-content { padding-bottom: 56px; }
	}
</style>
```

- [ ] **Step 3: Delete the old shell components**

Run: `cd frontend && grep -rln "IconSidebar\|ContextPanel" src` — expected: no matches outside the two files themselves. Then delete `src/lib/components/IconSidebar.svelte` and `src/lib/components/ContextPanel.svelte`. If anything else imports them, stop and migrate that importer to `AppNav`/`ProjectDrawer` first.

- [ ] **Step 4: Verify**

Run: `cd frontend && npm run check` and `npm run build`.
Manual, logged in as the superadmin test user, at `/orgs/<org>/`:
- Rail shows Home/Projects/Registry/Settings; each navigates; the active one highlights; the logo is at the top.
- Footer: command palette opens (also via ⌘K as before), theme toggle switches and persists across reload, collapse toggle shows/hides the project drawer (default: hidden, as before).
- Project drawer (Review Focus #4, #5): org initial + name + slug in the header; a long org name truncates; project list highlights the current project; while loading it shows "Loading…"; for an org with no projects it shows "No projects yet"; the New Project button renders (no action — unchanged).
- Account menu (Review Focus #2, #3): opens from the avatar; keyboard ArrowDown/ArrowUp/Escape work; Profile and Billing navigate; Admin Panel appears only for a superadmin; Sign out is not clicked (it would end the session — verify the item exists only). The update block appears only when an update is available and the member has the update permission.
- Phone width 400px (Review Focus #1): the rail is a bottom bar with the four nav items, theme toggle and avatar; palette and collapse buttons are hidden; the project drawer is hidden; page content is not covered by the bar; the account menu opens upward inside the viewport.
- Both themes.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/components/ProjectDrawer.svelte "frontend/src/routes/orgs/[orgSlug]/+layout.svelte"
git rm frontend/src/lib/components/IconSidebar.svelte frontend/src/lib/components/ContextPanel.svelte
git commit -m "refactor(app): rebuild org shell on NavRail and persistent NavDrawer"
```

---

### Task 12: Root layout, error page, root page

**Files:**
- Modify: `frontend/src/routes/+error.svelte`
- Review (modify only if they use global classes or inline `<svg>`): `frontend/src/routes/+layout.svelte`, `frontend/src/routes/+page.svelte`

**Interfaces:**
- Consumes: `Button` (incl. `href` link mode from Task 2), `@lucide/svelte` icons.

Census (pre-migration): `+error.svelte` — 245 lines, 2 global `btn` usages, 2 inline `<svg>`. `+layout.svelte` and `+page.svelte` — no global classes, no inline `<svg>`.

- [ ] **Step 1: Read all three files completely** and note every flow (links, `goto`, status-based copy).
- [ ] **Step 2: Migrate `+error.svelte`** — replace each inline `<svg>` with the closest `@lucide/svelte` icon (e.g. `AlertTriangle`, `SearchX`, `ArrowLeft`, `Home` — pick by what the path draws); replace `class="btn btn-*"` buttons with `Button` and `<a class="btn btn-*" href>` links with `Button href=…` (Task 2's link mode), keeping the same variant. Keep all status/message logic unchanged.
- [ ] **Step 3: Confirm `+layout.svelte` and `+page.svelte` need no change** (`grep -n 'btn\|badge-\|status-dot\|<svg' src/routes/+layout.svelte src/routes/+page.svelte` → no matches). If any appear, migrate them the same way.
- [ ] **Step 4: Verify** — `npm run check`, `npm run build`; visit a non-existent URL (e.g. `/orgs/<org>/does-not-exist`) → the error page renders with icons and working buttons/links in both themes and at 400px.
- [ ] **Step 5: Commit**

```bash
git add frontend/src/routes/+error.svelte
git commit -m "refactor(app): migrate error page to shared design system"
```

---

## How every migration task below works

Every task from here on migrates existing files. Each task is self-contained: it lists the file's pattern census (from a scan before planning), the exact mapping for the patterns that file uses, the flows that must keep working, and the manual check. The same six steps apply to every task:

1. **Read the whole file** (script, template, style) before editing. List its flows: API calls, links/`goto`, every `uiStore.pushPanel` call and its props, permission checks, confirmation steps, loading/empty/error states.
2. **Migrate the template** using the task's mapping. Script logic stays unchanged unless the task says otherwise; any forced change goes in the task report.
3. **Clean up CSS**: delete rules the components now own; leftover local CSS uses only `layout.css` tokens. Then confirm `grep -nE 'class="([^"]* )?(btn|badge|status-dot|card|input)( [^"]*)?"|<svg' <file>` prints nothing.
4. **Verify**: `cd frontend && npm run check` (no new errors or warnings attributable to the file) and `cd frontend && npm run build` (exit 0).
5. **Manual check** as the task describes, in light and dark theme — never completing a destructive action.
6. **Commit** with the task's exact message.

Standard mapping vocabulary used in the tasks (all components from `$lib/components/ui` unless noted):

- `btn btn-<variant>` → `Button variant="<variant>"` (`btn-sm`/`btn-xs` → `size="sm"`; `btn-icon` → `size="icon"` + `aria-label`); `<a class="btn …" href>` → `Button href=…`; a `title` tooltip → `Button title=…`.
- `badge badge-green|red|yellow|blue|muted` → `Badge tone="green|red|yellow|blue|neutral"`.
- `<span class="status-dot {s}">` → `<StatusDot status={toDotStatus(s)} />` with `import { toDotStatus } from '$lib/utils/status'`; keep the adjacent text label.
- tab row → `Tabs` (button mode: `value={activeTab}` + `onChange={(id) => switchTab(id as Tab)}` — or `bind:value` when there is no loader; Danger tab → `danger: true`).
- `.kv-grid`/`.kv-row` → `KeyValueList` (custom cell content via the `value` snippet).
- `.hero-row` → `Card` containing `ListRow` (icon snippet + title + meta).
- hand-rolled modal/backdrop: a delete/irreversible confirmation → `ConfirmDialog` (type-the-name → `confirmText`); a form dialog → `Modal` with `children`/`footer` snippets.
- `.form-group`/`.form-label`/`.form-input`/`.form-hint` → `FormField label=… hint=…` wrapping `TextField`/`Select`/`Textarea`/`Toggle`/`Checkbox`; keep every native attribute (`type`, `autocomplete`, `name`, `required`, `min`, `max`, `readonly`, handlers) — Task 2 forwards them; icon-prefixed input → `TextField` with an `icon` snippet.
- `.err-msg`/`.error-msg` → `InlineAlert tone="error"`; `.ok-msg`/`.success-msg` → `InlineAlert tone="success"`.
- `<table>` → `DataTable` client mode (`items`, `rowKey`, `columns`, `searchFields`, `row` snippet); `searchable={false}` unless the page already had a search box; expandable rows → a second `<tr>` with one `<td colspan>`; existing "Load more"/cursor buttons stay below the table.
- spinners → `Spinner size={…}`, or button text such as "Saving…".
- `.empty-state` → `EmptyState message=… sub=…` (icon via its `icon` snippet).
- inline `<svg>` → the nearest `@lucide/svelte` icon.
- page title + subtitle → `PageHeader title subtitle` with an `actions` snippet — **only** on standalone pages (never under the settings or registry layouts, which render their own title).

---

## Part C — Entry pages

### Task 13: Migrate `login`

**Files:** Modify `frontend/src/routes/login/+page.svelte`

**Interfaces:** Consumes `Card`, `FormField`, `TextField`, `Button`, `InlineAlert`, `Spinner`.

**Census:** 362 lines · 1 global `btn` · 2 spinners · hand-rolled form fields and error text.

**Mapping:** email/password fields → `FormField` + `TextField`, keeping `type`, `autocomplete`, `name`, `required`, `placeholder` and any handlers exactly; submit → `Button type="submit"` with its loading text; error text → `InlineAlert tone="error"`; the login card → `Card` (keep the page's own centering/background CSS and the brand block).

**Preserve:** the submit handler and post-login redirect, the "Admin login" link to `/admin/login`, the register link, any provider/OAuth buttons, Enter-to-submit.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — in a logged-out browser session: wrong password shows the error alert; correct test credentials land on `/orgs`; the email and password inputs still carry their `autocomplete` attributes (inspect element); Enter submits. Both themes, and at 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/routes/login/+page.svelte && git commit -m "refactor(app): migrate login page to shared design system"`

---

### Task 14: Migrate `register`

**Files:** Modify `frontend/src/routes/register/+page.svelte`

**Interfaces:** Consumes `Card`, `FormField`, `TextField`, `Button`, `InlineAlert`.

**Census:** 188 lines · 1 global `btn` · hand-rolled fields.

**Mapping:** fields → `FormField` + `TextField` (keep `type`/`autocomplete="new-password"`/`required`); submit → `Button type="submit"`; errors → `InlineAlert tone="error"`; card → `Card`.

**Preserve:** registration submit and redirect, validation messages, link back to login.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — logged out: the page renders; submitting empty fields shows the same validation as before; do **not** create a real account (stop before a valid submit, or cancel). Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/routes/register/+page.svelte && git commit -m "refactor(app): migrate register page to shared design system"`

---

### Task 15: Migrate `setup`

**Files:** Modify `frontend/src/routes/setup/+page.svelte`

**Interfaces:** Consumes `Card`, `FormField`, `TextField`, `Button`, `InlineAlert`, `Spinner`, `Badge`.

**Census:** 429 lines · 8 global `btn` · 3 spinners · multi-step first-run setup.

**Mapping:** step container → `Card`; every input → `FormField` + `TextField`/`Select`/`Toggle` with native attributes kept; step navigation buttons → `Button` (primary for continue, secondary/ghost for back); step indicators → keep local markup restyled with tokens, or `Badge` for step labels; errors → `InlineAlert`.

**Preserve:** step order, per-step validation, the final setup submit, the redirect when setup is already complete.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — the dev instance is already set up, so `/setup` should redirect exactly as before; to see the steps, read the template branches and verify each step renders by temporarily forcing the step state in the browser devtools (do not submit setup). Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/routes/setup/+page.svelte && git commit -m "refactor(app): migrate setup page to shared design system"`

---

### Task 16: Migrate `onboarding`

**Files:** Modify `frontend/src/routes/onboarding/+page.svelte`

**Interfaces:** Consumes `Card`, `FormField`, `TextField`, `Button`, `InlineAlert`; `@lucide/svelte` icons.

**Census:** 553 lines · 6 global `btn` · **5 inline `<svg>`** · onboarding steps.

**Mapping:** each inline `<svg>` → nearest lucide icon (check what each path draws — likely check marks, arrows, org/building, rocket); buttons → `Button`; inputs → `FormField` + `TextField`; step cards → `Card`; errors → `InlineAlert`.

**Preserve:** step progression, org creation call, redirect into the new org, skip/back behavior.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — load `/onboarding`; walk each step's UI up to (not including) the final create action; icons render; back/next work. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/routes/onboarding/+page.svelte && git commit -m "refactor(app): migrate onboarding page to shared design system"`

---

### Task 17: Migrate `accept-invite`

**Files:** Modify `frontend/src/routes/accept-invite/[token]/+page.svelte`

**Interfaces:** Consumes `Card`, `FormField`, `TextField`, `Button`, `InlineAlert`, `Spinner`, `Avatar`.

**Census:** 535 lines · 8 global `btn` · 2 form fields · 2 spinners.

**Mapping:** invite card → `Card`; org/inviter identity → `Avatar` + text; fields → `FormField` + `TextField` (keep `autocomplete`/`required`); accept/decline → `Button` (primary / ghost); invalid/expired token state → `InlineAlert tone="error"` or `EmptyState`.

**Preserve:** token validation on load, accept flow for logged-in and new users, decline flow, redirect after accepting.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — visit `/accept-invite/invalid-token`: the invalid/expired state renders; if a real pending invite token exists in the dev DB, open it and stop before accepting. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/accept-invite/[token]/+page.svelte" && git commit -m "refactor(app): migrate accept-invite page to shared design system"`

---

### Task 18: Migrate `unauthorized`

**Files:** Modify `frontend/src/routes/unauthorized/+page.svelte`

**Interfaces:** Consumes `EmptyState` or `Card`, `Button`.

**Census:** 129 lines · 2 global `btn`.

**Mapping:** message block → `Card` (or `EmptyState` with an icon snippet); actions → `Button`/`Button href`.

**Preserve:** the links/actions offered (back to orgs, sign out, etc.).

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — load `/unauthorized`; each action navigates as before (do not click sign out). Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/routes/unauthorized/+page.svelte && git commit -m "refactor(app): migrate unauthorized page to shared design system"`

---

### Task 19: Migrate the org picker (`/orgs`)

**Files:** Modify `frontend/src/routes/orgs/+page.svelte`

**Interfaces:** Consumes `PageHeader`, `Card`, `Avatar`, `Button`, `Modal`, `FormField`, `TextField`, `InlineAlert`, `EmptyState`, `Spinner`; lucide icons.

**Census:** 427 lines · 7 global `btn` · **2 inline `<svg>`** · 3 modal markers (create-org dialog) · 2 spinners · 4 empty-state markers.

**Mapping:** page title → `PageHeader` (standalone page) with the create-org button in `actions`; org cards → `Card` with `Avatar` + name/slug (keep the grid layout CSS); create-org dialog → `Modal` with `FormField`/`TextField` in `children` and Cancel/Create `Button`s in `footer`; empty state → `EmptyState`; inline `<svg>` → lucide.

**Preserve:** org list load, click-through into an org, create-org submit and redirect, validation errors, superadmin admin link if present.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — `/orgs` lists orgs; clicking one opens it; the create dialog opens, validates and cancels (do not create); keyboard: Tab stays inside the dialog and Escape closes it. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/routes/orgs/+page.svelte && git commit -m "refactor(app): migrate org picker to shared design system"`

---

## Part D — Org pages

### Task 20: Migrate the org home dashboard

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/+page.svelte`

**Interfaces:** Consumes `PageHeader`, `StatCard`, `Card`, `ListRow`, `ActivityList`, `Button`, `EmptyState`, `Badge`; lucide icons.

**Census:** 736 lines · 7 global `btn` · 4 empty-state markers.

**Mapping:** page title → `PageHeader`; summary counters → `StatCard` grid; recent-activity / project lists → `ActivityList` + `ListRow`; section cards → `Card`; empty states → `EmptyState`; buttons → `Button`/`Button href`.

**Preserve:** every data load, every link into projects/settings, permission-gated quick actions.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — `/orgs/<org>` renders all sections with real data; an org with no projects shows the empty states; links navigate. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/+page.svelte" && git commit -m "refactor(app): migrate org home to shared design system"`

---

### Task 21: Migrate `profile`

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/profile/+page.svelte`

**Interfaces:** Consumes `PageHeader`, `Card`, `FormField`, `TextField`, `Button`, `InlineAlert`, `Avatar`.

**Census:** 404 lines · 3 global `btn` · 10 form markers · 2 spinners.

**Mapping:** title → `PageHeader`; each form section → `Card`; fields → `FormField` + `TextField` (password fields keep `type="password"` and `autocomplete="current-password"`/`"new-password"`); save buttons → `Button` with loading text; success/error → `InlineAlert`.

**Preserve:** profile update, password change (current/new/confirm validation), any session-related side effects.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — the profile loads the user's data; the password form shows its validation on mismatched input; do not submit a password change. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/profile/+page.svelte" && git commit -m "refactor(app): migrate profile page to shared design system"`

---

### Task 22: Migrate `billing`

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/billing/+page.svelte`

**Interfaces:** Consumes `PageHeader`, `Card`, `Badge`, `StatusDot`, `ProgressBar`, `Button`, `InlineAlert`, `Spinner`, `KeyValueList`; `toDotStatus`.

**Census:** 950 lines · 8 global `btn` · 2 `status-dot` · 4 spinners.

**Mapping:** title → `PageHeader`; current plan / usage cards → `Card`; usage meters → `ProgressBar` (keep the existing percentage math); plan attributes → `KeyValueList`; plan tiers → `Card` grid with `Badge` for the current plan; status dots → `StatusDot` via `toDotStatus`; upgrade/downgrade/manage buttons → `Button`.

**Preserve:** plan and usage loads, plan selection/checkout flow, invoices/payment history links, permission gating of billing actions.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — `/orgs/<org>/billing` shows plan, usage meters and history; plan-change buttons open their flow but nothing is purchased or changed. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/billing/+page.svelte" && git commit -m "refactor(app): migrate billing page to shared design system"`

---

### Task 23: Migrate the projects list

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/projects/+page.svelte`

**Interfaces:** Consumes `PageHeader`, `Card`, `Button`, `Modal`, `FormField`, `TextField`, `Textarea`, `InlineAlert`, `EmptyState`.

**Census:** 428 lines · 4 global `btn` · 1 modal marker (create-project dialog).

**Mapping:** title → `PageHeader` with "New project" in `actions`; project cards → `Card` (keep grid CSS); create dialog → `Modal` (+ `FormField`/`TextField`/`Textarea`, Cancel/Create in `footer`); empty → `EmptyState`.

**Preserve:** project list (shared with the project drawer's store), create-project submit + store update + redirect, permission gating of the create button.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — the list matches the project drawer; the create dialog opens, validates, traps focus, cancels (do not create). Both themes, 400px.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/projects/+page.svelte" && git commit -m "refactor(app): migrate projects list to shared design system"`

---

### Task 24: Migrate the project page chrome (topology canvas stays as-is)

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/projects/[projectSlug]/+page.svelte`

**Interfaces:** Consumes `Button`, `Badge`, `Spinner`, `EmptyState`, `Dropdown` (if the toolbar has menus); `uiStore.pushPanel` unchanged.

**Census:** 865 lines · 5 global `btn` · 3 spinners · **7 `pushPanel` calls**.

**Mapping:** toolbar/header buttons → `Button` (icon buttons `size="icon"` + `aria-label`); status pills → `Badge`; loading overlay → `Spinner`; empty project state → `EmptyState`. **Do not touch** the canvas/topology rendering, node layout, drag/zoom handlers, or the per-node position persistence (`localStorage` keys like `svc_<id>`, `ctr_<id>`).

**Preserve:** all 7 `pushPanel` calls with identical `component`, `title`, `key` and `props` (including every `onCreated`/`onDeleted` callback), the add-resource entry point, canvas interactions.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — open a project: the canvas renders and pans/zooms; clicking a node opens its detail panel; the add-resource button opens `AddResourcePanel`; panels open full width at 400px. Both themes.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/projects/[projectSlug]/+page.svelte" && git commit -m "refactor(app): migrate project page chrome to shared design system"`

---

### Task 25: Migrate project settings

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/projects/[projectSlug]/settings/+page.svelte`

**Interfaces:** Consumes `PageHeader`, `Card`, `FormField`, `TextField`, `Textarea`, `Button`, `InlineAlert`, `ConfirmDialog`.

**Census:** 810 lines · 6 global `btn` · 3 modal markers (delete-project confirmation).

**Mapping:** title → `PageHeader`; settings sections → `Card`; danger zone → `Card tone="danger"`; fields → `FormField` + controls; the delete confirmation → `ConfirmDialog` with `confirmText` set to the same string the old modal required (project name/slug); saves → `Button` with loading text; messages → `InlineAlert`.

**Preserve:** settings save, the type-to-confirm requirement and exact confirm string, delete call + redirect, permission gating.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — settings load and a harmless field shows its save state (revert any change); open the delete dialog: the confirm button stays disabled until the exact name is typed; Cancel. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/projects/[projectSlug]/settings/+page.svelte" && git commit -m "refactor(app): migrate project settings to shared design system"`

---

### Task 26: Migrate the service editor page chrome (code editor stays as-is)

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/projects/[projectSlug]/apps/[serviceId]/editor/+page.svelte`

**Interfaces:** Consumes `Tabs`, `Button`, `Spinner`, `EmptyState`, `Badge`.

**Census:** 833 lines · 9 global `btn` · 2 tab markers · 2 spinners · 3 empty-state markers.

**Mapping:** editor file tabs / mode tabs → `Tabs` (button mode); toolbar buttons → `Button`; loading → `Spinner`; empty states → `EmptyState`. **Do not touch** `CodeEditor`, `FileTree`, `SandboxTerminal` internals or their event wiring.

**Preserve:** open/save file, run/preview actions, terminal session, unsaved-change handling.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — open a sandbox app's editor: file tree loads, a file opens in the editor, tabs switch, the terminal panel still attaches; do not save over real files (open, then close without saving). Both themes.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/projects/[projectSlug]/apps/[serviceId]/editor/+page.svelte" && git commit -m "refactor(app): migrate service editor chrome to shared design system"`

---

## Part E — Registry

### Task 27: Migrate the registry layout

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/registry/+layout.svelte`

**Interfaces:** Consumes `PageHeader`, `Tabs` (link mode), `Button`.

**Census:** 118 lines · 1 global `btn` · link tab bar (Browse / Settings).

**Mapping:** layout title → `PageHeader` (this layout owns the registry title — the child pages must not add another); tab bar → `Tabs` link mode with `tabs=[{id:'browse',label:'Browse',href:…},{id:'settings',label:'Settings',href:…}]` and `value` computed from the existing `isActive` logic (Browse is active for `/registry` and `/registry/<ns>/…` but not `/registry/settings` — keep that exact rule).

**Preserve:** the active-tab rule, any header action button.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — `/orgs/<org>/registry` → Browse active; open a namespace → Browse stays active; `/registry/settings` → Settings active. Both themes, 400px (tabs scroll horizontally if needed).
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/registry/+layout.svelte" && git commit -m "refactor(app): migrate registry layout to shared design system"`

---

### Task 28: Migrate registry browse

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/registry/+page.svelte`

**Interfaces:** Consumes `DataTable`, `Button`, `EmptyState`, `Badge`.

**Census:** 155 lines · 3 global `btn` · 1 `<table>`.

**Mapping:** namespace table → `DataTable` client mode (`searchable={false}` unless the page already has search); row click/link into a namespace kept; buttons → `Button`. No `PageHeader` (the layout owns the title).

**Preserve:** namespace list load and navigation into `[nsId]`.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — the table lists namespaces with pagination; clicking one opens it; an empty registry shows the empty message. Both themes.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/registry/+page.svelte" && git commit -m "refactor(app): migrate registry browse to shared design system"`

---

### Task 29: Migrate the registry namespace page

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/registry/[nsId]/+page.svelte`

**Interfaces:** Consumes `DataTable`, `Button`, `EmptyState`, `Badge`.

**Census:** 170 lines · 1 global `btn` · 1 `<table>`.

**Mapping:** repository table → `DataTable` client mode; back/breadcrumb link → `Button href` (ghost) or keep a local breadcrumb restyled with tokens; kind labels → `Badge`.

**Preserve:** repo list load, navigation into `[repo]`, back navigation.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — a namespace lists its repos; clicking one opens it; back returns to Browse. Both themes.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/registry/[nsId]/+page.svelte" && git commit -m "refactor(app): migrate registry namespace page to shared design system"`

---

### Task 30: Migrate the registry repository page

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/registry/[nsId]/[repo]/+page.svelte`

**Interfaces:** Consumes `DataTable`, `Button`, `ConfirmDialog`, `Badge`, `Spinner`, `KeyValueList`.

**Census:** 361 lines · 4 global `btn` · 1 `<table>` (tags) · 1 modal marker (delete tag) · 2 spinners.

**Mapping:** tag table → `DataTable` client mode; repo summary → `KeyValueList`; delete-tag dialog → `ConfirmDialog` (keep any type-to-confirm string as `confirmText`); copy/pull-command buttons → `Button`.

**Preserve:** tag list, digest/size display, copy pull command, delete-tag flow.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — tags list; copy works; the delete dialog opens and cancels. Both themes.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/registry/[nsId]/[repo]/+page.svelte" && git commit -m "refactor(app): migrate registry repository page to shared design system"`

---

### Task 31: Migrate registry settings

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/registry/settings/+page.svelte`

**Interfaces:** Consumes `Card`, `FormField`, `TextField`, `Select`, `Toggle`, `Button`, `Badge`, `DataTable`, `InlineAlert`.

**Census:** 464 lines · 4 global `btn` · 3 global `badge` · 1 `<table>`.

**Mapping:** settings sections → `Card`; fields → `FormField` + controls; badges → `Badge`; the table (credentials/retention/tokens — check the file) → `DataTable` client mode; saves → `Button`; messages → `InlineAlert`.

**Preserve:** settings load/save, any token create/revoke flows (revoke via `ConfirmDialog` if it was confirmed before).

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — settings load; a save shows its state (revert); create/revoke dialogs open and cancel. Both themes.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/registry/settings/+page.svelte" && git commit -m "refactor(app): migrate registry settings to shared design system"`

---

## Part F — Settings

### Task 32: Migrate the settings layout

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/settings/+layout.svelte`

**Interfaces:** Consumes `PageHeader`, `Tabs` (link mode with icons and badges), `EmptyState` or `InlineAlert` for the access-denied state.

**Census:** 178 lines · 1 global `btn` · 2 global `badge` · link tab bar of 7 tabs with icons and an "Admin" badge.

**Mapping:** "Settings" title + subtitle → `PageHeader` (this layout owns the title for every settings page); tab bar → `Tabs` link mode — map each existing tab to `{ id, label, href: tab.href(orgSlug), icon: tab.icon, badge: tab.badge === 'admin' ? 'Admin' : undefined }`, `value` = the id of the tab whose `isActive(href)` is true.

**Preserve:** the access check (admin/owner or any settings permission) and its denied state, the exact tab list and order, the Admin badges.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** (Review Focus #3) — as the superadmin: all 7 tabs render with icons, Admin badges on Providers/Cloudflare/Members/API Keys, the active tab follows the route. If a non-admin member account exists in the dev DB, confirm the same access behavior as before (denied state or limited tabs). Both themes, 400px (tabs scroll).
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/settings/+layout.svelte" && git commit -m "refactor(app): migrate settings layout to shared design system"`

---

### Task 33: Migrate settings › general

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/settings/general/+page.svelte`

**Interfaces:** Consumes `Card`, `FormField`, `TextField`, `Textarea`, `Button`, `InlineAlert`, `ConfirmDialog`; lucide icons.

**Census:** 646 lines · 6 global `btn` · **1 inline `<svg>`** · 4 spinners.

**Mapping:** sections → `Card`; danger zone (if present) → `Card tone="danger"` + `ConfirmDialog` (keep the confirm string); fields → `FormField` + controls; saves → `Button`; the inline `<svg>` → lucide; messages → `InlineAlert`. No `PageHeader`.

**Preserve:** org name/slug update, avatar/logo upload if present, delete/transfer org flows and their confirmations.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — values load; edit and revert a field; danger dialogs open and cancel. Both themes.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/settings/general/+page.svelte" && git commit -m "refactor(app): migrate general settings to shared design system"`

---

### Task 34: Migrate settings › providers

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/settings/providers/+page.svelte`

**Interfaces:** Consumes `Card`, `ListRow`, `ActivityList`, `Button`, `Modal`, `ConfirmDialog`, `FormField`, `TextField`, `EmptyState`, `Badge`; lucide icons.

**Census:** 470 lines · 6 global `btn` · **3 inline `<svg>`** (likely provider logos — use lucide `Github`/`Gitlab`/`GitBranch` where they exist; if a brand mark has no lucide equivalent, use a generic lucide icon and note it in the report) · 2 modal markers · 4 spinners · 2 empty-state markers.

**Mapping:** connected providers → `ActivityList` + `ListRow`; connect/disconnect → `Button`; the connect dialog → `Modal`; the disconnect confirmation → `ConfirmDialog`; empty → `EmptyState`.

**Preserve:** OAuth connect redirects, disconnect flow, provider status display.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — providers list; the connect dialog opens and cancels (no OAuth redirect completed); disconnect opens and cancels. Both themes.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/settings/providers/+page.svelte" && git commit -m "refactor(app): migrate providers settings to shared design system"`

---

### Task 35: Migrate settings › cloudflare

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/settings/cloudflare/+page.svelte`

**Interfaces:** Consumes `Card`, `FormField`, `TextField`, `Button`, `EmptyState`, `InlineAlert`, `ListRow`.

**Census:** 282 lines · 4 global `btn` · 2 spinners · 2 empty-state markers.

**Mapping:** connections → `Card`/`ListRow`; token fields → `FormField` + `TextField type="password"`; buttons → `Button`; empty → `EmptyState`.

**Preserve:** connection create/test/remove flows.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — the page loads; forms validate; remove opens and cancels. Both themes.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/settings/cloudflare/+page.svelte" && git commit -m "refactor(app): migrate cloudflare settings to shared design system"`

---

### Task 36: Migrate settings › members

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/settings/members/+page.svelte`

**Interfaces:** Consumes `DataTable` or `ActivityList`/`ListRow`, `Avatar`, `Badge`, `Button`, `Select`, `Spinner`, `ConfirmDialog`; `uiStore.pushPanel` (InvitePanel / MemberManagePanel) unchanged.

**Census:** 738 lines · 6 global `btn` · 3 spinners.

**Mapping:** member list → `DataTable` client mode (`searchFields: ['email']` if the page had search, else `searchable={false}`) with `Avatar` + email cell and role `Badge`; invite/manage buttons → `Button` (opening the same panels); remove-member → `ConfirmDialog` if it was confirmed before.

**Preserve:** invite flow (panel), manage-member flow (panel) and their callbacks, role changes, permission gating of every action (Review Focus #3).

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — members list; Invite opens `InvitePanel`; Manage opens `MemberManagePanel`; actions hidden for members without permission exactly as before. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/settings/members/+page.svelte" && git commit -m "refactor(app): migrate members settings to shared design system"`

---

### Task 37: Migrate settings › API keys

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/settings/api-keys/+page.svelte`

**Interfaces:** Consumes `DataTable`, `Button`, `Modal`, `ConfirmDialog`, `FormField`, `TextField`, `Select`, `Checkbox`, `Badge`, `EmptyState`, `InlineAlert`.

**Census:** 740 lines · **15 global `btn`** · 1 `<table>` · 2 modal markers · 3 empty-state markers.

**Mapping:** keys table → `DataTable` client mode; create-key dialog → `Modal` (name, scopes via `Checkbox`, expiry via `Select`); the one-time key reveal → `InlineAlert tone="warning"` + a copy `Button` (keep the "shown only once" copy); revoke → `ConfirmDialog`; status/scope pills → `Badge`.

**Preserve:** create (with the one-time secret display), copy, revoke, permission gating (Review Focus #3).

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — keys list; the create dialog opens, validates, cancels (do not create a key); revoke opens and cancels. Both themes.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/settings/api-keys/+page.svelte" && git commit -m "refactor(app): migrate API keys settings to shared design system"`

---

### Task 38: Migrate settings › deployments

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/settings/deployments/+page.svelte`

**Interfaces:** Consumes `DataTable`, `StatusDot`, `Badge`, `Button`, `Select`, `EmptyState`; `toDotStatus`.

**Census:** 577 lines · 4 global `btn` · **14 `status-dot`** · 1 `<table>` · 3 empty-state markers.

**Mapping:** deployments table → `DataTable` (client mode unless the endpoint returns `total`; check the API call); every status dot → `StatusDot status={toDotStatus(d.status)}` + text; filters → `Select`; row link into `[deploymentId]` kept.

**Preserve:** filters, pagination behavior, navigation into a deployment.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — deployments list with correct dot colors (running green, queued/pending pulsing yellow, failed red, need_attention static yellow); filters work; a row opens the detail page. Both themes.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/settings/deployments/+page.svelte" && git commit -m "refactor(app): migrate deployments settings to shared design system"`

---

### Task 39: Migrate settings › deployment detail

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/settings/deployments/[deploymentId]/+page.svelte`

**Interfaces:** Consumes `KeyValueList`, `StatusDot`, `Button`, `Card`; `toDotStatus`.

**Census:** 164 lines · 1 global `btn`.

**Mapping:** deployment attributes → `KeyValueList` (status via its `value` snippet with `StatusDot`); back link → `Button href` (ghost); sections → `Card`.

**Preserve:** data load, log viewing/link, back navigation.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — open a deployment: attributes render; back returns to the list. Both themes.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/settings/deployments/[deploymentId]/+page.svelte" && git commit -m "refactor(app): migrate deployment detail to shared design system"`

---

### Task 40: Migrate settings › audit

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/settings/audit/+page.svelte`

**Interfaces:** Consumes `DataTable`, `InlineAlert`, `Button`, `Spinner`.

**Census:** 266 lines · 3 global `btn` · 2 alert markers · 1 `<table>` · 2 spinners.

**Mapping:** audit table → `DataTable` (client mode over loaded rows; if the page uses cursor "Load more", keep that button below the table); errors → `InlineAlert`; expandable metadata (if present) → second `<tr>` with `<td colspan>`.

**Preserve:** filters, load-more/cursor behavior, metadata expansion.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — entries list; load more (if present) appends; expanding a row does not change the table width. Both themes.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/settings/audit/+page.svelte" && git commit -m "refactor(app): migrate audit settings to shared design system"`

---

### Task 41: Migrate settings › database

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/settings/database/+page.svelte`

**Interfaces:** Consumes `DataTable`, `Button`, `Modal`, `ConfirmDialog`, `FormField`, `TextField`, `Select`, `EmptyState`, `KeyValueList`, `Badge`.

**Census:** 871 lines · **18 global `btn`** · 1 `<table>` · **7 modal markers** · 2 empty-state markers.

**Mapping:** database list → `DataTable` client mode; connection details → `KeyValueList` (secrets stay masked exactly as today); create/edit dialogs → `Modal`; delete/reset/drop confirmations → `ConfirmDialog` (keep confirm strings); opening the DB client keeps calling the same `DbClientModal` entry point.

**Preserve:** every create/edit/delete/reset flow and its confirmation, credential reveal/copy behavior, the DB client launch.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — the list loads; each dialog opens and cancels; nothing is created, reset or dropped. Both themes.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/settings/database/+page.svelte" && git commit -m "refactor(app): migrate database settings to shared design system"`

---

### Task 42: Migrate settings › docker

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/settings/docker/+page.svelte`

**Interfaces:** Consumes `Tabs`, `DataTable`, `StatusDot`, `Badge`, `Button`, `Spinner`, `ConfirmDialog`; `toDotStatus`.

**Census:** 994 lines · 7 global `btn` · 5 tab markers · **5 `<table>`s** · 6 spinners.

**Mapping:** resource tabs → `Tabs` (button mode, `bind:value` or `value`+`onChange` if tab switches trigger loads); each of the 5 tables → its own `DataTable` client mode (expandable details as second `<tr>`); state dots → `StatusDot`; prune/remove → `ConfirmDialog` where confirmed before.

**Preserve:** per-tab loading, refresh, any prune/remove flows and their confirmations.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — every tab loads its table; expanding a row keeps the table width; prune/remove dialogs open and cancel. Both themes.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/settings/docker/+page.svelte" && git commit -m "refactor(app): migrate docker settings to shared design system"`

---

### Task 43: Migrate settings › infra (part 1: metrics, disk, network)

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/settings/infra/+page.svelte` (template lines ~321–617 and their CSS)

**Interfaces:** Consumes `StatCard`, `ProgressBar`, `Card`, `SectionLabel`, `Spinner`, `EmptyState`.

**Census (whole file):** 1,370 lines (template ~321–782, style 783–end) · 3 global `btn` · 2 `status-dot` · 2 `<table>`s · 4 spinners · 2 empty-state markers. Sections: Core Services (≈443), Disk (≈552), Network (≈588), Swarm Nodes (≈618), Join Tokens (≈689).

**Mapping (this task):** top metric cards → `StatCard` + `ProgressBar` (follow the admin `infra` page's composition: a wrapper around `StatCard` with the bar beneath); section headers (`.section-head` + hint) → `SectionLabel` + hint text; the Core Services, Disk and Network sections → `Card`s with their existing inner layout restyled with tokens. Leave Swarm Nodes and Join Tokens for Task 44.

**Preserve:** the live metrics stream/polling, threshold colors on bars, cumulative network counters.

- [ ] **Step 1–4:** as in "How every migration task works" (step 3's grep applies only to the sections migrated here).
- [ ] **Step 5: Manual check** — metrics update live over ~10 seconds; disk and network sections render. Both themes.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/settings/infra/+page.svelte" && git commit -m "refactor(app): migrate infra settings metrics to shared design system"`

---

### Task 44: Migrate settings › infra (part 2: swarm nodes, join tokens, CSS cleanup)

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/settings/infra/+page.svelte` (template lines ~618–782 and the remaining CSS)

**Interfaces:** Consumes `DataTable`, `StatusDot`, `Badge`, `Button`, `Card`; `toDotStatus`.

**Census:** see Task 43.

**Mapping (this task):** Swarm Nodes table → `DataTable` client mode with `StatusDot` state + role `Badge`; Join Tokens → `Card` + reveal/copy `Button`s (masking logic unchanged); then delete every CSS rule left unused by Tasks 43–44 and run step 3's grep over the whole file.

**Preserve:** token masking/reveal/copy, node state display.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — nodes list with correct dots; Show reveals a token, Copy shows "Copied!". Both themes.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/settings/infra/+page.svelte" && git commit -m "refactor(app): migrate infra settings nodes and tokens to shared design system"`

---

### Task 45: Migrate settings › mqtt

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/settings/mqtt/+page.svelte`

**Interfaces:** Consumes `DataTable`, `Button`, `Spinner`, `EmptyState`, `Badge`, `Card`.

**Census:** 503 lines · 1 global `btn` · **3 `<table>`s** · 4 spinners · **7 empty-state markers**.

**Mapping:** each table → `DataTable` client mode (`emptyMessage` replaces the per-table empty markup); refresh → `Button`; QoS/state pills → `Badge`.

**Preserve:** the three data loads and refresh behavior.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — all three tables render (the dev MQTT broker may be unreachable — then confirm each table's empty/error state renders instead of a blank area, Review Focus #5). Both themes.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/settings/mqtt/+page.svelte" && git commit -m "refactor(app): migrate mqtt settings to shared design system"`

---

### Task 46: Migrate settings › smtp

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/settings/smtp/+page.svelte`

**Interfaces:** Consumes `Card`, `FormField`, `TextField`, `Select`, `Toggle`, `Button`, `InlineAlert`.

**Census:** 344 lines · 6 global `btn` · 4 spinners.

**Mapping:** follow the already-migrated `frontend/src/routes/admin/smtp/+page.svelte` layout: `Card` sections, 2-column `FormField` grid, `Toggle` for enable, `Button` save/test. Port fields keep `type="number"` and any helper that reads the port must accept `string | number`.

**Preserve:** load, save, send-test-email flows.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — values load; edit the port and save, then restore it (confirm no crash or stuck "Saving…"); do not send a test email to a real address. Both themes.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/settings/smtp/+page.svelte" && git commit -m "refactor(app): migrate smtp settings to shared design system"`

---

### Task 47: Migrate settings › static

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/settings/static/+page.svelte`

**Interfaces:** Consumes `Card`, `StatusDot`, `ListRow`, `ActivityList`, `Button`, `EmptyState`, `Spinner`; `toDotStatus`.

**Census:** 475 lines · 4 global `btn` · 3 `status-dot` · 3 spinners · 4 empty-state markers.

**Mapping:** site list → `ActivityList` + `ListRow` (or `DataTable` if it is tabular); status dots → `StatusDot`; buttons → `Button`; empty → `EmptyState`.

**Preserve:** list load and any per-site actions.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — sites list or empty state renders; actions open and cancel. Both themes.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/settings/static/+page.svelte" && git commit -m "refactor(app): migrate static settings to shared design system"`

---

### Task 48: Migrate settings › traefik

**Files:** Modify `frontend/src/routes/orgs/[orgSlug]/settings/traefik/+page.svelte`

**Interfaces:** Consumes `Card`, `FormField`, `TextField`, `Button`, `StatusDot`, `InlineAlert`, `Spinner`; `toDotStatus`.

**Census:** 883 lines · 8 global `btn` · 3 `status-dot` · 4 spinners.

**Mapping:** settings sections → `Card`; fields → `FormField` + `TextField`; config previews → `Card padding="0"` + `<pre>` + copy `Button` (same pattern as admin `traefik/settings`); status dots → `StatusDot`; saves → `Button`; messages → `InlineAlert`.

**Preserve:** load/save, generated config preview, copy.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — values load; the preview updates as fields change; copy works; revert any edited field. Both themes.
- [ ] **Step 6: Commit** — `git add "frontend/src/routes/orgs/[orgSlug]/settings/traefik/+page.svelte" && git commit -m "refactor(app): migrate traefik settings to shared design system"`

---

## Part G — Slide panels

Panels open through `uiStore.pushPanel` and render inside `SlidePanel` (480px; full width at ≤639px). For every panel task: the panel's own `pushPanel` calls (sub-panels it opens) keep the identical `component`, `title`, `key` and `props`, including every `onCreated`/`onDeleted`/`onSelect` callback; sub-panels still end with `uiStore.popPanel()` (never `clearPanels()`); in-panel delete modals become `ConfirmDialog` with the same type-to-confirm string as `confirmText`. Manual checks open the panel the way a user does (from the project canvas, a settings page, or a parent panel) and check it at 400px as well.

### Task 49: Migrate `ServiceDetailPanel` (part 1: header, tabs, overview, replicas)

**Files:** Modify `frontend/src/lib/panels/ServiceDetailPanel.svelte` (template from line ~1045; the overview and replicas branches)

**Interfaces:** Consumes `Tabs`, `Card`, `ListRow`, `KeyValueList`, `StatusDot`, `Badge`, `Button`, `Spinner`, `EmptyState`, `DataTable`; `toDotStatus`.

**Census (whole file):** 4,051 lines (script 1–1044, template 1045–2236, style 2237–4051) · **47 global `btn`** · **15 global `badge`** · 4 `status-dot` · 6 tab markers · 2 modal markers · **19 spinners** · 4 empty-state markers · 3 `pushPanel` calls. Tabs: `overview`, `replicas`, `logs`, `git`, `deploy`, `volumes`, `domains`, `settings` (switching runs loaders in `switchTab`).

**Mapping (this task):** the panel header/hero → `Card` + `ListRow`; the tab row → `Tabs` with `value={activeTab}` and `onChange={(id) => switchTab(id as Tab)}` (so every tab's loader still runs); **overview**: connection info / attributes → `KeyValueList` (secret values keep their existing mask/reveal), status → `StatusDot` via `toDotStatus`, action buttons → `Button`; **replicas**: container/replica list → `DataTable` client mode or `ActivityList`/`ListRow` (whichever matches the current tabular vs. list layout), node placement → `Badge`, per-replica actions → `Button`. Leave the other tabs' markup for Tasks 50–51 (they keep working with the old classes until then).

**Preserve:** `switchTab` loaders, connection-info loading, replica scaling/restart actions and their confirmations, the 3 `pushPanel` calls.

- [ ] **Step 1–4:** as in "How every migration task works" (step 3's grep covers only the header/tabs/overview/replicas markup in this task).
- [ ] **Step 5: Manual check** — open a service from the project canvas: header, tabs (keyboard ArrowLeft/Right too), overview attributes and status, replicas list; actions open their confirmations and cancel. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/ServiceDetailPanel.svelte && git commit -m "refactor(app): migrate service panel overview and replicas to shared design system"`

---

### Task 50: Migrate `ServiceDetailPanel` (part 2: logs, git, deploy)

**Files:** Modify `frontend/src/lib/panels/ServiceDetailPanel.svelte` (the `logs`, `git`, `deploy` branches)

**Interfaces:** Consumes `Card`, `KeyValueList`, `StatusDot`, `Badge`, `Button`, `FormField`, `TextField`, `Select`, `Toggle`, `InlineAlert`, `Spinner`, `EmptyState`, `ActivityList`, `ListRow`; `toDotStatus`.

**Census:** see Task 49.

**Mapping (this task):** **logs**: toolbar buttons → `Button`; the log body itself stays (it is the live log stream — chrome only); webhook token display → `KeyValueList` with copy `Button`; **git**: repo/branch settings → `FormField` + controls (git-provider pickers keep opening the same sub-panels), webhook section → `Card`; **deploy**: deployment history → `ActivityList`/`ListRow` with `StatusDot`, step list → `ListRow`s, deploy/redeploy buttons → `Button`.

**Preserve:** log streaming and filters, webhook token load/copy/regenerate, git config save, deploy trigger + steps loading for the latest deployment.

- [ ] **Step 1–4:** as in "How every migration task works" (grep covers the three tabs).
- [ ] **Step 5: Manual check** — logs stream; git settings load; deploy history and steps render; do not trigger a deploy or regenerate a webhook token. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/ServiceDetailPanel.svelte && git commit -m "refactor(app): migrate service panel logs, git and deploy to shared design system"`

---

### Task 51: Migrate `ServiceDetailPanel` (part 3: volumes, domains, settings, CSS cleanup)

**Files:** Modify `frontend/src/lib/panels/ServiceDetailPanel.svelte` (the `volumes`, `domains`, `settings` branches and the whole style block)

**Interfaces:** Consumes `Card`, `ActivityList`, `ListRow`, `Button`, `Badge`, `FormField`, `TextField`, `Select`, `Toggle`, `InlineAlert`, `ConfirmDialog`, `EmptyState`.

**Census:** see Task 49.

**Mapping (this task):** **volumes**: volume list → `ActivityList`/`ListRow` (add/attach keeps opening the same sub-panels via `VolumeMountList`/pickers); **domains**: domain list → `ActivityList`/`ListRow` with status `Badge`, add-domain keeps pushing `DomainAddPanel`; **settings**: fields → `FormField` + controls (registry URL/user/password keep their exact reset logic in `switchTab`), danger area → `Card tone="danger"`, delete-service modal → `ConfirmDialog` with the same `confirmText`. Then delete every now-unused rule in the 1,800-line style block and run step 3's grep over the whole file.

**Preserve:** volume add/remove, domain add/remove/verify, settings save, delete-service flow and redirect/callback.

- [ ] **Step 1–4:** as in "How every migration task works" (grep covers the whole file now).
- [ ] **Step 5: Manual check** — volumes and domains list; add buttons push their sub-panels; settings load and save a harmless field (revert); delete dialog requires the exact name and cancels. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/ServiceDetailPanel.svelte && git commit -m "refactor(app): migrate service panel volumes, domains and settings to shared design system"`

---

### Task 52: Migrate `StaticSiteDetailPanel` (part 1: header, tabs, overview, config, docs)

**Files:** Modify `frontend/src/lib/panels/StaticSiteDetailPanel.svelte` (template from line ~519; overview ≈642, config ≈851, docs ≈975)

**Interfaces:** Consumes `Tabs`, `Card`, `ListRow`, `KeyValueList`, `FormField`, `TextField`, `Select`, `Toggle`, `Textarea`, `Button`, `Badge`, `InlineAlert`, `Spinner`.

**Census (whole file):** 1,945 lines (script 1–518, template 519–1053, style 1054–end) · 15 global `btn` · 3 tab markers · **2 `hero-row`** · 4 modal markers · **26 form markers** · 2 alert markers · 10 spinners · 3 empty-state markers · 2 `pushPanel` calls. Tabs: `overview`, `domains`, `deployments`, `git`, `config`, `docs`.

**Mapping (this task):** hero rows → `Card` + `ListRow`; tab row → `Tabs` (`value`+`onChange` if switching loads data, else `bind:value`); **overview** attributes → `KeyValueList`; **config** form → `FormField` + controls; **docs** → `Card` + `<pre>` blocks with copy `Button`; alerts → `InlineAlert`.

**Preserve:** config save, build/output settings, docs copy actions.

- [ ] **Step 1–4:** as in "How every migration task works" (grep covers the migrated branches).
- [ ] **Step 5: Manual check** — open a static site from the canvas: overview, config (edit + revert), docs (copy) all work. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/StaticSiteDetailPanel.svelte && git commit -m "refactor(app): migrate static site panel overview, config and docs to shared design system"`

---

### Task 53: Migrate `StaticSiteDetailPanel` (part 2: domains, deployments, git, dialogs, CSS cleanup)

**Files:** Modify `frontend/src/lib/panels/StaticSiteDetailPanel.svelte`

**Interfaces:** Consumes `ActivityList`, `ListRow`, `StatusDot`, `Badge`, `Button`, `ConfirmDialog`, `Modal`, `EmptyState`, `FormField`, `TextField`; `toDotStatus`.

**Census:** see Task 52.

**Mapping (this task):** **domains** → `ActivityList`/`ListRow` (add pushes the same domain sub-panel); **deployments** → `ActivityList`/`ListRow` with `StatusDot`, redeploy/rollback → `Button` + `ConfirmDialog` if confirmed before; **git** → `Card` + `FormField`s; the 4 modal markers → `ConfirmDialog` (destructive) or `Modal` (forms); then delete unused CSS and grep the whole file.

**Preserve:** domain add/remove, deployment history and rollback/redeploy, git settings, delete-site flow and its callback.

- [ ] **Step 1–4:** as in "How every migration task works" (grep covers the whole file).
- [ ] **Step 5: Manual check** — domains/deployments/git tabs render; dialogs open and cancel. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/StaticSiteDetailPanel.svelte && git commit -m "refactor(app): migrate static site panel domains, deployments and git to shared design system"`

---

### Task 54: Migrate `EdgeFunctionDetailPanel` (part 1: header, tabs, overview, functions)

**Files:** Modify `frontend/src/lib/panels/EdgeFunctionDetailPanel.svelte` (template from line ~496; overview ≈624)

**Interfaces:** Consumes `Tabs`, `Card`, `ListRow`, `KeyValueList`, `ActivityList`, `Button`, `Badge`, `Spinner`, `EmptyState`.

**Census (whole file):** 1,539 lines (script 1–495, template 496–1018, style 1019–end) · 18 global `btn` · 4 tab markers · 4 modal markers · 11 spinners · 3 empty-state markers · 3 danger markers · 1 `pushPanel`. `type Tab = 'overview' | 'functions' | 'git' | 'domains' | 'danger'`.

**Mapping (this task):** header → `Card` + `ListRow`; tab row → `Tabs` with the `danger` tab flagged `danger: true` (replacing `class:tab-danger`); **overview** → `KeyValueList`; **functions** list → `ActivityList`/`ListRow` with per-function `Button`s.

**Preserve:** function list loading and per-function actions.

- [ ] **Step 1–4:** as in "How every migration task works" (grep covers the migrated branches).
- [ ] **Step 5: Manual check** — open an edge function from the canvas: tabs (Danger turns red when active), overview, functions. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/EdgeFunctionDetailPanel.svelte && git commit -m "refactor(app): migrate edge function panel overview and functions to shared design system"`

---

### Task 55: Migrate `EdgeFunctionDetailPanel` (part 2: git, domains, danger, CSS cleanup)

**Files:** Modify `frontend/src/lib/panels/EdgeFunctionDetailPanel.svelte`

**Interfaces:** Consumes `Card`, `FormField`, `TextField`, `ActivityList`, `ListRow`, `Badge`, `Button`, `ConfirmDialog`, `InlineAlert`.

**Census:** see Task 54.

**Mapping (this task):** **git** → `Card` + `FormField`s; **domains** → `ActivityList`/`ListRow` (add pushes `EdgeFnDomainAddPanel` unchanged); **danger** → `Card tone="danger"` and the delete modal (`deleteValid`/`isDeleting` logic) → `ConfirmDialog` with the same `confirmText`; then delete unused CSS and grep the whole file.

**Preserve:** git settings, domain add/remove, delete flow and callback.

- [ ] **Step 1–4:** as in "How every migration task works" (grep covers the whole file).
- [ ] **Step 5: Manual check** — git/domains render; the delete dialog requires the exact name and cancels. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/EdgeFunctionDetailPanel.svelte && git commit -m "refactor(app): migrate edge function panel git, domains and danger to shared design system"`

---

### Task 56: Migrate `DeploymentLogsPanel`

**Files:** Modify `frontend/src/lib/panels/DeploymentLogsPanel.svelte`

**Interfaces:** Consumes `StatusDot`, `ListRow`, `Spinner`, `Button`; `toDotStatus`.

**Census:** 448 lines · **7 `status-dot`** · 2 spinners.

**Mapping:** step/status dots → `StatusDot` via `toDotStatus`; step rows → `ListRow` or local rows restyled with tokens; the log body stays (chrome only); spinners → `Spinner`.

**Preserve:** live log streaming, step expansion, auto-scroll.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — open a deployment's logs (from the service deploy tab): steps with correct dots, logs stream. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/DeploymentLogsPanel.svelte && git commit -m "refactor(app): migrate deployment logs panel to shared design system"`

---

### Task 57: Migrate `EnvManagerPanel`

**Files:** Modify `frontend/src/lib/panels/EnvManagerPanel.svelte`

**Interfaces:** Consumes `TextField`, `Textarea`, `Button`, `Toggle`, `InlineAlert`, `Spinner`, `ConfirmDialog`.

**Census:** 780 lines · 10 global `btn` · 4 spinners.

**Mapping:** env rows → local grid restyled with tokens using `TextField` (key in `--font-mono`) and a masked value `TextField type="password"` with a reveal `Button size="icon"`; add/remove/bulk-edit → `Button`; bulk text mode → `Textarea`; save → `Button` with loading text; delete-all/remove confirmations → `ConfirmDialog` if confirmed before.

**Preserve:** add/edit/remove variables, bulk paste parsing, reveal toggles, save, any secret-masking behavior.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — open env vars for a service: list, reveal, add a row and remove it without saving. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/EnvManagerPanel.svelte && git commit -m "refactor(app): migrate env manager panel to shared design system"`

---

### Task 58: Migrate `ExecPanel`

**Files:** Modify `frontend/src/lib/panels/ExecPanel.svelte`

**Interfaces:** Consumes `Button`, `Select`, `Badge`; terminal core untouched.

**Census:** 491 lines · 4 global `btn`.

**Mapping:** container/replica selector → `Select`; connect/disconnect/clear → `Button`; connection state → `Badge`. The terminal element and its socket wiring stay exactly as they are.

**Preserve:** container selection, exec session connect/disconnect, terminal resize.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — open Exec on a running service: select a container, connect, the terminal works, disconnect. Both themes.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/ExecPanel.svelte && git commit -m "refactor(app): migrate exec panel to shared design system"`

---

### Task 59: Migrate `InvitePanel`

**Files:** Modify `frontend/src/lib/panels/InvitePanel.svelte`

**Interfaces:** Consumes `FormField`, `TextField`, `Select`, `Checkbox`, `Button`, `InlineAlert`, `Autocomplete` (only if the existing email field already suggests users — otherwise plain `TextField`).

**Census:** 573 lines · 5 global `btn`.

**Mapping:** email(s) → `FormField` + `TextField type="email"`; role → `Select`; permission checkboxes → `Checkbox`; send → `Button` with loading text; result/errors → `InlineAlert`.

**Preserve:** invite submit, role/permission selection, the `onCreated`-style callback and `uiStore.popPanel()` on success.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — open Invite from settings › members: fields validate; do not send an invite. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/InvitePanel.svelte && git commit -m "refactor(app): migrate invite panel to shared design system"`

---

### Task 60: Migrate `MemberManagePanel`

**Files:** Modify `frontend/src/lib/panels/MemberManagePanel.svelte`

**Interfaces:** Consumes `Avatar`, `Select`, `Checkbox`, `Button`, `ConfirmDialog`, `Spinner`, `Card`.

**Census:** 527 lines · 3 global `btn` · 2 spinners.

**Mapping:** member identity → `Avatar` + email; role → `Select`; permissions → `Checkbox` groups; save → `Button`; remove member → `ConfirmDialog`; danger area → `Card tone="danger"`.

**Preserve:** role/permission save, remove-member flow and its callback, owner/self restrictions.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — open Manage for a member: role and permissions load; the remove dialog opens and cancels; restricted controls stay disabled for the owner/self as before. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/MemberManagePanel.svelte && git commit -m "refactor(app): migrate member manage panel to shared design system"`

---

### Task 61: Migrate `NetworkDetailPanel`

**Files:** Modify `frontend/src/lib/panels/NetworkDetailPanel.svelte`

**Interfaces:** Consumes `KeyValueList`, `ActivityList`, `ListRow`, `Button`, `Spinner`, `ConfirmDialog`.

**Census:** 362 lines · 2 global `btn` · 2 spinners.

**Mapping:** attributes → `KeyValueList`; attached services → `ActivityList`/`ListRow`; actions → `Button`; delete → `ConfirmDialog` if confirmed before.

**Preserve:** attach/detach and delete flows with callbacks.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — open a network from the canvas; attributes and attached services render; dialogs cancel. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/NetworkDetailPanel.svelte && git commit -m "refactor(app): migrate network detail panel to shared design system"`

---

### Task 62: Migrate `SandboxAppDetailPanel`

**Files:** Modify `frontend/src/lib/panels/SandboxAppDetailPanel.svelte`

**Interfaces:** Consumes `KeyValueList`, `StatusDot`, `Button`, `Card`; `toDotStatus`.

**Census:** 295 lines · 4 global `btn` · 3 `status-dot`.

**Mapping:** attributes → `KeyValueList` (status via `value` snippet + `StatusDot`); open-editor / start / stop → `Button` (link to the editor page via `Button href` if it was a link).

**Preserve:** start/stop actions, open-editor navigation.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — open a sandbox app from the canvas; status dot correct; open-editor navigates. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/SandboxAppDetailPanel.svelte && git commit -m "refactor(app): migrate sandbox app detail panel to shared design system"`

---

### Task 63: Migrate `VolumeDetailPanel`

**Files:** Modify `frontend/src/lib/panels/VolumeDetailPanel.svelte`

**Interfaces:** Consumes `KeyValueList`, `ActivityList`, `ListRow`, `Button`, `Spinner`, `ConfirmDialog`.

**Census:** 307 lines · 1 global `btn` · 2 spinners.

**Mapping:** attributes → `KeyValueList`; mounted-by list → `ActivityList`/`ListRow`; delete → `ConfirmDialog` if confirmed before.

**Preserve:** volume info load, delete flow and callback.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — open a volume from the canvas; dialogs cancel. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/VolumeDetailPanel.svelte && git commit -m "refactor(app): migrate volume detail panel to shared design system"`

---

### Task 64: Migrate the resource launchers (`AddResourcePanel`, `TemplatePanel`, `SandboxAppTemplatePanel`)

**Files:** Modify `frontend/src/lib/panels/AddResourcePanel.svelte`, `frontend/src/lib/panels/resources/TemplatePanel.svelte`, `frontend/src/lib/panels/resources/SandboxAppTemplatePanel.svelte`

**Interfaces:** Consumes `Card`, `ListRow`, `ActivityList`, `Button`, `Spinner`, `EmptyState`.

**Census:** 112 / 116 / 225 lines · `AddResourcePanel` 1 `pushPanel` (no global classes — option tiles only) · `TemplatePanel` 2 spinners, 3 `pushPanel` · `SandboxAppTemplatePanel` 3 global `btn`.

**Mapping:** resource/template option tiles → `Card` with `ListRow` content (icon snippet, title, description as `meta`) or keep a local tile grid restyled with tokens if `ListRow` cannot express the tile; buttons → `Button`; loading → `Spinner`; empty template list → `EmptyState`.

**Preserve:** every `pushPanel` call (which panel opens for which option, its props and callbacks).

- [ ] **Step 1–4:** as in "How every migration task works" (for all three files).
- [ ] **Step 5: Manual check** — from a project canvas, open Add Resource; each option opens its creation panel (then close it); the template picker lists templates and opens the next step. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/AddResourcePanel.svelte frontend/src/lib/panels/resources/TemplatePanel.svelte frontend/src/lib/panels/resources/SandboxAppTemplatePanel.svelte && git commit -m "refactor(app): migrate resource launcher panels to shared design system"`

---

### Task 65: Migrate `DatabasePanel` (create database)

**Files:** Modify `frontend/src/lib/panels/resources/DatabasePanel.svelte`

**Interfaces:** Consumes `FormField`, `TextField`, `Select`, `Button`, `InlineAlert`, `Spinner`.

**Census:** 344 lines · 5 global `btn` · **31 form markers** · 2 alert markers · 2 spinners · 3 `pushPanel` calls (pickers).

**Mapping:** every field → `FormField` + `TextField`/`Select` (keep `type`, `min`/`max`, `required`; any numeric helper accepts `string | number`); picker launchers (network/volume/etc.) → `Button` pushing the same picker panels; submit → `Button type="submit"`; errors → `InlineAlert`.

**Preserve:** validation, picker callbacks, submit + `onCreated` + `popPanel()`.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — open Add Resource › Database: every field renders; pickers open and return a selection; validation shows; do not create. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/resources/DatabasePanel.svelte && git commit -m "refactor(app): migrate database create panel to shared design system"`

---

### Task 66: Migrate `DockerComposePanel`

**Files:** Modify `frontend/src/lib/panels/resources/DockerComposePanel.svelte`

**Interfaces:** Consumes `Tabs`, `FormField`, `TextField`, `Textarea`, `Button`, `InlineAlert`, `Spinner`, `Card`.

**Census:** 1,062 lines · 8 global `btn` · 1 tab marker · 2 alert markers · 2 spinners · 1 `pushPanel`.

**Mapping:** source/mode tabs → `Tabs` (button mode); compose YAML input → `Textarea` with `spellcheck={false}` and the mono font; parsed-services preview → `Card` list; fields → `FormField` + controls; submit → `Button`; errors → `InlineAlert`.

**Preserve:** YAML parsing/validation, git source option (its sub-panel), submit + callback.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — open Add Resource › Docker Compose: paste a small compose snippet → the preview parses; switch tabs; do not create. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/resources/DockerComposePanel.svelte && git commit -m "refactor(app): migrate docker compose panel to shared design system"`

---

### Task 67: Migrate `DockerImagePanel`

**Files:** Modify `frontend/src/lib/panels/resources/DockerImagePanel.svelte`

**Interfaces:** Consumes `FormField`, `TextField`, `Select`, `Toggle`, `Button`, `InlineAlert`, `Spinner`.

**Census:** 508 lines · 8 global `btn` · **35 form markers** · 2 alert markers · 2 spinners · 4 `pushPanel` calls.

**Mapping:** fields → `FormField` + controls (image, tag, registry credentials as `type="password"`, port/replica numbers with `string | number`-safe helpers); picker launchers → `Button`; submit → `Button`; errors → `InlineAlert`.

**Preserve:** registry/artifactory picker callbacks, port mapping sub-panel, submit + callback.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — open Add Resource › Docker Image: fields, pickers, validation; do not create. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/resources/DockerImagePanel.svelte && git commit -m "refactor(app): migrate docker image panel to shared design system"`

---

### Task 68: Migrate the domain-add panels (`DomainAddPanel`, `EdgeFnDomainAddPanel`)

**Files:** Modify `frontend/src/lib/panels/resources/DomainAddPanel.svelte`, `frontend/src/lib/panels/resources/EdgeFnDomainAddPanel.svelte`

**Interfaces:** Consumes `FormField`, `TextField`, `Select`, `Toggle`, `Button`, `InlineAlert`, `KeyValueList`.

**Census:** 370 / 348 lines · 3 global `btn` each · 17 form markers each · 2 alert markers each · 2 spinners each. The two files share the same structure — migrate them identically.

**Mapping:** domain input → `TextField` with a lucide `Globe` icon snippet (it was icon-prefixed); path/port/TLS options → `FormField` + `TextField`/`Select`/`Toggle`; DNS instructions → `KeyValueList` (record type/name/value, mono values) with copy `Button`s; submit → `Button`; errors → `InlineAlert`.

**Preserve:** validation, DNS-record display, submit + callback + `popPanel()`.

- [ ] **Step 1–4:** as in "How every migration task works" (both files).
- [ ] **Step 5: Manual check** — open Add Domain from a service and from an edge function: fields, validation, DNS instructions; do not add a domain. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/resources/DomainAddPanel.svelte frontend/src/lib/panels/resources/EdgeFnDomainAddPanel.svelte && git commit -m "refactor(app): migrate domain add panels to shared design system"`

---

### Task 69: Migrate `EdgeFunctionPanel` (create edge function)

**Files:** Modify `frontend/src/lib/panels/resources/EdgeFunctionPanel.svelte`

**Interfaces:** Consumes `FormField`, `TextField`, `Select`, `Button`, `InlineAlert`, `Spinner`.

**Census:** 402 lines · 8 global `btn` · 13 form markers · 2 alert markers · 4 spinners · 4 `pushPanel` calls.

**Mapping:** fields → `FormField` + controls; source pickers (git repo/branch/artifactory) → `Button` pushing the same panels; submit → `Button`; errors → `InlineAlert`.

**Preserve:** picker callbacks, validation, submit + callback.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — open Add Resource › Edge Function: fields and pickers; do not create. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/resources/EdgeFunctionPanel.svelte && git commit -m "refactor(app): migrate edge function create panel to shared design system"`

---

### Task 70: Migrate `GitRepoPanel` (create from git repo)

**Files:** Modify `frontend/src/lib/panels/resources/GitRepoPanel.svelte`

**Interfaces:** Consumes `FormField`, `TextField`, `Select`, `Toggle`, `Button`, `InlineAlert`, `Spinner`, `ListRow`.

**Census:** 547 lines · 10 global `btn` · **30 form markers** · 2 alert markers · 4 spinners · **7 `pushPanel` calls**.

**Mapping:** fields → `FormField` + controls; the selected account/repo/branch summaries → `ListRow`s with a change `Button` that pushes the same picker; submit → `Button`; errors → `InlineAlert`.

**Preserve:** all 7 picker pushes and their `onSelect` callbacks, build settings, submit + callback.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — open Add Resource › Git Repo: pick account → repo → branch through the pickers; fields validate; do not create. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/resources/GitRepoPanel.svelte && git commit -m "refactor(app): migrate git repo create panel to shared design system"`

---

### Task 71: Migrate `StaticSitePanel` (create static site)

**Files:** Modify `frontend/src/lib/panels/resources/StaticSitePanel.svelte`

**Interfaces:** Consumes `FormField`, `TextField`, `Select`, `Toggle`, `Button`, `InlineAlert`, `Spinner`, `ListRow`.

**Census:** 685 lines · 6 global `btn` · **32 form markers** · 2 spinners · 5 `pushPanel` calls.

**Mapping:** fields → `FormField` + controls; source summaries → `ListRow` + change `Button`; submit → `Button`; errors → `InlineAlert`.

**Preserve:** picker pushes and callbacks, build/output settings, submit + callback.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — open Add Resource › Static Site: pickers, fields, validation; do not create. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/resources/StaticSitePanel.svelte && git commit -m "refactor(app): migrate static site create panel to shared design system"`

---

### Task 72: Migrate the git pickers (`GitAccountPickerPanel`, `GitBranchPickerPanel`, `GitRepoPickerPanel`)

**Files:** Modify `frontend/src/lib/panels/resources/GitAccountPickerPanel.svelte`, `GitBranchPickerPanel.svelte`, `GitRepoPickerPanel.svelte`

**Interfaces:** Consumes `SearchInput`, `ActivityList`, `ListRow`, `Button`, `Spinner`, `EmptyState`; lucide icons.

**Census:** 74 / 153 / 370 lines · `GitAccountPickerPanel` **3 inline `<svg>`** (provider marks — lucide `Github`/`Gitlab`/`GitBranch`, generic icon where no brand icon exists, noted in the report) · 2–4 spinners · `GitRepoPickerPanel` 1 global `btn`.

**Mapping:** search box → `SearchInput` (keep the existing filter/debounce logic and `autofocus` behavior if present); option rows → `ActivityList` + `ListRow` (selected state shown with a check icon in `trailing`); loading → `Spinner`; empty → `EmptyState`.

**Preserve:** search/filter, selection → `onSelect` callback → `popPanel()`.

- [ ] **Step 1–4:** as in "How every migration task works" (all three files).
- [ ] **Step 5: Manual check** — from `GitRepoPanel`, open each picker: search filters, selecting returns to the parent with the choice filled in. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/resources/GitAccountPickerPanel.svelte frontend/src/lib/panels/resources/GitBranchPickerPanel.svelte frontend/src/lib/panels/resources/GitRepoPickerPanel.svelte && git commit -m "refactor(app): migrate git picker panels to shared design system"`

---

### Task 73: Migrate the other pickers (`NetworkPickerPanel`, `VolumePickerPanel`, `ArtifactoryPickerPanel`)

**Files:** Modify `frontend/src/lib/panels/resources/NetworkPickerPanel.svelte`, `VolumePickerPanel.svelte`, `ArtifactoryPickerPanel.svelte`

**Interfaces:** Consumes `SearchInput`, `ActivityList`, `ListRow`, `Checkbox`, `Button`, `Spinner`, `EmptyState`.

**Census:** 145 / 150 / 244 lines · 1 global `btn` each · 2 spinners each. `VolumePickerPanel` is multi-select (a `Set` of selected ids).

**Mapping:** search → `SearchInput`; rows → `ActivityList` + `ListRow` (multi-select rows use `Checkbox` in `trailing`); confirm → `Button`; loading/empty → `Spinner`/`EmptyState`.

**Preserve:** selection semantics (single vs. multi), confirm callback, `popPanel()`.

- [ ] **Step 1–4:** as in "How every migration task works" (all three files).
- [ ] **Step 5: Manual check** — open each picker from its parent panel: search, select (multi for volumes), confirm returns the selection. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/resources/NetworkPickerPanel.svelte frontend/src/lib/panels/resources/VolumePickerPanel.svelte frontend/src/lib/panels/resources/ArtifactoryPickerPanel.svelte && git commit -m "refactor(app): migrate network, volume and artifactory pickers to shared design system"`

---

### Task 74: Migrate the small create panels (`NetworkPanel`, `VolumePanel`, `PortMappingPanel`)

**Files:** Modify `frontend/src/lib/panels/resources/NetworkPanel.svelte`, `VolumePanel.svelte`, `PortMappingPanel.svelte`

**Interfaces:** Consumes `FormField`, `TextField`, `Select`, `Button`, `InlineAlert`.

**Census:** 102 / 100 / 282 lines · `NetworkPanel` and `VolumePanel`: 2 global `btn`, 10–12 form markers, 2 alert markers each · `PortMappingPanel`: 2 global `btn`, a list of port rows.

**Mapping:** fields → `FormField` + `TextField`/`Select`; port rows → local row grid using `TextField type="number"` (helpers accept `string | number`) + remove `Button size="icon"`; add row / submit → `Button`; errors → `InlineAlert`.

**Preserve:** validation, port parsing (`parsePortString`), submit/apply callbacks, `popPanel()`.

- [ ] **Step 1–4:** as in "How every migration task works" (all three files).
- [ ] **Step 5: Manual check** — open each from its parent: fields validate; port rows add/remove; do not create resources. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/panels/resources/NetworkPanel.svelte frontend/src/lib/panels/resources/VolumePanel.svelte frontend/src/lib/panels/resources/PortMappingPanel.svelte && git commit -m "refactor(app): migrate network, volume and port mapping panels to shared design system"`

---

## Part H — Page components

### Task 75: Migrate `GitSettingsSection`

**Files:** Modify `frontend/src/lib/components/GitSettingsSection.svelte`

**Interfaces:** Consumes `Card`, `FormField`, `TextField`, `Select`, `Toggle`, `Button`, `InlineAlert`, `Spinner`, `KeyValueList`.

**Census:** 508 lines · 6 global `btn` · 6 spinners. Used inside service/static-site/edge-function git tabs.

**Mapping:** section → `Card`; repo/branch/auto-deploy settings → `FormField` + controls; webhook URL/secret → `KeyValueList` with copy `Button`s; save/sync → `Button` with loading text; messages → `InlineAlert`.

**Preserve:** git config save, webhook regenerate/copy, auto-deploy toggle, every prop and callback the parent panels pass.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — open the git tab of a service and of a static site: settings load; copy works; do not regenerate secrets or save changes to real repos (edit + revert only). Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/components/GitSettingsSection.svelte && git commit -m "refactor(app): migrate git settings section to shared design system"`

---

### Task 76: Migrate `VolumeMountList`

**Files:** Modify `frontend/src/lib/components/VolumeMountList.svelte`

**Interfaces:** Consumes `ActivityList`, `ListRow`, `TextField`, `Button`, `EmptyState`.

**Census:** 239 lines · 4 global `btn` · 1 `pushPanel` (volume picker).

**Mapping:** mount rows → `ActivityList`/`ListRow` with an editable mount-path `TextField` (mono) and a remove `Button size="icon"`; add → `Button` pushing the same picker; empty → `EmptyState`.

**Preserve:** picker callback, mount path edits, remove, the bound value/callback contract with parents.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — in a service's volumes tab (or a create panel that embeds it): add opens the picker; rows edit/remove locally; do not save. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/components/VolumeMountList.svelte && git commit -m "refactor(app): migrate volume mount list to shared design system"`

---

### Task 77: Migrate `LogViewerOverlay`

**Files:** Modify `frontend/src/lib/components/LogViewerOverlay.svelte`

**Interfaces:** Consumes `Button`, `Badge`, `Select`, `SearchInput`, `Spinner`, `DataTable` (if the 1 `<table>` is a structured log table; if it is the log stream body, leave it).

**Census:** 729 lines · 6 global `btn` · **7 global `badge`** · 1 `<table>` · 3 spinners. Also used by admin pages (`admin/static`, `admin/traefik/settings`) — they must keep working.

**Mapping:** overlay header/toolbar buttons → `Button`; level/status pills → `Badge`; line-count/source selectors → `Select`; filter → `SearchInput`; loading → `Spinner`. The streamed log lines and their `.log-*` colors stay.

**Preserve:** stream connect/disconnect, line limits, filters, copy, close — for every caller's props (`open`, `title`, `subtitle`, `streamUrl`, `fetchFn`, `onClose`).

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — open "Show Log Stream" on `/admin/static` and from a main-app log entry point: connect, filter, close. Both themes.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/components/LogViewerOverlay.svelte && git commit -m "refactor(app): migrate log viewer overlay to shared design system"`

---

### Task 78: Migrate `MonitorViewOverlay`

**Files:** Modify `frontend/src/lib/components/MonitorViewOverlay.svelte`

**Interfaces:** Consumes `Button`, `StatCard`, `ProgressBar`, `Spinner`; lucide icons.

**Census:** 510 lines · 1 global `btn` · **4 inline `<svg>`** · 2 spinners.

**Mapping:** inline `<svg>` → lucide icons if they are icons; if any `<svg>` draws a metric sparkline/chart, keep the chart's own geometry (the chart-geometry exception) but make sure it uses only tokens for colors; metric tiles → `StatCard` (+ `ProgressBar` where there is a percentage); close/refresh → `Button`.

**Preserve:** live metric polling/streaming, time-range controls, close.

- [ ] **Step 1–4:** as in "How every migration task works" (any kept chart `<svg>` is listed in the report as the documented exception).
- [ ] **Step 5: Manual check** — open the monitor view for a running service: metrics update over ~10 seconds; close works. Both themes.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/components/MonitorViewOverlay.svelte && git commit -m "refactor(app): migrate monitor overlay to shared design system"`

---

### Task 79: Migrate `DbClientModal` chrome (SQL editor and results grid core stay)

**Files:** Modify `frontend/src/lib/components/DbClientModal.svelte`

**Interfaces:** Consumes `Button`, `Tabs`, `Select`, `Badge`, `InlineAlert`, `Spinner`, `ActivityList`, `ListRow`.

**Census:** 1,044 lines · 6 global `btn` · 1 `<table>` (the query results grid).

**Mapping:** toolbar (run, format, export, close) → `Button`; connection/database selector → `Select`; query tabs (if any) → `Tabs`; schema sidebar → `ActivityList`/`ListRow` or local list restyled with tokens; errors → `InlineAlert`; loading → `Spinner`. **Keep** the SQL editor and the results grid rendering (results are dynamic-column, potentially large, and virtualised/hand-tuned — not a `DataTable` candidate); restyle the grid's colors with tokens only.

**Preserve:** query run, results paging/export, schema browsing, the modal open/close contract.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — open the DB client from settings › database: run a read-only query (`SELECT 1`), browse schema, close. Do not run writes. Both themes.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/components/DbClientModal.svelte && git commit -m "refactor(app): migrate db client chrome to shared design system"`

---

### Task 80: Migrate `LogViewer` chrome and `EnvManagerOverlay`

**Files:** Modify `frontend/src/lib/components/LogViewer.svelte`, `frontend/src/lib/components/EnvManagerOverlay.svelte`

**Interfaces:** Consumes `Button`, `SearchInput`, `Toggle`.

**Census:** `LogViewer` 337 lines · 4 global `btn` (log body is a specialised core) · `EnvManagerOverlay` 101 lines · 1 global `btn`.

**Mapping:** `LogViewer` toolbar buttons (pause, clear, wrap, download, follow) → `Button`/`Toggle`, filter → `SearchInput`; the log line rendering stays. `EnvManagerOverlay` frame buttons → `Button` (it hosts `EnvManagerPanel`, migrated in Task 57).

**Preserve:** follow/pause/clear behavior, filtering, the overlay open/close contract.

- [ ] **Step 1–4:** as in "How every migration task works" (both files).
- [ ] **Step 5: Manual check** — service logs tab: toolbar controls work while streaming; open the env manager overlay and close it. Both themes.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/components/LogViewer.svelte frontend/src/lib/components/EnvManagerOverlay.svelte && git commit -m "refactor(app): migrate log viewer and env overlay chrome to shared design system"`

---

### Task 81: Migrate feedback components (`PermissionDeniedDialog`, `Toast`, `AlertToast`)

**Files:** Modify `frontend/src/lib/components/PermissionDeniedDialog.svelte`, `Toast.svelte`, `AlertToast.svelte`

**Interfaces:** Consumes `Modal`, `Button`, `InlineAlert` (for visual vocabulary only).

**Census:** `PermissionDeniedDialog` 144 lines · 3 global `btn` (a hand-rolled dialog) · `Toast` 114 lines and `AlertToast` 142 lines · no global classes.

**Mapping:** `PermissionDeniedDialog` → built on `Modal` (title, message in `children`, actions in `footer` as `Button`s) — it gains the focus trap; `Toast`/`AlertToast` keep their own positioning/animation but their tone colors must use `--accent-green/-red/-yellow` + `-muted` tokens (no hardcoded hex) and their close buttons become `Button size="icon"` with `aria-label`.

**Preserve:** when each appears (store-driven), auto-dismiss timing, stacking, close behavior.

- [ ] **Step 1–4:** as in "How every migration task works" (all three files).
- [ ] **Step 5: Manual check** — trigger a toast (e.g. copy something that toasts, or a harmless failed request) and close it; trigger the permission-denied dialog if reachable with a restricted action, otherwise verify by temporarily setting its store flag in devtools. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/components/PermissionDeniedDialog.svelte frontend/src/lib/components/Toast.svelte frontend/src/lib/components/AlertToast.svelte && git commit -m "refactor(app): migrate feedback components to shared design system"`

---

### Task 82: `CommandPalette` alignment

**Files:** Modify `frontend/src/lib/components/CommandPalette.svelte`

**Interfaces:** Consumes tokens only (and `Spinner` if it shows loading).

**Census:** 466 lines · no global classes · no inline `<svg>`.

**Mapping:** the palette is a specialised keyboard UI (open on ⌘K, arrow navigation, grouped results) and keeps its own markup. Align its look with the design system: surfaces/borders/radius/shadow/text colors use only `layout.css` tokens (`--bg-surface`, `--border`, `--radius-lg`, `--shadow-lg`, `--accent-muted` for the active row, `--font-mono` for shortcut hints); remove any hardcoded hex colors.

**Preserve:** ⌘K / palette button opening, keyboard navigation, every command's action.

- [ ] **Step 1–4:** as in "How every migration task works".
- [ ] **Step 5: Manual check** — ⌘K opens it; type to filter; arrow keys move; Enter runs a harmless navigation command; Escape closes. Both themes, 400px.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/components/CommandPalette.svelte && git commit -m "refactor(app): align command palette with design system tokens"`

---

### Task 83: Specialised components — chrome leftovers only

**Files:** Review/modify `frontend/src/lib/components/FileTree.svelte`, `SandboxTerminal.svelte`, `CodeEditor.svelte`, `BrandLogo.svelte`

**Interfaces:** Consumes `Button`, `StatusDot`; `toDotStatus`.

**Census:** `FileTree` 237 lines · 1 global `btn` · `SandboxTerminal` 302 lines · 2 `status-dot` · `CodeEditor` 159 lines and `BrandLogo` 85 lines · no global classes.

**Mapping:** `FileTree`'s one button → `Button`; `SandboxTerminal`'s connection dots → `StatusDot` via `toDotStatus` (or the literal status it represents). Do **not** change tree rendering, terminal rendering/socket wiring, editor setup, or the brand mark artwork.

**Preserve:** everything — these are specialised cores.

- [ ] **Step 1–4:** as in "How every migration task works" (step 3's grep must pass for all four files).
- [ ] **Step 5: Manual check** — the editor page (Task 26) still loads the tree, editor and terminal; the terminal's connection dot shows the right state. Both themes.
- [ ] **Step 6: Commit** — `git add frontend/src/lib/components/FileTree.svelte frontend/src/lib/components/SandboxTerminal.svelte && git commit -m "refactor(app): migrate specialised component chrome to shared design system"` (add `CodeEditor.svelte`/`BrandLogo.svelte` only if they changed).

---

## Part I — Cleanup and acceptance

### Task 84: Delete the old global classes and sidebar tokens

**Files:**
- Modify: `frontend/src/routes/layout.css`
- Modify: `frontend/src/lib/panels/StoragePreviewPanel.svelte` (admin — its download links still use global `btn` classes)
- Modify as needed: any file the greps below still flag

**Interfaces:** Consumes `Button` link mode (Task 2).

- [ ] **Step 1: Convert the known straggler.** In `StoragePreviewPanel.svelte`, replace `<a class="btn btn-secondary" …>` / `<a class="btn btn-primary" …>` download links with `Button href=… variant="secondary"|"primary"` (keep `download`/`target` behavior: if a link uses the `download` attribute, keep a native `<a download>` styled with local token CSS instead, because `Button` does not forward `download`).
- [ ] **Step 2: Find every remaining global-class user across the whole frontend.**

```bash
cd frontend && grep -rnE 'class="([^"]* )?(btn|badge|status-dot|card|card-interactive|input)( [^"]*)?"' src --include='*.svelte'
```

For each hit: if the class is defined in that file's own `<style>` (a local look-alike, e.g. `admin/login/+page@.svelte`), rename it to a file-specific name (e.g. `.login-btn`) so it cannot be confused with the deleted globals; otherwise migrate it to the component (`Button`/`Badge`/`StatusDot`/`Card`/`TextField`). Re-run until the grep prints nothing.

- [ ] **Step 3: Delete the globals from `layout.css`.** Remove the `.status-dot` block and all its variants, every `.btn` rule (`.btn`, `:focus-visible`, `-primary`, `-secondary`, `-danger`, `-ghost`, `-sm`, `-xs`, `-danger-outline`, `-icon` and hovers), `.card`, `.card:hover`, `.card-interactive` (+ hover), `.input` (+ focus, placeholder), and every `.badge` rule. Keep the `.log-*` colors. For `@keyframes pulse`: run `grep -rn "animation:[^;]*\bpulse\b" src` — delete the keyframes only if nothing references them; otherwise keep them (it is an animation primitive, not a global class).
- [ ] **Step 4: Delete the sidebar tokens.** Remove `--sidebar-bg`, `--sidebar-surface`, `--sidebar-border`, `--sidebar-text`, `--sidebar-text-hover`, `--sidebar-text-active`, `--sidebar-hover-bg`, `--sidebar-active-bg`, `--sidebar-active-border`, `--sidebar-glow`, `--sidebar-width`, and `--context-panel-width` (with any dark-theme overrides), after confirming `grep -rn "sidebar-\|context-panel-width" src` finds no users outside `layout.css`.
- [ ] **Step 5: Confirm the end-state greps** (spec success criteria):

```bash
cd frontend
grep -rnE 'class="([^"]* )?(btn|badge|status-dot|card|card-interactive|input)( [^"]*)?"' src --include='*.svelte'   # → nothing
grep -rln '<svg' src/routes src/lib/panels src/lib/components --include='*.svelte' | grep -v 'components/ui/\(AreaChart\|BarChart\|DonutChart\)'   # → nothing (or only documented chart exceptions from Task 78)
grep -rnE 'var\(--(surface|surface-2|text-2|text-3|ok|danger|warn|mono|font)\)|var\(--radius\)' src --include='*.svelte'   # → nothing
grep -rn 'sidebar-' src   # → nothing
```

- [ ] **Step 6: Verify** — `npm run check` (no new errors/warnings vs. the baseline) and `npm run build` (exit 0).
- [ ] **Step 7: Commit**

```bash
git add frontend/src/routes/layout.css frontend/src/lib/panels/StoragePreviewPanel.svelte
git add -u frontend/src
git commit -m "cleanup(app): remove legacy global classes and sidebar tokens"
```

---

### Task 85: Full acceptance walkthrough

**Files:** none (verification only; fix-forward any regression in the owning file with its own small commit).

- [ ] **Step 1: Enumerate the routes** from the directory tree: `find frontend/src/routes -name '+page*.svelte' -not -path '*/admin/*'` (do not build the list from memory or from nav config).
- [ ] **Step 2: Walk every route** logged in as the superadmin test user, in light and dark theme, at desktop width and at 400px: the shell is consistent (rail, project drawer, phone bottom bar — Review Focus #1), no page looks like the old design, no console errors (`read the console after each load`), empty/loading/error states visible where data is missing (Review Focus #5).
- [ ] **Step 3: Walk every slide panel** reachable from the project canvas (service, static site, edge function, database, network, volume, sandbox app) and each creation panel under Add Resource — open, switch every tab, open every sub-panel, cancel every confirmation. Check at 400px.
- [ ] **Step 4: Keyboard pass** (Review Focus #2): account menu, one dialog per kind (`Modal`, `ConfirmDialog`), button tabs in a detail panel, the command palette.
- [ ] **Step 5: Admin sanity sweep** — because shared components changed (Tasks 2–8), load every `/admin/*` route once in both themes and confirm it renders as before.
- [ ] **Step 6: Record the result** in the task report: routes and panels checked, any fixes made (with commit SHAs), any route that could not be exercised and why.

---

## Self-review notes

- **Spec coverage:** foundation components (Tasks 1–9), shell incl. phone mode and account menu (10–11), root/error (12), entry pages (13–19), org pages (20–26), registry (27–31), settings (32–48, with infra split 43–44), panels (49–74, with the three largest split by tab), page components (75–83), cleanup and acceptance (84–85). Every file in the spec's inventory has an owning task; `settings/+page.svelte` (8-line redirect) and `routes/+page.svelte` are covered by Task 12's review step.
- **Review Focus pins:** phone width → Tasks 7, 8, 11, 85; keyboard → 3, 4, 5, 10/11, 85; permission gating → 10/11, 32, 36, 37 and the panel tasks; long text → 6, 8, 11; empty/loading/error → 8, 11, 45 and every page task's manual step.
- **Interfaces used later match their definitions:** `toDotStatus` (Task 1), `Card tone` / `Button href|title` / form passthrough (Task 2), `Tabs`/`TabItem` with `value`+`onChange` (Task 5), `KeyValueList`/`KeyValueItem` with `value`/`action` snippets (Task 6), `NavRail` `href`/`active`/`footer`/`phoneBar` (Task 7), `NavDrawer` `header`/`footer`/`loading`/`emptyText`/`hideOnPhone` (Task 8), `Dropdown` `menu`/`placement`/`triggerLabel` (Task 4).

