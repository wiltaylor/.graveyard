/*
Skip gates:
- Drawer/modal stacking: ec979f18-db77-4ee2-b75b-9b1dfa64c62b
- Auth seed: WEB_TEST_USER/WEB_TEST_PASS, documented in docs/mobile.md
*/
import { expect, test } from "@playwright/test";

import { gates, gotoAndAuth, isDesktop, skipWithoutAuth } from "./_fixtures/viewports";

async function openAddInstanceModal(page: import("@playwright/test").Page) {
  await gotoAndAuth(page);
  const hamburger = page.locator(".ftopbar-hamburger");
  if (await hamburger.isVisible()) {
    await hamburger.click();
  }
  await page.getByRole("button", { name: "Add instance" }).click();
  return page.locator('[role="dialog"]').filter({ hasText: "Add instance" }).last();
}

test.describe("mobile modal stacking and dismissal", () => {
  test.beforeEach(() => {
    skipWithoutAuth();
  });

  test("Add Instance modal stacks above the drawer backdrop", async ({ page }, testInfo) => {
    test.skip(gates.drawer.skip, gates.drawer.reason);
    test.skip(isDesktop(testInfo), "modal stacking check targets drawer viewports");

    const dialog = await openAddInstanceModal(page);
    await expect(dialog).toBeVisible();

    const zIndexes = await page.evaluate(() => {
      const modalBackdrop = Array.from(document.querySelectorAll('[role="dialog"]')).find(
        (el) => el.textContent?.includes("Add instance"),
      );
      const sidebar = document.querySelector(".fsidebar");
      return {
        modal: Number.parseInt(window.getComputedStyle(modalBackdrop!).zIndex, 10),
        sidebar: Number.parseInt(window.getComputedStyle(sidebar!).zIndex, 10),
      };
    });

    expect(zIndexes.modal).toBeGreaterThan(zIndexes.sidebar);
  });

  test("Add Instance modal dismisses via close button", async ({ page }) => {
    test.skip(gates.drawer.skip, gates.drawer.reason);
    const dialog = await openAddInstanceModal(page);
    await page.getByRole("button", { name: "Close" }).click();
    await expect(dialog).toBeHidden();
  });

  test("Add Instance modal dismisses via backdrop click", async ({ page }) => {
    test.skip(gates.drawer.skip, gates.drawer.reason);
    const dialog = await openAddInstanceModal(page);
    await page.mouse.click(4, 4);
    await expect(dialog).toBeHidden();
  });

  test("Add Instance modal dismisses via Escape", async ({ page }) => {
    test.skip(gates.drawer.skip, gates.drawer.reason);
    const dialog = await openAddInstanceModal(page);
    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
  });
});
