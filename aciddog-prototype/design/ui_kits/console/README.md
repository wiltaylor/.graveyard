# Forge Console — UI Kit

A click-thru recreation of a generic technical-tools console: dashboard + services list + logs + settings. Built with React (interchangeable with SolidJS — the JSX is plain).

## Files

| File | What it is |
| --- | --- |
| `index.html` | The full prototype — sidebar + topbar + 4 routes wired up. |
| `App.jsx` | Top-level router/state. |
| `Sidebar.jsx` | Left nav (240/56px collapsible). |
| `TopBar.jsx` | App header with breadcrumbs, search, theme toggle, user menu. |
| `Dashboard.jsx` | Metric tiles + recent deploys. |
| `Services.jsx` | Sortable status table. |
| `Logs.jsx` | Streaming log viewer (mocked). |
| `Settings.jsx` | Settings form with sections. |
| `ui.jsx` | Shared primitives — Button, Input, Badge, Card, Icon, etc. |

## How to run

Open `index.html` directly. It pulls React + Babel + Forge tokens from `../../colors_and_type.css`.

## What this is and isn't

- **Is**: a high-fidelity visual reference for buttons, tables, forms, layout, navigation, empty states, theme switching, focus rings, density.
- **Isn't**: production code. State is fake. Routing is local. The log stream is mocked.
