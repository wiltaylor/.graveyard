# aciddog manager — Web UI inventory (handoff for redesign)

## 1. Purpose

This document is a **current-state inventory** of the aciddog manager Web UI. It exists so a UI design pass can read it and propose a redesign without having to spelunk the source first.

It describes:

- the global shell, navigation, and tabs,
- every view / page / pane that the user can land on,
- the shared visual primitives (alerts, badges, modals, empty states) that recur,
- the live-update channels that drive real-time UI state.

It deliberately does **not** prescribe an aesthetic direction or list pain points — pick whatever direction makes the product look better, as long as the constraints in §2 still hold.

Unless otherwise noted, all source paths in this doc are relative to `crates/aciddog-manager/web/` (the SPA lives entirely under that directory).

## 2. Hard constraints (must be preserved by any redesign)

- **Tech stack stays:** Solid.js + TypeScript + Vite + Tailwind CSS 3.4. No framework swap, no CSS-in-JS, no styled-components. Tailwind utility classes in JSX only — `src/styles.css` contains nothing but the three `@tailwind` directives.
- **Single-page app, no client-side router.** Active view is in-memory state inside `AppShell.tsx`. URLs do not change as the user moves between tabs.
- **Auth gate:** unauthenticated users see only the login page; the app shell and all tabs require a session cookie.
- **Live updates via Server-Sent Events** on Workflows / Cron / Chat / Channel / Inbox surfaces (see §8). The redesign must accommodate connection-status feedback and `lagged` recovery affordances.
- **Existing rendering libraries stay:** Cytoscape (Memory graph), Marked + DOMPurify (markdown rendering for chat blocks).
- **Backend contract is fixed.** All API endpoints under `/api/*` and the SSE streams already exist; the redesign reshapes presentation, not data.

## 3. Global shell & layout

Source: `src/AppShell.tsx`.

Top-level CSS grid:

```
grid-cols-[20rem_1fr] grid-rows-[auto_1fr]
```

- Header spans the full width (top row).
- Sidebar (20 rem) anchors the left column.
- Main pane fills the remaining space.

```
+------------------------------------------------------------+
|  HEADER  aciddog manager · subtitle      user · [Log out]  |
+----------------------+-------------------------------------+
|                      |  TAB BAR                            |
|   SIDEBAR (20rem)    |  Team Agents Tasks Workflows Cron   |
|   - Instances tree   |       Inbox Memory                  |
|   - Settings tree    +-------------------------------------+
|   - Add instance     |                                     |
|     form             |  MAIN PANE (per-tab content)        |
|                      |                                     |
+----------------------+-------------------------------------+
```

### Header
- Brand: `aciddog manager` (h1) + subtitle `Registered aciddog :webservice instances.` (small p).
- Right side: signed-in username and a bordered `Log out` button.
- Background: `bg-slate-900`, border-bottom `border-slate-800`. Implied height ≈ 3.5 rem (chat pane uses `h-[calc(100vh-3.5rem)]`).

### Sidebar (left, 20 rem)
1. **Instances** section — `InstanceTree` (`src/components/InstanceTree.tsx`).
2. **Settings** section — `SettingsTree` (`src/components/SettingsTree.tsx`).
3. **Add instance** form — `AddInstanceForm` (`src/components/AddInstanceForm.tsx`).
4. Scrolls with `overflow-y-auto`.

### Tab bar
- Horizontal row of buttons at the top of the main pane.
- Active tab marked by a `border-b-2 border-sky-500`; inactive tabs darken text on hover.
- Tab order is fixed: **Team, Agents, Tasks, Workflows, Cron, Inbox, Memory** (7 tabs). Defined in `AppShell.tsx:40-46`.
- Tabs are only meaningful once the user has selected an instance from the sidebar; otherwise the main pane shows a "select an instance" placeholder.

### Main pane
Switches between the views in §5 based on `activeTab()` and the current sidebar selection.

## 4. Color & typography conventions

The product is dark-mode only.

### Palette

