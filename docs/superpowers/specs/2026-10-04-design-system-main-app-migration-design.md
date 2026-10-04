# Design System — Main App Migration

**Date:** 2026-10-04
**Status:** Approved design (brainstorming), pending implementation plan
**Predecessor:** `docs/superpowers/specs/2026-09-29-design-system-admin-migration-design.md` (admin panel migration, complete and merged)

## Goal

Move the Shipyard main app — every non-admin route, slide panel, and page
component — onto the shared component library in
`frontend/src/lib/components/ui/`, so the main app and the admin panel share
one design system. This is a **design-system and component refinement, not a
behavior change**: every user flow stays exactly as it is today.

## What the user asked for (verbatim intent)

- Migrate the main app using the new design system and components.
- Existing flows remain the same as the existing main app; this only refines
  the design system and components.
- Scope decision: **one plan** covering everything (like admin), not phased
  sub-projects.
- Shell decision: **rebuild the navigation shell on `NavRail` + `NavDrawer`**
  (same items and flow, light surface matching admin; the dark navy sidebar
  look goes away).
- End-state decision: **delete the main app's global CSS classes** at the end
  so the component library is the single styling source of truth.
- Component decision: **add a few** new shared components where the main app
  repeats a pattern across many files (not strict reuse, not add-freely).

## Starting point (surveyed 2026-10-04)

