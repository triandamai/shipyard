# Shipyard Design System + Admin Panel Migration — Design Spec

**Date:** 2026-09-29
**Status:** Approved for implementation planning
**Scope:** Establish one shared design system (tokens + a reusable Svelte component library) for the whole Shipyard frontend, and migrate every admin panel page (`frontend/src/routes/admin/**`, 32 pages) onto it. The main app (org/project dashboard, sandbox editor, etc.) keeps its current `layout.css`-token-based styling as-is for now — it already speaks the target token language, it just lacks reusable components. Migrating it to the new component library is an explicit follow-up, not part of this plan.

## Context

Investigation of the admin panel (32 pages, ~8,650 lines) found two compounding problems, not one:

1. **Two parallel design systems.** `frontend/src/routes/admin/+layout.svelte` defines its own `.shell`-scoped CSS custom properties (`--bg`, `--accent: #1d4ed8`, `--radius: 9px`, `--ok`/`--warn`/`--danger`, `system-ui` font, its own near-black sidebar) that never reference the main app's `frontend/src/routes/layout.css` tokens (`--bg-base`, `--accent: #2563EB`, `--radius-sm/md/lg`, Inter/JetBrains Mono, etc.) at all — including a second, independent dark/light theme toggle (`localStorage['shipyard-admin-theme']`) disconnected from the main app's own toggle. Admin doesn't look "a bit different" from the rest of Shipyard; it's a structurally separate visual system that happens to occupy the same color family by coincidence.

2. **Near-zero component reuse.** Across all 32 admin pages, only 2 import anything from `$lib/components`/`$lib/panels` (both incidental — a log-viewer overlay). Every data table, stat card, settings form, skeleton loader, badge, modal, and pagination control is hand-rolled per page in a local `<style>` block, duplicating the same CSS dozens of times with small unintentional variations (e.g. one page's primary button renders black text on a blue background — a legibility bug that a shared `Button` component would have made structurally impossible).

Sampling representative pages surfaced three dominant recurring patterns that account for the majority of the 32 pages:

- **Data table** — search/filter toolbar, expandable rows, pagination, a separate hand-duplicated mobile card-list fallback, loading skeleton, empty/error states (e.g. `docker/containers`, and the same shape in `docker/images`, `docker/volumes`, `docker/networks`, `docker/services`, `users`, `orgs`, `projects`, `staff`, `audit`, `mqtt/*`, `nodes`, `deployments`)
- **Dashboard/stat overview** — page header, stat-card grid, nav-card grid, skeleton, section dividers (the admin root page; similar shape in `infra`, `system`)
- **Settings form** — page header, card, field/label/input/select/toggle, save footer, inline result/error banners (`smtp`, and similarly `config`, `traefik/settings`, `plan`, `payments`, `database`)

Four pages fall back to the native browser `confirm()` for destructive actions instead of a real confirmation dialog — inconsistent even with itself, let alone the rest of the app.

## Decisions

### Visual identity: "Soft Monochrome," refined from a reference

Approved through iterative visual review (three rounds of mockups plus a live interaction prototype — see `.superpowers/brainstorm/84350-1790624694/content/` for the retained mockup files). The direction is inspired by a reference video the user provided (a personal-finance dashboard app), adapted for an infra/ops admin tool:

- **Single-hue tonal family, not scattered accents.** Status and brand surfaces use Shipyard's existing `--accent` blue and its established `-muted` variants (`--accent-muted`, `--accent-green(-muted)`, `--accent-red(-muted)`, `--accent-yellow(-muted)`) — no new hex colors invented. Category/type tagging (e.g. an org's avatar color) may draw from a small fixed palette of additional soft tones, but semantic status (running/stopped/failed) always uses the existing green/red/yellow tokens.
- **Radius:** reuse the *existing* `layout.css` scale exactly as already defined — `--radius-sm` (4px) for inputs/chips/badges, `--radius-md` (6px) for buttons, `--radius-lg` (10px) for cards/panels/tables. No new radius tokens. (An earlier mockup round used 16–24px "bubble" radii; user feedback was explicit that this reads too soft/AI-generated — tightening to the existing scale doubles as the fix.)
- **Typography:** Inter (sans) + JetBrains Mono (mono) — these are already `--font-sans`/`--font-mono` in `layout.css`. No new font is introduced; admin retires its own `system-ui` font token in favor of the existing one. (Confirmed after a live side-by-side against IBM Plex, Sora+Inter mixed pairing, and Manrope — Inter+JetBrains Mono won on its own merits, not just as the path of least resistance.)
- **Circular icon badges**, not square icon boxes, for stat-card icons, activity-row leading icons, and avatars — background uses the relevant `-muted` token, icon color the solid token (e.g. `background: var(--accent-muted); color: var(--accent);`).
- **Flat activity-list rows** (icon + title + meta + trailing status/value, separated by hairline dividers, no per-row border box) for simple lists — alongside a proper dense `DataTable` component for real tabular data (user counts, org lists) that doesn't fit a card/list metaphor.
- **Hero/banner card**: a soft two-stop gradient card for page-level summaries (e.g. "Good morning" / org counts), built from `color-mix(in srgb, var(--accent) N%, var(--bg-surface))` rather than hardcoded hex, so it adapts automatically between light and dark theme.
- **Both theme modes are first-class.** Every new component is verified in both light and dark `layout.css` themes before being considered done — not designed dark-first and back-filled.