| Role               | Tailwind tokens used                                              |
|--------------------|-------------------------------------------------------------------|
| Page background    | `slate-950`                                                       |
| Surfaces / cards   | `slate-900`, `slate-900/60`, `slate-900/40`                       |
| Borders / dividers | `slate-800`, `slate-700`                                          |
| Body text          | `slate-100` (primary), `slate-200` (secondary), `slate-400` (muted), `slate-500` (disabled) |
| Accent / interactive | `sky-500` (active tab), `sky-300` (links/IDs), `sky-200`        |
| Success            | `emerald-700/800/900`, `emerald-200`                              |
| Error              | `red-800/900`, `red-200`                                          |
| Warning            | `amber-400/800/900`, `amber-200`                                  |

### Typography

- Headings: `font-semibold`, `text-lg` / `text-xl`.
- Section labels: `text-xs uppercase tracking-wider` (used for "Agents", "Channels" sub-headings in the instance tree, fleet-stat labels on the team view, etc.).
- Body: `text-sm`.
- Monospace: `font-mono` for IDs, transport URLs / socket paths, timestamps, and code-like values.
- System font stack (Tailwind default) — no custom fonts are loaded.

### Iconography

No icon font. Inline glyphs only:

- `▾` / `▸` — collapsed/expanded carets in the instance & settings trees.
- `#` — channel prefix.
- `←` — back button on chat / channel views.
- `✓` / `✗` / `◐` / `↻` / `·` — workflow step status glyphs.

The redesign can introduce a real icon set, but should keep the dark-mode aesthetic and preserve semantic color usage.

## 5. Pages / views

Each entry below specifies: source file, when it appears, layout sections in render order, controls present, live-update channel (if any), and the empty/loading/error states already implemented.

---

### 5.1 Login page

- **Source:** `src/components/LoginPage.tsx`
- **When it appears:** rendered by `App.tsx` whenever `authApi.whoami()` returns no session.
- **Layout (top → bottom):**
  1. Centered card on `slate-950`, `max-w-sm`.
  2. Title `aciddog manager` + subtitle `Sign in to continue`.
  3. Username input (`autocomplete="username"`).
  4. Password input (`autocomplete="current-password"`).
  5. Error alert (red, conditional).
  6. Submit button — text swaps to `Signing in…` while pending, button disabled.

---

### 5.2 Team tab

- **Source:** `src/components/TeamView.tsx`
- **When it appears:** `activeTab === "team"` and an instance is selected. This is the default landing tab when the user picks an instance.
- **Backend:** `GET /api/instances/:id/team` (one-shot; no SSE).
- **Layout (top → bottom):**
  1. Header row: title `Team` + refresh button + small loading hint.
  2. Team-info card: team ID heading, default AI core (if set), description, custom-thinking-messages indicator.
  3. **Fleet stats grid** (5 cells, responsive `2 / 3 / 5` columns): Agents, Workers, Automatons, Channels, Workflows. Each cell = label + count.
  4. **AI Cores list:** one `CoreCard` per core. Each card shows core ID, kind badge (monospace), `team default` badge (if applicable), usage-provider badge, and a property grid: model, credential, default tools (chip list), system-prompt indicator, "used by" agent list.
- **States:** `loading…` text, red error alert, empty section (no AI cores).

---

### 5.3 Agents tab — instance overview

- **Source:** `src/components/InstanceDetail.tsx`
- **When it appears:** `activeTab === "agents"` and no agent/channel chosen from the sidebar.
- **Layout (top → bottom):**
  1. Instance header: name (h2), transport / URL / socket path (small text), red `Delete` button (top-right).
  2. **Health section:** refresh button, HTTP status + body. Color-coded green (`ok`) or amber (`not ok`).
  3. **Agents table:** columns `ID`, `Kind`, `Status`, `Parent`, `AI core`. Rows are clickable — clicking jumps into the chat pane for that agent. Agent IDs render in `sky-300`; row hover darkens the background.
- **States:** `Expand this instance in the tree to load agents.` (initial), red/amber error alert, `Deleting…` button text.