| Area | Count | Size |
|---|---|---|
| Non-admin pages | 35 `+page*.svelte` (incl. 2 redirect stubs) | ~19,000 lines incl. layouts |
| Layouts | 4 (`+layout.svelte`, `orgs/[orgSlug]`, `.../settings`, `.../registry`) | |
| Slide panels (`lib/panels/**`) | 31 main-app (excludes admin's `StoragePreviewPanel`) | ~17,650 lines |
| Page components (`lib/components/*`, non-`ui`) | 19 | ~6,200 lines |

Findings that shape the design:

- The main app already has its **own design system**: global classes in
  `frontend/src/routes/layout.css` (`btn` + variants, `badge-*`,
  `status-dot`, form classes) plus copy-pasted patterns (tabs, kv-grid,
  hero row, danger section, in-panel delete modal, err/ok messages, spinners,
  empty states). This migration replaces that system with the component
  library; it is not cleaning up ad-hoc markup.
- Icons are nearly done: 7 non-admin files still contain inline `<svg>`.
- Tokens are nearly done: ~6 files still use legacy names (`--surface`,
  `--surface-2`, bare `--radius`) that are **already broken** in the main app
  (no bridge exists outside admin).
- Only 1 non-admin file imports the component library today.
- The shell is `IconSidebar` (dark navy, 52px; four direct-link items Home /
  Projects / Registry / Settings; bottom: command palette, theme toggle,
  context-panel collapse toggle, account menu) + `ContextPanel` (220px, always
  visible, collapsible; org header, project list with loading/empty states,
  New Project button).
- The shell has a **phone layout**: below 640px the icon sidebar becomes a
  56px bottom navigation bar (logo, palette and collapse toggle hidden,
  account button kept), the context panel hides, and slide panels go
  full-width.
- Settings and registry layouts use **horizontal link tab bars** (settings
  tabs carry icons and an "Admin" badge).

## Global constraints (carried from admin, binding on every task)

- **No new color, radius, or font tokens.** Use only tokens already defined in
  `frontend/src/routes/layout.css`.
- **Material 3 governs structure, states, and anatomy only** — never a visual
  reskin with M3 default colors or type scale.
- **Icons are `@lucide/svelte` only.** Zero inline `<svg>` in migrated code
  (chart geometry inside chart components is the only exception, as in admin).
- **Svelte 5 runes only** (`$props`, `$state`, `$derived`, `$bindable`,
  snippets). Never use `class:` directives on Svelte *components* (native
  elements are fine) — pass a class string instead.
- **Behavior is preserved.** Script logic stays unchanged unless a
  component's API forces a change; any forced change is disclosed in the task
  report. Same routes, same panel stacking, same callbacks, same confirmation
  steps, same permission gating.
- **No backend changes.** If a `DataTable` would need a backend change to run
  in server mode, it uses client mode instead.
- **No frontend test framework exists.** Verification per task is
  `npm run check` (no new errors/warnings), `npm run build` (succeeds), and a
  manual browser check in light and dark theme — plus phone width (~400px) for
  anything touching navigation or panels.

## Scope

**In scope**

- All 35 non-admin pages and the 4 non-admin layouts.
- All 31 main-app slide panels.
- All 19 page components, including `IconSidebar`, `ContextPanel`,
  `PanelContainer`, `SlidePanel`.
- New/extended shared components listed below.
- Final cleanup of the global classes, `--sidebar-*` tokens, legacy token
  names, and inline `<svg>`.

**Out of scope**

- The admin panel (already migrated), except where an extended shared
  component must stay backward compatible with it.
- Backend changes.
- The **core rendering** of specialised screens: the project topology canvas,
  the code editor (`CodeEditor`), the terminal (`SandboxTerminal`), the live
  log stream body (`LogViewer`), `FileTree`, and the SQL editor inside
  `DbClientModal`. Their surrounding chrome (toolbars, buttons, headers,
  empty/loading/error states) **is** in scope.
- New features, new flows, copy changes beyond what a component requires.

## Component library changes

### New components

- **`Tabs`** — one component covering the three tab styles in the main app:
  - link tabs (`href` per tab, active from route) for the settings and
    registry layouts, with optional per-tab icon and trailing badge;
  - button tabs (controlled `value`, `onChange`) for in-panel tab rows;
  - a `danger` flag per tab for the Danger tab (red active state).
- **`KeyValueList`** — the kv-grid: bordered rows of muted key / primary
  value, values truncate with ellipsis, optional mono flag per value, optional
  trailing action per row.

### Extensions (backward compatible — admin must render unchanged)

- **`NavRail`**
  - Item `href`: an item with `href` is a link that navigates and highlights
    from the current route instead of toggling a drawer. Admin keeps drawer
    mode by not passing `href`.
  - `footer` snippet for bottom actions (command palette, theme toggle,
    collapse toggle, account menu).
  - Opt-in phone mode (prop): below 640px render as a 56px bottom navigation
    bar mirroring today's main-app phone layout. Admin does not opt in.
- **`NavDrawer`** — `header` and `footer` snippets; `loading` and `emptyText`
  props; item `icon` optional (project items show a dot).
- **`Card`** — `tone="danger"` (red border) for danger sections.
- **`Dropdown`** — keyboard support (arrow keys move, Enter activates, Escape
  closes and returns focus) since the account menu depends on it.
- **`Modal`** (and therefore `ConfirmDialog`) — focus trap and focus restore
  on close, and a unique title id per instance, since many more dialogs will
  rely on it.

## Shell design

- `orgs/[orgSlug]/+layout.svelte` renders `NavRail` (link items Home,
  Projects, Registry, Settings; footer with palette / theme / collapse /
  account) + a **persistent** `NavDrawer` for projects + main content +
  `PanelContainer`.
- The project drawer shows: org avatar initial + name + slug (header),
  loading state, empty state ("No projects yet"), project items (dot, name,
  active from route), and the New Project button (footer) with its current
  behavior unchanged.
- The existing collapse toggle (`uiStore.sidebarCollapsed`) drives the
  drawer's open state. Because the persistent drawer is in normal flow, the
  fixed-position context panel and the hand-calculated content margin are
  removed.
- The account menu moves to `Dropdown` with the same entries and gating:
  profile, billing, update notice + update action (permission-gated), admin
  link (superadmin only), log out. The update badge on the avatar stays.
- Theme toggle keeps the same `shipyard_theme` localStorage key and
  `data-theme` mechanism.
- Phone width: `NavRail` phone mode (bottom bar), project drawer hidden,
  slide panels full width — matching today's behavior.

## Pattern mapping (pages, panels, components)

| Current pattern | Becomes |
|---|---|
| `.tabs` / `.tab` / `.tab-danger`, settings/registry tab bars | `Tabs` |
| `.kv-grid` | `KeyValueList` |
| `.section-head` (title + action) | `SectionLabel` + `Button` |
| `.hero-row` (icon + name + sub-line) | `ListRow` inside `Card` |
| list rows | `ActivityList` / `ListRow` |
| tabular lists | `DataTable` (client mode by default) |
| `.danger-section` | `Card tone="danger"` |
| in-panel delete modal (type-to-confirm) | `ConfirmDialog` with `confirmText` |
| `.form-group` / `.form-label` / `.form-input` / `.form-hint` | `FormField` + `TextField` / `Select` / `Textarea` / `Toggle` / `Checkbox` |
| icon-prefixed input | `TextField` with `icon` snippet |
| `.err-msg` / `.ok-msg` | `InlineAlert` |
| `btn btn-*` | `Button` variants |
| `badge badge-*` | `Badge` tones |
| `status-dot` | `StatusDot` |
| `.btn-spinner` / `.spinner-sm` | `Spinner` (or `Button` loading text) |
| `.empty-state` | `EmptyState` |
| page title + subtitle | `PageHeader` (standalone pages only — never under a layout that already renders a header) |

`DataTable` rules (from admin's Ruling 10): client mode unless the endpoint
already returns a `total`; show the search box only if the data source
actually supports searching.

The confirmation dialog moves from panel-scoped (`position:absolute` over the
panel) to `ConfirmDialog` (full-screen scrim). The confirmation step and the
type-to-confirm requirement are unchanged; only the scrim covers the whole
screen instead of the panel.

## Execution order (one plan)

1. **Foundation** — `Tabs`, `KeyValueList`, `Card` danger tone, `NavRail`
   link items + footer + phone mode, `NavDrawer` header/footer/loading,
   `Dropdown` keyboard support, `Modal` focus trap/restore/unique id,
   `PanelContainer`/`SlidePanel` frame restyle.
2. **Shell** — org layout, rail, project drawer, root layout.
3. **Entry pages** — login, register, setup, onboarding, accept-invite,
   unauthorized, root page, org picker.
4. **Org pages** — org home, profile, billing, projects list, project page
   (canvas chrome), project settings, service editor page (chrome).
5. **Registry** — layout + browse, namespace, repo, settings.
6. **Settings** — layout + general, providers, cloudflare, members,
   api-keys, deployments, deployment detail, audit, database, docker, infra,
   mqtt, smtp, static, traefik.
7. **Panels** — all 31, grouped (detail panels, create-resource panels,
   action/picker panels).
8. **Page components** — the remaining non-shell components.
9. **Cleanup** — delete global classes and `--sidebar-*` tokens; grep for
   zero legacy tokens, zero inline `<svg>`, zero global-class usage; full
   walkthrough of every non-admin route in both themes and at phone width.

Very large files are split across several tasks by tab/section:
`ServiceDetailPanel` (4,051 lines), `StaticSiteDetailPanel` (1,945),
`EdgeFunctionDetailPanel` (1,539), `settings/infra` (1,370).

## Verification and safety

- Per task: `npm run check`, `npm run build`, manual browser check in both
  themes (+ phone width for nav/panels), exercising the real flow — open the
  panel, fill the form, stop at confirmation steps. Never run destructive
  actions (delete, drop, prune, restart, deploy) against the shared dev
  environment.
- Each task gets its own review; the whole branch gets one final review.
- Work happens on a dedicated branch/worktree, not directly on `main`.
- Known environment caveat: restarting the backend issues a new token-signing
  secret, so verification sessions need re-login after any backend restart.

## Success criteria

- Every non-admin screen uses the component library and is visually
  consistent with the admin panel in both themes and at phone width.
- No user flow behaves differently (routes, panels, callbacks, permissions,
  confirmations).
- Global `btn`/`badge`/`status-dot`/form classes and `--sidebar-*` tokens are
  deleted; grep finds zero legacy token names, zero inline `<svg>`, zero
  global-class usage in non-admin code.
- `npm run check` shows no new errors or warnings; `npm run build` succeeds.
- Admin renders unchanged after the shared-component extensions.
