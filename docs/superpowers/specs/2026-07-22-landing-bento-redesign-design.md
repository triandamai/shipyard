# Landing site bento grid redesign

## Context

The `landing/` SvelteKit app (marketing homepage + docs) currently styles every page with hardcoded hex/px values duplicated across five separate `<style>` blocks (`+page.svelte`, `docs/+page.svelte`, `docs/api`, `docs/edge-functions`, `docs/registry` — ~5,200 lines total). The homepage's Features (12 items) and Tech Stack (6 items) sections are uniform equal-size card grids. This redesign introduces a bento grid visual system — a fitted grid of varied-size cells giving visual hierarchy to featured content — for the homepage's card collections, backed by a shared design-tokens file so colors/spacing/radii stop drifting across files. Docs pages adopt the same tokens for visual cohesion but keep their existing long-form reading layout, since bento grids don't fit prose/reference content.

## 1. Design tokens

New `landing/src/lib/styles/tokens.css`, imported once in `src/routes/+layout.svelte` so it's available globally without a build-system change.

Custom properties:
- **Colors**: `--bg-base` (#0a0a0f), `--bg-raised`, `--bg-hover`, `--text-primary` (#e2e8f0), `--text-dim`, `--accent` (#60a5fa), `--accent-bg`, `--border-subtle` (rgba(255,255,255,0.06)), plus 2-3 secondary accent hues (amber/green/purple) used only for icon-background variety within bento cells — not a palette change, small hue shifts on an otherwise-identical dark base.
- **Spacing scale**: `--space-1` through `--space-8` (4px base unit).
- **Radii**: `--radius-sm` (8px), `--radius-md` (12px), `--radius-lg` (16px).
- **Shadows/transitions**: one hover-lift shadow, one standard transition duration/easing.

All five page files switch their hardcoded values to `var(--token-name)`. This is mechanical (find hardcoded value → replace with matching token) but touches every file's `<style>` block — the bulk of the "whole site" scope is this substitution, not new CSS.

## 2. Homepage bento sections

### Features grid (12 items, currently 4-col uniform)

Reworked to a 4-column CSS Grid with `grid-auto-flow: dense`. Each item in the existing `features` array gets a new `size: 'lg' | 'md' | 'sm'` field:

| Size | Span | Items |
|---|---|---|
| `lg` | 2 cols × 2 rows | Live Topology Canvas (gets a small mock topology graphic/animated dots), Git-based Deployments (gets a tiny `git push → deployed` terminal snippet) |
| `md` | 2 cols × 1 row | Container Orchestration, Automatic HTTPS (small supporting badge/icon treatment) |
| `sm` | 1 col × 1 row | Remaining 8: One-click Rollback, Resource Limits, Role-based Access, API Keys & Webhooks, Docker Compose Import, Audit Logs, Multi-node Swarm, Live Monitoring |

### Tech stack grid (6 items, currently 3-col uniform)

**Rust** becomes the single `lg` featured cell (2-col span — it's the foundation; larger logo + one-line "why Rust" callout). The other 5 (SvelteKit, PostgreSQL, Docker, Traefik, MQTT) stay standard 1×1 cells.

### CSS mechanics

`.features-grid` / `.stack-grid` become `display: grid; grid-template-columns: repeat(4, 1fr); grid-auto-flow: dense; gap: var(--space-4);` (stack grid uses `repeat(3, 1fr)` given fewer items). Card classes get `.card-lg { grid-column: span 2; grid-row: span 2; }`, `.card-md { grid-column: span 2; }`, applied via `class:card-lg={f.size === 'lg'}` etc. in the `{#each}` block.

## 3. Homepage non-bento sections

Hero, the 3-step "how it works" section, the install detail block, CTA band, footer, and nav are **not** converted to bento — they're not card collections. The 3 steps specifically stay equal-weight cards on purpose: bento hierarchy implies unequal importance, and a numbered sequential process (step 1/2/3) has none — all three are equally necessary. These sections get the token-based restyle only (colors/spacing/radii from `tokens.css`) plus minor polish (matching border/hover treatment on the install-card and terminal-block) so the page reads as one cohesive system rather than "new bento sections bolted onto old design."

## 4. Docs pages

`docs/+page.svelte`, `docs/api/+page.svelte`, `docs/edge-functions/+page.svelte`, `docs/registry/+page.svelte` keep their existing sidebar + scroll-spy + linear-sections structure unchanged — correct shape for long-form reference docs, and forcing bento grids onto prose/tables/code blocks would hurt readability, not help it. They switch to the same `tokens.css` custom properties so borders, code blocks, callout/table styling, and the sidebar nav visually match the homepage (same accent blue, same card border-radius language). No grid restructuring, no new components — a token substitution pass identical in kind to what the homepage gets.

## 5. Responsive behavior

Bento spans collapse via media queries:
- Below ~1024px: 4-col → 2-col. `lg`/`md` cells shrink to full 2-col width; `sm` cells pair up two-per-row.
- Below ~640px: everything becomes a single column, all cells equal width. Bento hierarchy only reads correctly with room to breathe — on mobile it naturally simplifies to a normal stacked list, which is correct behavior, not a compromise.

## Verification

- `npm run dev` in `landing/`, visually check both bento grids (Features, Tech Stack) at desktop (~1280px+), tablet (~768-1024px), and mobile (~375-640px) widths.
- Confirm docs pages render identically in structure with only visual (token) differences — no layout regressions in the sidebar/scroll-spy behavior.
- `npm run check` (svelte-check) and `npm run build` in `landing/` to confirm the static build isn't broken.
- Manual spot-check: install-card copy button, docs sidebar active-section highlighting, and mobile menu toggle still work (these are existing interactive behaviors that must survive the CSS-focused redesign untouched).
