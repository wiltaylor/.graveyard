---
name: forge-design
description: Use this skill to generate well-branded interfaces and assets for Forge, either for production or throwaway prototypes/mocks/etc. Contains essential design guidelines, colors, type, fonts, assets, and UI kit components for prototyping technical-tools UIs (developer dashboards, observability, infra consoles, admin panels). Dark by default, light theme via OS preference.
user-invocable: true
---

Read the README.md file within this skill, and explore the other available files.

If creating visual artifacts (slides, mocks, throwaway prototypes, etc), copy assets out and create static HTML files for the user to view. If working on production code, you can copy assets and read the rules here to become an expert in designing with this brand.

If the user invokes this skill without any other guidance, ask them what they want to build or design, ask some questions, and act as an expert designer who outputs HTML artifacts _or_ production code, depending on the need.

## Quick map of this skill

- `README.md` — brand, content fundamentals, visual foundations, iconography. **Read first.**
- `colors_and_type.css` — single source of truth for tokens. Drop into any HTML doc with `<link rel="stylesheet" href="colors_and_type.css">`.
- `assets/` — logo SVGs and a starter set of Lucide icons (currentColor, 1.5px stroke).
- `preview/` — small specimen cards, useful as visual reference when picking treatments.
- `ui_kits/console/` — high-fidelity recreation of a tools console (dashboard, services, logs, settings). The CSS in `console.css` defines `.fbtn`, `.fcard`, `.ftable`, `.fbadge`, etc. — reuse these directly in mocks.

## Defaults to follow without asking

- **Dark mode is the default.** Honor `prefers-color-scheme` for light. Never light-only.
- **Stack**: SolidJS + Tailwind. Tokens are CSS vars — wire them through Tailwind `theme.extend` or use `var(--token)` directly in `class:` style strings.
- **Type**: IBM Plex Sans + JetBrains Mono. Sentence case everywhere. No display face.
- **Icons**: Lucide. 1.5px stroke. `currentColor`. Never emoji.
- **Density**: 32px row height, 14px body. This is a tools system, not a marketing site.
- **Motion**: 80/160/240ms ease-out. No spring. Honor reduced-motion.
- **Borders carry structure.** Shadows are reserved for popovers and modals.

## When in doubt

Reach for `ui_kits/console/` first — there is almost certainly a component there that matches what you need. Copy + simplify rather than re-derive.