---

### 5.4 Agents tab — chat pane

- **Source:** `src/components/ChatPane.tsx`
- **When it appears:** an agent is selected from the instance tree (and `activeTab === "agents"`).
- **Backend:** `GET /api/instances/:id/agents/:agent/history` + SSE `…/events`.
- **Layout (top → bottom):**
  1. Header: `← back` button, agent name (h2), stream-status badge (`connecting / live / lagged / closed`), refresh button.
  2. Scrollable message area — block list rendered by `BlockView` components. Markdown is rendered with Marked + DOMPurify.
  3. Input form: textarea (rows=2, Enter sends, Shift+Enter newline), send button (disabled when empty or sending).
  4. Error alert (red, conditional).
- **Live updates:** SSE pushes new chat blocks; on `lagged` (with dropped count) the pane refetches history and shows a dropped-count indicator.
- **States:** `(no messages yet)`, `history unavailable`, `Sending…` button text, stream-status colors.

---

### 5.5 Agents tab — channel view

- **Source:** `src/components/ChannelView.tsx`
- **When it appears:** a channel is selected from the instance tree (and `activeTab === "agents"`).
- **Backend:** `GET /api/instances/:id/channels/:channel/history` + SSE `…/events`.
- **Layout:** Same shape as the chat pane, with these additions in the header:
  - Channel name prefixed with `#`.
  - Description (if present).
  - **Member chips** (one chip per member).
- **Live updates:** SSE for channel messages, with `lagged` recovery just like chat.

---

### 5.6 Tasks tab

- **Source:** `src/components/TasksView.tsx`
- **When it appears:** `activeTab === "tasks"` and an instance is selected.
- **Backend:** `GET/POST /api/instances/:id/tasks` and per-task `PATCH/DELETE` plus `notes`, `todos`, `comments` sub-resources. No SSE.
- **Layout (top → bottom):**
  1. Header: `Tasks` title, loading indicator, `+ New task` button, refresh button.
  2. **Kanban board:** 5 columns — **Open / Assigned / In Progress / Done / Cancelled**. Responsive column count. Cards are HTML5-draggable between columns. Each card shows the task title and is clickable to open detail.
  3. **Task detail modal** (when a card is opened): full task fields, notes, todos (each todo with a checkbox + edit + delete), comments thread.
  4. **Add task modal** (when `+ New task` is clicked).
- **States:** loading hint, red error alert, modal open/closed, per-row submitting states.

---

### 5.7 Workflows tab

- **Source:** `src/components/WorkflowsView.tsx` (the `WorkflowDiagram` component lives in the same file, lines 769–969).
- **When it appears:** `activeTab === "workflows"` and an instance is selected.
- **Backend:** `GET /api/instances/:id/workflows/templates`, `GET /workflows/instances`, `POST /workflows/instances`, `DELETE /workflows/instances/:wf_id`, plus SSE `/workflows/events`.
- **Layout:**

```
+--------------------------------------------------------------+
|  Header: "Workflows" · stream-status badge · refresh         |
+--------------------------------------------------------------+
|  Running instances strip (horizontal scroll)                 |
|   [#abc running step3]  [#def failed]  [#ghi succeeded]      |
+----------------+-------------------+-------------------------+
|                |                   |                         |
| TEMPLATES      | RUN FORM          | INSTANCE DETAIL         |
| (left pane)    | (middle pane)     | / TEMPLATE PREVIEW      |
|                |                   | (right pane)            |
| - tmpl id      | - description     |  Status badge           |
|   description  | - property        |  Instance ID + wf ID    |
|   step list    |   inputs (text/   |  Dismiss btn (failed)   |
|   selected =   |   number/select/  |  Failure reason         |
|   sky-500      |   multi-select)   |  WorkflowDiagram (SVG)  |
|   border       | - Run / Starting…|  Variables grid         |
|                | - error alert    |  Output (last 200 lines)|
+----------------+-------------------+-------------------------+
```