### Navigation: new `NavRail` + `NavDrawer`, admin-only for now

Admin's current always-expanded 220px labeled sidebar is replaced by a new, shared nav shell:

- A narrow (~56–60px) **icon rail**, always visible, showing the existing Shipyard anchor logo mark (unchanged — keep as-is, not restyled) plus one icon per top-level nav group.
- Clicking a rail icon opens a **drawer** (~230px) that overlays the content area (does not push/resize it) showing that group's full labeled sub-items, mirroring admin's existing three nav groups (Platform / Infrastructure / Services) verbatim — no information-architecture changes, just a new shell around the same links.
- The drawer stays open until dismissed (click elsewhere, click the same icon again, or navigate to an item) — validated live in an interactive prototype before being approved.
- This is designed as **one new shared component**, but this plan applies it to **admin only**. The main app's `IconSidebar`/`ContextPanel` pair serves a different job (org switching, project list — not grouped settings navigation) and keeps its current two-column shape for now. Retrofitting the main app onto the same rail+drawer shell is an explicit follow-up, not part of this plan — flagged so it isn't silently forgotten, not because it's undesirable.

### Component inventory

New shared components, under `frontend/src/lib/components/ui/` (new directory — keeps the design-system layer visually distinct from existing page-specific components in `$lib/components/` and panel components in `$lib/panels/`):

**Foundational:** `Button` (primary/secondary/ghost/danger/danger-outline variants; sm/md/icon sizes — formalizes the `.btn`/`.btn-*` global classes already documented as convention into a real component), `Badge`, `StatusDot`, `Avatar`, `IconBadge` (circular soft-tint icon container), `Spinner`, `Skeleton` (row/card/text variants), `Tooltip`, `Divider`.

**Forms:** `TextField`, `Select`, `Textarea`, `Toggle`, `Checkbox`, `RadioGroup`, `FormField` (label + hint + error wrapper), `SearchInput` (pill-shaped, icon-prefixed).

**Layout:** `PageHeader` (eyebrow + title + subtitle + actions slot), `HeroCard` (the gradient summary banner), `StatCard`, `Card`/`Panel`, `SectionLabel`, `EmptyState`, `InlineAlert` (error/success/warning/info banner — formalizes the already-documented `.err-msg`/`.ok-msg` convention).

