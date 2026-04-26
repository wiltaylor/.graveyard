# Forge Design System

A design system for **technical tools and developer-facing web UIs**. Forge prioritizes **legibility, density, and calm** — the kind of system you can stare at for eight hours without fatigue.

## Context

This system was designed from first principles for a generic "technical tools" product surface: dashboards, observability, CI/CD, infra consoles, admin panels, internal tools. It is not derived from a specific company codebase or Figma file — there were no source materials provided. If you have an existing product to adapt this to, drop screenshots, a Figma link, or a codebase into the project and we'll iterate.

### Stack assumptions
- **SolidJS** for components (the JSX in `ui_kits/` reads as either React or Solid — the JSX is intentionally plain; swap `className` ↔ `class` and you're done)
- **Tailwind CSS** for utilities, with the tokens in `colors_and_type.css` exposed as CSS variables so Tailwind's `theme.extend` can consume them directly
- **No other runtime dependencies** by default. Icons are from [Lucide](https://lucide.dev) (CDN or `lucide-solid`), which is the closest match to the system's stroke aesthetic.

### Theme behavior
- **Dark by default.**
- Honors `prefers-color-scheme` automatically via the `:root` / `@media (prefers-color-scheme: light)` cascade in `colors_and_type.css`.
- A manual override is supported by adding `data-theme="light"` or `data-theme="dark"` to `<html>`, which beats the media query.

## Sources

None provided. This system is original. If you have:
- **A Figma file** — paste the link and I'll re-derive tokens and components from it.
- **A codebase** — import it and I'll align the recreations to real source.
- **Screenshots** — they help for vibe but I'd still prefer code or Figma for fidelity.

---

## Index

| File | What's in it |
| --- | --- |
| `README.md` | This file. Brand overview, content + visual foundations, iconography. |
| `SKILL.md` | Agent Skill manifest — load this to design with Forge. |
| `colors_and_type.css` | All design tokens (color, type, spacing, radii, shadow) as CSS vars + semantic element styles. |
| `fonts/` | Web font files (currently empty — using Google Fonts CDN; see Type below). |
| `assets/` | Logos, icons, and brand imagery. |
| `preview/` | Small HTML cards that populate the Design System tab. |
| `ui_kits/console/` | Recreations of core technical-tool screens (dashboard, logs, settings) as a click-thru prototype. |

---

## CONTENT FUNDAMENTALS

Forge copy is written **for engineers who are mid-task and slightly annoyed**. It respects their time.

### Voice
- **Direct.** Say what happened, what to do, what changed. No hedging.
- **Calm.** Even when something is broken. Panic in the UI compounds panic in the user.
- **Precise.** Use the real noun. "Pod", "deployment", "commit", "span" — not "item" or "thing".
- **Lowercase-friendly.** Sentence case for everything: titles, buttons, menu items, table headers. Title Case feels marketing-y. ALL CAPS is reserved for tiny labels (eyebrows, table column headers in muted tone).

### Person & tense
- **You** for the user. **We** is reserved for the system speaking on behalf of itself ("We couldn't reach the registry"). Avoid "I".
- **Present tense** for state. **Past tense** for history. Never future tense in UI strings — "Will deploy" should just be "Deploy".

### Length
- Buttons: 1–3 words. Verbs. "Deploy", "Roll back", "Invite member".
- Empty states: one sentence + one action. Not a paragraph.
- Errors: what failed → why → what to do. In that order. One line each, max.
- Tooltips: under 60 characters.

### Numbers, units, time
- Always include units. `12 ms`, `4.2 GB`, `3 nodes`. Space between number and unit.
- Use **relative time** in lists ("2m ago", "yesterday") and **absolute time** on hover/detail (`2026-04-26 14:31:08 UTC`).
- Monospace numerals in tables (the type stack uses `font-variant-numeric: tabular-nums`).

### Tone in practice — examples
- ✅ "Deploy failed. The build timed out after 10 min. Retry, or increase the timeout in settings."
- ❌ "Oops! Something went wrong with your deployment. 😬 Don't worry, we're on it!"

- ✅ "No alerts in the last 24h."
- ❌ "Looks like there's nothing here yet! Why not create your first alert?"

- ✅ "3 environments · last deploy 2m ago"
- ❌ "You have 3 environments and your most recent deployment was 2 minutes ago."

### Emoji and decoration
- **No emoji** in product UI. Status is conveyed by color + shape (dot, icon, badge), not emoji.
- **No exclamation marks** outside of celebratory one-time moments (onboarding completion, first deploy). Default to periods or no punctuation.

---

## VISUAL FOUNDATIONS

### The vibe in one sentence
Quiet, dense, slightly cool. Reads like a well-lit terminal. Color is information, not decoration.

### Color
- **Two themes**, dark default. Both are **near-neutral with a slight cool cast** (a hint of blue in the grays, ~5° hue shift). Pure `#000` and pure `#FFF` are avoided — backgrounds are `#0B0D10` (dark) and `#FAFAFA` (light) so the eye relaxes.
- **One accent**: `--accent` is a desaturated blue (`oklch(0.62 0.16 250)`). Used for primary actions, focus rings, selection, active nav. Never used as a large background fill.
- **Semantic colors** are tuned per theme so contrast is held: `--success` (green), `--warning` (amber), `--danger` (red), `--info` (blue). Each has a `-bg` (subtle tint for backgrounds) and `-fg` (text on tint) variant.
- Imagery vibe: **none by default**. This is a tools system. If imagery is used, it's **technical diagrams, screenshots, or monochrome photography** — never stock illustrations. No gradients on photos.

### Type
- **Sans:** Inter — but you said avoid Inter, so we substitute **IBM Plex Sans** as the primary. It's neutral, has good number rendering, and ships from Google Fonts. *Flagged: if you have a brand sans, drop the woff2 into `fonts/` and update `--font-sans`.*
- **Mono:** **JetBrains Mono** — paired with Plex Sans for tone, designed for code, ligatures off by default.
- **Display:** none. Headings use the same sans, just larger and tighter. Technical tools don't need a display face.
- **Scale:** modular, 1.2 ratio, anchored at 14px body. Defined in `colors_and_type.css`.
- **Tabular numerals everywhere** in tables and metrics.

### Spacing
- 4px base unit. Scale: 4, 8, 12, 16, 20, 24, 32, 40, 48, 64.
- Dense by default. Table rows are 32px tall. List items 36px. Buttons 32px (sm), 36px (md), 40px (lg).

### Backgrounds
- **Solid colors only.** No gradient backgrounds. No textures. No hand-drawn illustrations.
- A subtle **1px hairline grid** is allowed on empty states (`--border-subtle`, `repeating-linear-gradient`).
- Cards sit on `--bg-1`; the page background is `--bg-0`. The 2-step elevation comes from the bg shift, not shadows.

### Borders
- Hairlines everywhere: `1px solid var(--border)`. Borders carry the structure — shadows are minimal.
- `--border-subtle` for in-card dividers. `--border` for card outlines. `--border-strong` on hover/focus.

### Elevation
- **No drop shadows.**  Black-on-black shadows vanish in dark mode and adding a colored glow always reads as decorative noise. Hierarchy is communicated through:
  - **Background step** — `bg-1` (resting) → `bg-2` (nested) → `bg-4` (floating popover/menu).
  - **Border weight** — switch from `--border` to `--border-strong` on floating surfaces.
- The `--shadow-sm` / `--shadow-md` tokens still exist as `none` so historical references no-op cleanly.

### Radii
- 4px standard (buttons, inputs, badges).
- 6px for cards.
- 8px for modals and large floating panels.
- Full (pill) only for status dots and avatar containers. Never on buttons.

### Animation
- **Fast, ease-out, no bounce.**
- `--ease-out: cubic-bezier(0.2, 0, 0, 1)`.
- `--dur-1: 80ms` (hover/press), `--dur-2: 160ms` (panels), `--dur-3: 240ms` (modals, route transitions).
- Spring/bounce physics are not used. Tools should feel **immediate**, not playful.
- Reduced-motion: honored. All non-essential transitions become instant.

### States
- **Hover**: bg shifts by one step (`--bg-1` → `--bg-2`), or border goes from `--border` to `--border-strong`. Color does not change.
- **Press**: bg shifts one more step, or `transform: translateY(0.5px)` for buttons. No scale-down.
- **Focus**: 2px `--accent` outline with 2px offset. Always visible (no `:focus-visible`-only — keyboard and mouse users both deserve focus rings on tools).
- **Disabled**: 0.4 opacity, no pointer events. No special bg.
- **Loading**: a thin 1px progress bar at the top of the affected surface, or an inline shimmer on the field. No spinners-as-default.

### Transparency & blur
- **Sparingly.** Used only for:
  - Modal backdrop: `rgb(0 0 0 / 0.5)` with `backdrop-filter: blur(4px)`.
  - Sticky table headers / sidebar headers: `--bg-0` at 80% opacity with `backdrop-filter: blur(8px)`.
- **Never** for cards, popovers, or buttons. Frosted glass is a marketing aesthetic.

### Cards
- `1px solid var(--border)` + `--bg-1` + `6px` radius.
- No shadow.
- Internal padding: 16px or 20px. Header divider is `1px solid var(--border-subtle)`.

### Layout
- **Fixed app shell**: top bar (48px) + left sidebar (240px collapsed to 56px). Both `position: sticky`.
- Content area uses a max-width of 1280px for prose-like views, full-width for tables and dashboards.
- 12-column grid is **not** enforced — content tends to be table-driven or detail-driven.

---

## ICONOGRAPHY

Forge uses **[Lucide](https://lucide.dev)** as the icon system.

### Why Lucide
- 1.5px stroke, geometric, neutral — matches Plex Sans's tone.
- Open-source (ISC), available as `lucide-solid` for SolidJS, or via CDN for HTML.
- Comprehensive coverage for technical UI: `terminal`, `git-branch`, `database`, `cpu`, `activity`, `alert-triangle`, `check`, etc.

### Rules
- **Stroke 1.5px.** All icons. Never mix stroke weights.
- **Size**: 14px (inline with body), 16px (buttons / list items), 20px (section headers), 24px (empty states / hero).
- **Color**: inherits `currentColor`. Never apply brand color to an icon unless it's a status indicator.
- **No emoji.** Anywhere. Status is a colored dot or a Lucide icon.
- **No unicode-as-icon** (no `→` for arrows, no `✓` for checkmarks). Use the icon font.
- A few raw SVGs ship in `assets/icons/` for the most-used set so demos work offline.

### Logo
- Wordmark in `assets/logo-wordmark.svg`. Mark only in `assets/logo-mark.svg`. Both render in `currentColor`, so they adapt to theme automatically.

### Asset summary
- `assets/logo-mark.svg` — the geometric "anvil" mark. Single-color, currentColor.
- `assets/logo-wordmark.svg` — mark + "forge" wordmark in Plex Sans.
- `assets/icons/*.svg` — a starter set of Lucide icons copied locally.

*Flagged substitution:* No source brand was provided, so the Forge mark is original-but-generic and meant to be replaced. Drop your own SVG into `assets/` and update the references in `ui_kits/console/Header.jsx`.