- **Templates sidebar (left):** `Templates` heading, list of clickable template buttons. Each shows template ID, 2-line-clamped description, step count and step kinds. Selected template gets a `sky-500` border highlight.
- **Run form (middle):** template description, dynamically-built inputs based on the template's properties (text / number / select / multi-select), `Run` submit button (`Starting…` while pending), error alert.
- **Detail pane (right):** if an instance is selected, shows that instance's runtime detail; otherwise shows a static `Flow preview` of the selected template; otherwise `Pick a workflow…`.
- **Running-instances strip (top):** horizontally-scrollable row of buttons, one per running / failed / succeeded instance, color-coded by status.
- **`WorkflowDiagram` (custom SVG):** vertical flowchart, ~220 px wide × dynamic height. Forward arrows are grey; retry edges are dashed amber. Status colors: pending=slate, running=blue, succeeded=green, failed=red, retrying=amber. Step glyphs: `·` pending, `◐` running, `✓` succeeded, `✗` failed, `↻` retrying. Retry count appears as `×N` badge.
- **Live updates:** SSE events `started / step_started / step_output / var_changed / step_finished / retrying / finished`. On a successful instance, the entry auto-disappears 4 s after `finished`.
- **States:** loading, red error, stream status, `No workflows declared.` empty state.

---

### 5.8 Cron tab

- **Source:** `src/components/CronView.tsx`
- **When it appears:** `activeTab === "cron"` and an instance is selected.
- **Backend:** `GET/POST /api/instances/:id/cron`, per-job `PATCH/DELETE`, plus SSE `/cron/events`.
- **Layout (top → bottom):**
  1. Header: `Cron` title, stream-status badge, refresh button, add-job button.
  2. **Jobs table:** columns `Job ID`, `Target agent`, `Schedule`, `Payload`, `Next run`, `Enabled`, `Actions`. Row actions: `Edit`, `Delete`, enable/disable toggle.
  3. **Create / edit job modal:** target-agent dropdown, payload kind (`message` / `slash_command`), payload body or command text, schedule kind (`recurring` / `one-off`), schedule text (cron expression or local datetime), timezone, "catchup on startup" checkbox, "enabled" checkbox.
  4. **Delete confirmation modal** (red).
- **Live updates:** SSE pushes job state changes. The view also auto-ticks every ~30 s to keep the `Next run` column fresh.

---

### 5.9 Inbox tab

- **Source:** `src/components/InboxView.tsx`
- **When it appears:** `activeTab === "inbox"` and an instance is selected.
- **Backend:** `GET /api/instances/:id/inbox`; `DELETE /inbox/:index`; `POST /inbox/:index/reply`; `POST /inbox/:index/forward`. SSE on `/api/instances/:id/events` is consumed only as a refetch trigger when a `user_message` event arrives.
- **Layout (top → bottom):**
  1. Header: `Inbox` title, stream-status badge.
  2. **Message list:** one row per inbox message, showing sender, timestamp, message preview.
  3. **Row expansion:** clicking a row expands it. Available row modes:
     - **Reply** — inline composer (textarea + send button).
     - **Forward** — inline composer with an additional agent-selector dropdown.
     - **Delete** — removes the message.
- **Live updates:** SSE-driven refetch on new `user_message`.

---

### 5.10 Memory tab

- **Source:** `src/components/MemoryView.tsx`
- **When it appears:** `activeTab === "memory"` and an instance is selected.
- **Backend:** `GET /api/instances/:id/memory/banks`; per-bank `search`, entry CRUD, links CRUD, `graph` data endpoint. No SSE.
- **Layout:**
  1. Header: `Memory` (+ bank name when one is selected), back button (when in bank detail), refresh button.
  2. **Bank list (default):** responsive grid of bank cards (1 col mobile, 2 col md+). Each card = bank ID, description, entry count. Click selects the bank.
  3. **Bank detail (when a bank is selected):** description block, search form, **Cytoscape graph visualization**, entry list/table, add/edit/delete-entry forms.
- **States:** loading, red error, `No memory banks declared on this instance.` empty state.

