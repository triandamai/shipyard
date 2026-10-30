---
title: Design system
description: Drydock, the visual language of the Shipyard dashboard — tokens, type, components and the rules that keep them consistent.
---

# Design system

Shipyard's dashboard is built on **Drydock**: graphite and concrete neutrals carry the interface, one signal colour marks what you can act on, and status colours are reserved for state. It is deliberately quiet. The interesting thing on screen should be your services, not the chrome around them.

The source of truth is `frontend/src/routes/layout.css` (tokens) and `frontend/src/lib/components/ui/` (components). This page documents both. If you are building a screen, an extension or a generated UI, follow it.

## Principles

1. **One signal.** `--accent` (orange) means *this is the action* or *this is where you are*. Never use it for decoration, charts or illustration.
2. **Status is state.** Green, red, yellow and blue only describe the state of something — running, failed, pending, informational. They never decorate.
3. **Never colour alone.** Every status also has a shape or a word: a glyph, a label, a badge. Colour-blind users and grayscale screenshots must read the same.
4. **Flat by default.** Surfaces are separated by 1px rules, not shadows. Shadows exist only on things that float: menus, drawers, dialogs.
5. **Machines speak mono.** IBM Plex Mono is for what a machine produced or will read — image refs, ports, hashes, slugs, log lines. Prose is always Plex Sans.

## Colour

All colours are CSS custom properties on `:root`, redefined under `[data-theme="dark"]`. Components reference tokens, never hex values.

### Surfaces

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `--bg-base` | `#ECECE8` | `#0E0F11` | App ground, canvas — "concrete" |
| `--bg-surface` | `#FFFFFF` | `#17191C` | Cards, panels, inputs |
| `--bg-elevated` | `#F7F7F5` | `#131518` | Side panels, table headers, field fills |
| `--bg-hover` | `#E6E6E1` | `#22252A` | Hover state on rows and ghost controls |
| `--border` | `#DCDCD6` | `#2A2D32` | Every 1px rule |
| `--border-control` | `#8A9097` | `#6B7178` | Form field edges (3:1 against surfaces) |

### Navigation rail

The rail is graphite in both themes so navigation reads as one fixed object.

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `--rail-bg` | `#141619` | `#0A0B0C` | Rail background |
| `--rail-fg` | `#8B9097` | `#8B9097` | Idle icon and label |
| `--rail-fg-active` | `#FFFFFF` | `#FFFFFF` | Current destination |
| `--rail-hover-bg` | `#22252A` | `#1B1D21` | Icon tile on hover, rail hairlines |
| `--rail-active-bg` | `#262A30` | `#22252A` | Icon tile of the current destination |

### Signal

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `--accent` | `#C2410C` | `#F26B1D` | Primary action, current location, focus ring |
| `--accent-hover` | `#9A3412` | `#FB8B47` | Hover on primary |
| `--accent-fg` | `#FFFFFF` | `#141619` | Text on an accent fill (graphite in dark: white on orange fails contrast) |
| `--accent-muted` | 9% accent | 14% accent | Selected row, selected segment |

### Status

Each status has a solid, a `-muted` fill for backgrounds and a `-text` for words on that fill.

| Token | Light | Dark | Means |
| --- | --- | --- | --- |
| `--accent-green` | `#15803D` | `#4ADE80` | Running, healthy, succeeded |
| `--accent-red` | `#B91C1C` | `#F87171` | Failed, error, destructive |
| `--accent-yellow` | `#A16207` | `#FBBF24` | Pending, degraded, warning |
| `--accent-blue` | `#1D4ED8` | `#60A5FA` | Informational, deploying |

### Text

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `--text-primary` | `#16181B` | `#ECEDEE` | Headings, values |
| `--text-secondary` | `#41464D` | `#C3C6CA` | Body |
| `--text-muted` | `#5F656C` | `#9097A0` | Labels, meta |
| `--text-dim` | `#8A9097` | `#767C83` | Placeholders, disabled |

### Terminal

Logs, build output and exec sessions sit on a dark surface in **both** themes, so output looks the same wherever it appears.

