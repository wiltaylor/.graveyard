# Mobile Review

## Breakpoint Contract

The manager web UI uses the mobile gate at `max-width: 768px`. T1 owns the shared breakpoint tokens:

| Token | Width | Use |
| --- | ---: | --- |
| `--bp-sm` | 375px | Small phone smoke baseline. |
| `--bp-md` | 768px | Drawer, compact topbar, and icon-only tab gate. |

The Playwright matrix covers 375, 414, 768, and 1280px. Chromium runs the full matrix; WebKit runs the 414px mobile project for the iOS input zoom-prevention check.

## Breakpoint Affordances

| Project | Width | Expected navigation | Tabs | Touch targets |
| --- | ---: | --- | --- | --- |
| `mobile-375` | 375px | Hamburger visible; sidebar starts off-canvas; backdrop click closes it. | Icon-only labels; active tab scrolls into view. | Visible `.tab`, `.ftopbar-icon-btn`, `.fbtn`, and `.tn` targets are at least 44px tall. |
| `mobile-414` | 414px | Same as 375px. | Same as 375px. | Same as 375px. |
| `webkit-mobile-414` | 414px | Same as 414px Chromium. | Same as 414px Chromium. | `.ftopbar-search input` font-size is at least 16px. |
| `tablet-768` | 768px | Hamburger visible; sidebar drawer behavior still applies. | Icon-only labels at the mobile gate. | Mobile-only 44px assertions are skipped in v1 unless T5 widens the contract. |
| `desktop-1280` | 1280px | Sidebar visible by default; hamburger hidden. | Full labels. | Desktop control only checks no horizontal overflow. |

## Automated Smoke

Run the fast frontend suite with:

```sh
just manager-frontend-test
```

Run the opt-in browser smoke suite with:

```sh
export WEB_TEST_USER=<admin-username>
export WEB_TEST_PASS=<admin-password>
cargo run --manifest-path crates/aciddog-manager/Cargo.toml -- reset-password --username "$WEB_TEST_USER" --password "$WEB_TEST_PASS"
just manager-frontend-e2e
```

The Playwright global setup logs in with `WEB_TEST_USER` and `WEB_TEST_PASS` and writes `crates/aciddog-manager/web/e2e/.auth/state.json`. If either variable is missing, e2e tests skip with a pointer to this file. The e2e recipe is intentionally not part of `just ci` until browser install/runtime cost is measured.

## Manual Smoke Checklist

Use browser DevTools responsive mode at 375, 414, 768, and 1280px.

1. Start the manager with a seeded admin and sign in.
2. Visit the login screen, instance list, Team, Agents/Chat, Tasks, Workflows, Cron, Settings > Users, and Settings > Skills.
3. At every viewport, confirm `document.documentElement.scrollWidth <= document.documentElement.clientWidth`.
4. At 375, 414, and 768px, confirm the hamburger opens the sidebar drawer and backdrop click closes it.
5. At 1280px, confirm the sidebar is visible by default and the hamburger is hidden.
6. On an instance detail page, click every tab and confirm the active tab remains fully visible in the tab strip.
7. Open Add Instance from the sidebar plus button and dismiss the modal by close button, backdrop click, and Escape.
8. On mobile viewports, inspect visible tabs, icon buttons, form buttons, and tree rows for at least 44px height.

## Skip Gates

| Spec or suite | Gate | Ticket |
| --- | --- | --- |
| `e2e/mobile-shell.spec.ts` | Breakpoint shell/no-overflow assertions | `40aa10ce-86b7-4e17-8c0c-1fd9bfacc1c6` |
| `e2e/mobile-shell.spec.ts` | Drawer open/close behavior | `ec979f18-db77-4ee2-b75b-9b1dfa64c62b` |
| `e2e/mobile-tabs.spec.ts` | Scroll-into-view and icon-only tab behavior | `e48778e1-5142-4910-900f-85f3e2da1b9a` |
| `e2e/mobile-touch-targets.spec.ts` | 44px targets and iOS input font-size | `48d16c8a-fc51-4c44-8915-3b4dfa1d2a67` |
| `e2e/mobile-touch-targets.spec.ts` | Compact topbar search surface | `c69c1499-3328-433d-b878-ba075d7050d2` |
| `e2e/mobile-modal.spec.ts` | Drawer/modal stacking and dismissal | `ec979f18-db77-4ee2-b75b-9b1dfa64c62b` |
| `e2e/desktop-control.spec.ts` | Auth seed only; no T1-T5 behavior gate | `WEB_TEST_USER` / `WEB_TEST_PASS` |
| `src/__tests__/mobile-drawer.test.tsx` | Drawer ARIA and focus restoration | `ec979f18-db77-4ee2-b75b-9b1dfa64c62b` |
| `src/__tests__/tabs-aria.test.tsx` | Tab roles and selected state | `e48778e1-5142-4910-900f-85f3e2da1b9a` |

Follow-up CI ticket: add `manager-frontend-e2e` to `just ci` after a seeded Playwright run captures wall-clock cost for this matrix.