---

### 5.11 Users settings

- **Source:** `src/components/UsersView.tsx`
- **When it appears:** the user clicks Settings → "Web UI" → "Users" in the sidebar (`SettingsTree.tsx`). Replaces the main pane.
- **Backend:** `GET/POST /api/users`, `DELETE /users/:id`, `POST /users/:id/password`, `POST /auth/password`.
- **Layout (top → bottom):**
  1. Heading: `Web UI users` + subtitle `Accounts that can sign in to this manager portal.`
  2. **Add-user form:** username + password + submit (`Add user` / `Adding…`), error alert.
  3. **Users table:** columns `Username` (with `(you)` label on the current user), `Last login` (formatted date or `never`), `Actions`.
     - Row actions: `Reset password` (toggles into an inline password-reset form) / `Cancel` (collapses the form). `Delete` (red, only shown when not self **and** more than one user exists).
     - Expanded row: inline new-password input + `Set password` button.
  4. **Change-my-password form:** current password, new password, confirm new password (last two side-by-side at md+), submit (`Update password` / `Updating…`), error or success alert.

---

## 6. Sidebar components (deeper detail)

### 6.1 InstanceTree — `src/components/InstanceTree.tsx`
- One row per registered instance; row shows expand caret (`▾`/`▸`), name, transport badge (`unix` / `https`).
- Expanded instance reveals two sub-lists:
  - **Agents** — uppercase `Agents` label; each agent button shows ID + kind + status badges. Click jumps to chat pane.
  - **Channels** — uppercase `Channels` label; each channel button shows `# id` + member count + message count. Click jumps to channel view.
- Selection states: instance background `bg-slate-800`; agent/channel background `bg-slate-700`.
- Empty: `No instances registered yet.` Loading: `Loading…`. Errors render in amber.

### 6.2 SettingsTree — `src/components/SettingsTree.tsx`
- Single tree: `Web UI` parent (collapsible, defaults open) → `Users` child button. Selecting `Users` swaps the main pane to `UsersView`.

### 6.3 AddInstanceForm — `src/components/AddInstanceForm.tsx`
- Fields: name (text), transport (`unix` | `https`).
- Conditional fields by transport:
  - `unix` → socket path.
  - `https` → URL, optional API key (password input), optional TLS CA path.
- Submit (`Add instance` / `Adding…`), error alert.

## 7. Shared UI primitives

The redesign should treat these as the recurring building blocks that show up across many views:

- **Error alert** — `border border-red-800 bg-red-900/40 text-red-200`, padding `px-2 py-1` or `px-3 py-2` depending on density. Used for form errors, API errors, deletion errors.
- **Success alert** — `border border-emerald-800 bg-emerald-900/40 text-emerald-200`. Currently only used for the password-change confirmation.
- **Stream-status badge** — small color-coded chip at the top of any SSE-driven view (Workflows, Cron, Chat, Channel). States: `connecting` (slate/amber), `live` (emerald), `lagged` (amber), `closed` (red). When lagged, an additional dropped-count indicator appears nearby.
- **Loading indicator** — plain `loading…` text in `slate-400`. Buttons swap their label to `Adding…` / `Saving…` / `Sending…` / `Updating…` / `Deleting…` and are disabled with reduced opacity while pending.
- **Modals** — overlay + centered card pattern, used for task add/detail and cron add/edit/delete. No portaling library; standard fixed-position overlay.
- **Empty states** — flat single-line text, e.g. `(no messages yet)`, `No instances registered yet.`, `No workflows declared.`, `No agents reported.`, `No memory banks declared on this instance.`, `No users yet — add one above.`
- **Tag chips** — small pill text used for transport badges in the instance tree, "team default" / kind / provider badges on AI core cards, member chips in channel headers, default-tools chips on cores.

## 8. Live-update channels (SSE)