| Token | Value | Use |
| --- | --- | --- |
| `--terminal-bg` | `#141619` | Console background |
| `--terminal-border` | `#2A2D32` | Rules inside the console |
| `--terminal-fg` | `#C3C6CA` | Output text |
| `--terminal-dim` | `#8B9097` | Timestamps, debug lines, column heads |
| `--terminal-red` | `#F87171` | Error level |
| `--terminal-yellow` | `#FBBF24` | Warn level |
| `--terminal-blue` | `#60A5FA` | Info level, links |
| `--terminal-green` | `#4ADE80` | Live indicator, success |
| `--terminal-orange` | `#F0B07E` | Origin column (service or source) |

## Typography

| Role | Font | Size / weight |
| --- | --- | --- |
| Page title | IBM Plex Sans | 20px / 700, `-0.02em` |
| Eyebrow | IBM Plex Sans | 11px / 700, `--accent` |
| Section title | IBM Plex Sans | 13–14px / 600 |
| Body | IBM Plex Sans | 13px / 400 |
| Label | IBM Plex Sans | 11–12px / 500–600, `--text-muted` |
| Machine output | IBM Plex Mono | 12px / 400 |
| Metric | IBM Plex Mono | 18px / 600, tabular figures |

Fonts are bundled with `@fontsource` rather than fetched from Google Fonts: Shipyard is self-hosted and often runs on networks without outside access.

## Shape, elevation, motion

| Token | Value | Use |
| --- | --- | --- |
| `--radius-sm` | `4px` | Chips, segments inside a group |
| `--radius-md` | `6px` | Buttons, inputs, icon tiles |
| `--radius-lg` | `8px` | Cards, consoles, panels |
| `--radius-xl` | `12px` | Floating layers only |
| `--shadow-md` | — | Dropdowns, popovers |
| `--shadow-lg` | — | Drawers, dialogs |
| `--transition-fast` | `120ms` | Hover, colour changes |
| `--transition-normal` | `200ms` | Indicators, drawers |
| `--rail-width` | `64px` | Navigation rail |
| `--slide-panel-width` | `480px` | Right-hand slide panels (`100vw` on phones) |

All motion respects `prefers-reduced-motion`. Focus is always a 2px `--accent` outline with a 2px offset.

## Components

Components live in `$lib/components/ui` and are imported from its index:

```svelte title="Usage"
<script lang="ts">
  import { Button, Badge, SearchInput, Select, Toggle, PageHeader } from '$lib/components/ui';
</script>
```

### Buttons

`<Button variant size>` with variants `primary`, `secondary`, `ghost`, `danger`, `danger-outline` and sizes `sm` (30px), `md` (36px), `icon` (32px square). Hover and press are a `currentColor` overlay at 8% / 12%, so every variant gets the same feedback. One primary button per view.

<!-- specimen -->
<div class="ds-specimen"><div class="ds-row">
<span class="ds-btn ds-btn--primary">Deploy</span>
<span class="ds-btn ds-btn--secondary">Redeploy</span>
<span class="ds-btn ds-btn--ghost">Cancel</span>
<span class="ds-btn ds-btn--danger-outline">Delete service</span>
</div></div>
<!-- /specimen -->

### Badges and status

`<Badge tone>` for a state word (`green`, `red`, `yellow`, `blue`, `neutral`); `<StatusDot>` for a compact live state next to a name. Pair a dot with text whenever there is room.

<!-- specimen -->
<div class="ds-specimen"><div class="ds-row">
<span class="ds-badge ds-badge--green">Running</span>
<span class="ds-badge ds-badge--red">Failed</span>
<span class="ds-badge ds-badge--yellow">Pending</span>
<span class="ds-badge ds-badge--blue">Deploying</span>
<span class="ds-badge ds-badge--neutral">Stopped</span>
</div></div>
<!-- /specimen -->

### Segmented control

Filters with a handful of mutually exclusive options (time range, log level, source) use a segmented group: one surface with a 1px border, the selected segment filled with `--accent-muted` and labelled in `--accent`. Levels add a glyph so the state is not carried by colour alone: ◆ error, ■ warn, ● info, ○ debug.

<!-- specimen -->
<div class="ds-specimen"><div class="ds-row">
<span class="ds-seg"><span>1h</span><span>6h</span><span class="on">24h</span><span>7d</span><span>30d</span></span>
<span class="ds-seg"><span class="on">All</span><span><i class="g g-err"></i>Error</span><span><i class="g g-warn"></i>Warn</span><span><i class="g g-info"></i>Info</span><span><i class="g g-debug"></i>Debug</span></span>
</div></div>
<!-- /specimen -->