**Data display:** `DataTable` — the highest-value component, consolidating the pattern duplicated across ~12+ pages: search/filter toolbar, sortable columns, row-rendering slot, built-in pagination, loading skeleton, empty state, error state, and a responsive mobile card-list fallback, all in one component instead of hand-rolled per page. See "DataTable: client and server modes" below for its pagination/filtering contract. `ActivityList`/`ListRow` (the flat icon+title+meta+status row). `DonutChart`, `BarChart`, `AreaChart` (lightweight inline SVG, no charting library dependency — matches the reference's data-viz style). `ProgressBar`. `Pagination` (the M3-style control described below, used standalone by `DataTable` internally, but exported in case a page needs it without the rest of the table).

**Overlays:** `Modal`/`ConfirmDialog` (replaces the four pages' native `confirm()` calls and the five pages' hand-rolled modal markup with one component, reusing the confirmation-modal pattern already documented for the main app), `Toast`, `Dropdown`/`SelectMenu` (checkmark-style multi-item list, matching the reference's category filter), `NavRail`/`NavDrawer` (above).

`SlidePanel` (`PanelContainer` + `uiStore.pushPanel()`) already exists and is kept as-is — it's formalized as part of this same system, not rebuilt.

### Component behavior follows Material 3 — as a rulebook, not a reskin

Every component's **structure, states, and interaction patterns** follow Material Design 3's component specifications. This does **not** reopen the colors/radius/font decisions above — those are already settled (Shipyard blue, the existing 4/6/10px radius scale, Inter+JetBrains Mono, non-pill button shapes). M3 governs *how a component is built and behaves*, not *what it's colored like*:

- **State layers.** Every interactive element (button, list/table row, nav item, chip, menu item) gets a defined hover/focus/pressed/disabled treatment via a semi-transparent overlay on top of its base color, per M3's state-layer opacities (hover ≈8%, focus/pressed ≈12%, disabled content at 38% opacity) — implemented once as a shared CSS pattern (e.g. a `.state-layer` mixin or shared class) rather than each component inventing its own hover/active shades.
- **Type scale.** Adopt M3's *scale structure* (a fixed hierarchy of named roles — display/headline/title/body/label, each in large/medium/small) mapped onto Inter, rather than the ad hoc font-sizes scattered across the 32 pages today (each hand-rolled page currently invents its own heading/label sizes). Exact size/weight/line-height values for each role are decided during implementation, referencing M3's scale proportions but tuned to Inter's metrics and Shipyard's existing base sizes (12-14px body, matching current density) rather than M3's own (larger, more consumer-app-scaled) defaults.
- **Elevation.** Map component elevation needs (resting card vs. raised/dragged card vs. modal vs. menu) onto the *existing* `--shadow-sm/md/lg` tokens by role, following M3's concept of discrete elevation levels — no new shadow tokens invented.
- **Component anatomy & accessibility.** Follow M3's documented anatomy for each component type (e.g. a `Toggle`'s track/thumb/state relationship, a `Chip`'s leading-icon/label/trailing-icon slots, a `Modal`'s scrim/container/action-row structure) and its accessibility guidance (roles, keyboard interaction, focus management) as the baseline, adjusted only where it would conflict with an already-approved visual decision.

### `DataTable`: client and server modes, M3-style pagination

The reference for `DataTable`'s pagination control is Material 3's data table footer pattern: a **rows-per-page selector**, an **"X–Y of Z" range label**, and **first/previous/next/last** icon-button navigation, right-aligned in a footer row below the table body. This replaces every ad hoc "Prev / Page N of M / Next" pager currently hand-rolled per page.

Functionally, `DataTable` supports two modes, chosen by which prop the consumer passes:

- **Client mode** (`items: T[]`): the component owns filtering/sorting/pagination over an already-fetched, complete array in memory. Appropriate for naturally small/bounded datasets — a single host's Docker containers/images/volumes/networks, MQTT clients/topics/subscriptions — where fetching "everything" is already what today's pages do and is not a scaling concern.
- **Server mode** (`fetchPage: (params: { page, pageSize, search, sort? }) => Promise<{ rows: T[], total: number }>`): the component calls back into the consumer for each page/filter/sort change instead of slicing a local array, and drives the M3 footer's "X–Y of Z" / last-page math from the returned `total`.

**What this means per page, checked against the actual backend endpoints (corrected from an earlier draft of this spec, which mis-cited which ones already paginate):**

- `GET /admin/projects` (page/limit/q) and `GET /admin/deployments/app` (page/limit) already match the server-mode contract exactly — their pages wire to server mode with no backend change.
- `GET /admin/orgs` and `GET /admin/users` currently return the *entire* unbounded result set with no pagination params at all. This plan does **not** add backend pagination to them — that's a real scalability improvement but backend work orthogonal to a presentation migration, and self-hosted Shipyard's realistic org/user counts (dozens to low thousands) make client-mode fine in practice. Their pages use `DataTable` in **client mode** — functionally identical to today (fetch everything), just through the shared component instead of hand-rolled markup. Adding real backend pagination to these two is a good follow-up, not part of this plan.
- `GET /admin/audit-logs` is **cursor-paginated** (`cursor`/`limit`/`org_id`, no `page`, no `total`) — a fundamentally different shape than page-number pagination, and one the M3 first/prev/next/last-with-total footer can't represent without a backend rewrite (no way to compute "X of Z" or jump to "last" from a cursor alone). Forcing this into `DataTable`'s server mode is out of scope. The audit log page instead uses `ActivityList` (it's a flat chronological feed anyway, a better semantic fit than a data grid) with a restyled "Load more" control driven by the existing cursor — no backend change, no M3 pagination footer on this one page, and that's fine because it's a legitimately different UI, not the table-of-records pattern.
- Every other list endpoint (staff, payments, storage, nodes, swarm nodes, plans, core services, provisioning) also has no pagination today and follows the orgs/users precedent: `DataTable` in client mode, no backend change, same data-fetching behavior as today.

Loading skeleton, empty state, and error state are identical in both `DataTable` modes from the consumer's perspective — the component shows them automatically based on whether `items`/`fetchPage` has resolved data, is pending, or errored.

### Icons: `@lucide/svelte` only, never inline `<svg>`

`@lucide/svelte` (already a dependency, already used by `IconSidebar`/`ContextPanel`/panels elsewhere in the app) is the single icon source for the whole design system — including the Shipyard brand mark itself, which is already `<Anchor>` from lucide in `IconSidebar.svelte` today (confirmed no separate logo asset exists to migrate).

Every one of admin's 32 pages currently inlines raw `<svg><path d="…"/></svg>` markup directly in its template (confirmed in `admin/+layout.svelte`'s nav icons and every page's stat-card/toolbar icons) — this is deleted entirely during migration, replaced by the matching `@lucide/svelte` icon. A quick survey confirms every admin icon in use today (home, building/org, users, staff/shield, folder/project, rocket/deployment, server/compute, cpu/system, docker whale-equivalent, layers, package/registry, hard-drive/storage, radio/mqtt, globe/static, mail/smtp, database, clipboard/audit, credit-card/plan, banknote/payment, refresh/update, sliders/config) has a direct or close `@lucide/svelte` equivalent — no bespoke icon files are expected to be necessary. On the rare occasion one genuinely is (no reasonable lucide equivalent exists), it goes in its own file at `frontend/src/lib/components/ui/icons/<Name>.svelte` — raw `<svg>` markup is never written inline inside a page or component template.

### Token changes

No new color, radius, or font tokens are introduced — the whole point is that admin adopts the tokens `layout.css` already defines. The only additions are structural: a documented gradient recipe for `HeroCard` (`color-mix`-based, not hardcoded) and any spacing tokens the component library needs that `layout.css` doesn't yet define (to be identified during implementation, not guessed here).

`admin/+layout.svelte`'s entire `.shell` custom-property block is deleted. Admin's own dark/light toggle (`localStorage['shipyard-admin-theme']`) is retired in favor of the main app's existing toggle mechanism (`localStorage['shipyard_theme']`, set via `document.documentElement.dataset.theme`) — one theme state for the whole app, not two.

### Migration scope

All 32 admin pages are migrated in this one plan (explicit user choice — the alternative of a foundation-first, one-page-per-pattern proof followed by a smaller follow-up plan was offered and declined). The plan this spec hands off to will therefore be large; it covers, in rough dependency order:

1. Token/theme unification in `admin/+layout.svelte` (delete `.shell`, wire the existing tokens, retire the second theme toggle).
2. Build `NavRail`/`NavDrawer` and re-platform admin's layout shell onto it (same nav groups/links, new shell).
3. Build the full component library listed above.
4. Migrate all 32 admin pages, page by page, onto the new components — the data-table family (~12+ pages), the dashboard family (~3 pages), the settings-form family (~6-8 pages), and the remaining one-off pages.

## Testing

No frontend component test framework exists in this codebase today (no Vitest/Testing Library config found) — introducing one is out of scope for this project. Verification is:

- `npm run check` (svelte-check) after every task, same as existing convention.
- `npm run build` (production build) at the end of the plan and after any task touching shared components, to catch build-time regressions across all 32 pages at once.
- Manual visual verification in a real browser for every new component (both light and dark theme) and every migrated page, since this is a pure UI/presentation project — no backend or business-logic changes are in scope, so functional correctness of migrated pages means "renders the same data with the same interactions as before, in the new visual language," not new test coverage.