| Endpoint                                             | Consumer                       | Events / behaviour                                                                                                                                     |
|------------------------------------------------------|--------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------|
| `/api/instances/:id/agents/:agent/events`            | `ChatPane.tsx`                 | Pushes chat block events. `lagged` event includes a dropped-count → triggers history refetch and surfaces the dropped count.                            |
| `/api/instances/:id/channels/:channel/events`        | `ChannelView.tsx`              | Channel message events. `lagged` recovery as above.                                                                                                    |
| `/api/instances/:id/workflows/events`                | `WorkflowsView.tsx`            | `started`, `step_started`, `step_output`, `var_changed`, `step_finished`, `retrying`, `finished`. Successful instances are auto-removed 4 s after finish. |
| `/api/instances/:id/cron/events`                     | `CronView.tsx`                 | Cron-job state changes (created / updated / deleted / enabled / disabled / next-run updated).                                                          |
| `/api/instances/:id/events`                          | `InboxView.tsx`                | Used as a refetch trigger only — when a `user_message` event arrives, the view reloads `/inbox`.                                                      |

Every SSE-backed view shows a connection-status badge in its header (see §7). The redesign needs to keep this affordance, since `lagged` and `closed` states meaningfully change what the user is looking at.

## 9. Source file index

All paths are relative to `crates/aciddog-manager/web/src/`.

| File                              | What it is                                                                  |
|-----------------------------------|-----------------------------------------------------------------------------|
| `index.tsx`                       | App entry, mounts the Solid root into `#root`.                              |
| `App.tsx`                         | Auth gate — chooses between `LoginPage` and `AppShell`.                     |
| `AppShell.tsx`                    | Authenticated shell: header, sidebar, tabs, main pane switcher.             |
| `auth.ts`                         | Auth API client (`login`, `logout`, `whoami`, `changePassword`).            |
| `api.ts`                          | All non-auth API calls (instances, teams, tasks, workflows, cron, …).        |
| `types.ts`                        | Wire types and API response shapes.                                         |
| `errors.ts`                       | Custom error classes (e.g. `UnauthorizedError`).                            |
| `styles.css`                      | Tailwind directives only — no custom CSS.                                   |
| `components/LoginPage.tsx`        | Login form (5.1).                                                           |
| `components/AddInstanceForm.tsx`  | Sidebar add-instance form (6.3).                                            |
| `components/InstanceTree.tsx`     | Sidebar instances tree with agents/channels (6.1).                          |
| `components/SettingsTree.tsx`     | Sidebar settings tree (6.2).                                                |
| `components/InstanceDetail.tsx`   | Agents tab — instance overview / agents table (5.3).                        |
| `components/ChatPane.tsx`         | Agents tab — agent chat (5.4).                                              |
| `components/ChannelView.tsx`      | Agents tab — channel view (5.5).                                            |
| `components/TeamView.tsx`         | Team tab (5.2).                                                             |
| `components/TasksView.tsx`        | Tasks tab — kanban + modals (5.6).                                          |
| `components/WorkflowsView.tsx`    | Workflows tab — three-pane layout, running strip, custom SVG diagram (5.7). |
| `components/CronView.tsx`         | Cron tab (5.8).                                                             |
| `components/InboxView.tsx`        | Inbox tab (5.9).                                                            |
| `components/MemoryView.tsx`       | Memory tab — bank list + Cytoscape graph detail (5.10).                     |
| `components/UsersView.tsx`        | Settings → Users (5.11).                                                    |

Build / config files (touch only if the redesign changes the toolchain):

- `index.html` — single-page entry, declares `<div id="root">` and loads `/src/index.tsx`.
- `vite.config.ts` — Vite + `vite-plugin-solid`.
- `tailwind.config.js` — content paths only, no theme extensions today.
- `postcss.config.js` — Tailwind + Autoprefixer.
- `package.json` — declares Solid.js, Cytoscape, Marked, DOMPurify, TypeScript 5.7.

## 10. Out of scope

The Rust/Axum backend, build tooling beyond naming Vite, and anything outside `crates/aciddog-manager/web/` are explicitly out of scope for this redesign handoff. The API contract (endpoints in §5 and SSE streams in §8) is fixed.