### Navigation rail

The rail is the app's primary navigation: a 64px graphite column with the brand mark, destinations and account controls. Each destination is **one** target containing its icon tile and label.

The current destination gets three cues together:

- a 3px `--accent` bar flush with the rail's leading edge, 22px tall and rounded on its outer side — it sits outside the icon tile so the tile's radius never clips it;
- the icon tile filled with `--rail-active-bg` and the icon stroke thickened;
- the label in `--rail-fg-active` at weight 600.

Hovering an idle destination previews the bar as an 8px neutral tick. On phones the rail becomes a 56px bottom bar and the indicator moves to the top edge.

<!-- specimen -->
<div class="ds-specimen ds-specimen--rail"><div class="ds-rail">
<span class="ds-rail-logo"></span>
<span class="ds-rail-item"><span class="tile"><i class="ico"></i></span>Home</span>
<span class="ds-rail-item"><span class="tile"><i class="ico"></i></span>Projects</span>
<span class="ds-rail-item on"><span class="tile"><i class="ico"></i></span>Logs</span>
<span class="ds-rail-item"><span class="tile"><i class="ico"></i></span>Settings</span>
</div></div>
<!-- /specimen -->

### Panels and drawers

- **Nav drawer** — opens beside the rail for a group's sub-pages (admin) or the project list (tenant). Active item: `--accent-muted` fill, `--accent` label.
- **Slide panel** — detail views for one resource open from the right at `--slide-panel-width`, stacked through `uiStore.pushPanel()`. Actions inside a detail panel push a sub-panel; they never inline a form.
- **Dialog** — confirmation for destructive actions, always requiring the resource name to be typed.

### Log console

Runtime logs use the terminal surface. Columns are time, level, origin and message; rows are 12px mono at 1.55 line height. Error and warn rows carry a 2px leading bar in their level colour; the selected row uses the signal bar instead. Masked secrets render as a dim `••• [kind]` chip. Selecting a row opens a detail pane beside the console with the full message, metadata and structured fields.

<!-- specimen -->
<div class="ds-specimen ds-specimen--flush"><div class="ds-log">
<div class="ds-log-row"><span class="t">14:02:11.204</span><span class="l info">INF</span><span class="o">api.1</span><span>GET /healthz 200 1ms</span></div>
<div class="ds-log-row warn"><span class="t">14:02:12.870</span><span class="l warn">WRN</span><span class="o">worker.2</span><span>retrying job 812 (attempt 2/5)</span></div>
<div class="ds-log-row err"><span class="t">14:02:13.019</span><span class="l err">ERR</span><span class="o">api.1</span><span>db connect failed: password <span class="m">••• [secret]</span> rejected</span></div>
<div class="ds-log-row"><span class="t">14:02:14.551</span><span class="l dbg">DBG</span><span class="o">api.1</span><span>pool size 8, idle 6</span></div>
</div></div>
<!-- /specimen -->

## Page layout

- Tenant pages: rail → project drawer (collapsible) → page content on `--bg-base`, padded `20px 28px`.
- Admin pages: rail → modal nav drawer → page content, padded `24px 28px`.
- Every page opens with `<PageHeader eyebrow title subtitle actions>`; the eyebrow names the area ("Observability"), the title names the page.
- Content width follows the content: tables and consoles go full width, forms cap at about 640px.

## Do and don't

| Do | Don't |
| --- | --- |
| Reference tokens: `var(--accent-red)` | Hard-code hex values (except inside `color-mix()`) |
| Use `<Button>` and the shared `ui` components | Re-style buttons per component |
| Put icon and label in one clickable target | Make only the icon clickable |
| Mark the current place with the signal bar + filled tile | Use a coloured background alone, or an inset shadow clipped by a radius |
| Use mono for hashes, ports, image refs, logs | Use mono for sentences |
| Separate surfaces with 1px `--border` rules | Add shadows to cards that do not float |
| Use `class={cond ? 'a b' : 'a'}` on Svelte components | Use `class:name` directives on components |
