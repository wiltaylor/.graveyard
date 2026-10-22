/*
Skip gates:
- Breakpoint contract: 40aa10ce-86b7-4e17-8c0c-1fd9bfacc1c6
- Drawer behavior: ec979f18-db77-4ee2-b75b-9b1dfa64c62b
- Auth seed: WEB_TEST_USER/WEB_TEST_PASS, documented in docs/mobile.md
*/
import { expect, test } from "@playwright/test";

import {
  cataloguePages,
  expectNoHorizontalOverflow,
  gates,
  gotoAndAuth,
  isDesktop,
  isDrawerViewport,
  skipWithoutAuth,
} from "./_fixtures/viewports";

test.describe("mobile shell viewport smoke", () => {
  test.beforeEach(() => {
    skipWithoutAuth();
  });

  for (const cataloguePage of cataloguePages) {
    test(`${cataloguePage.name} has no horizontal page overflow`, async ({
      page,
    }) => {
      test.skip(gates.appShellBreakpoints.skip, gates.appShellBreakpoints.reason);
      await cataloguePage.open(page);
      await expectNoHorizontalOverflow(page);
    });
  }

  test("hamburger and drawer state follow the breakpoint contract", async ({
    page,
  }, testInfo) => {
    test.skip(gates.drawer.skip, gates.drawer.reason);
    await gotoAndAuth(page);

    const hamburger = page.locator(".ftopbar-hamburger");
    const sidebar = page.locator(".fsidebar");

    if (isDesktop(testInfo)) {
      await expect(hamburger).toBeHidden();
      await expect(sidebar).toBeInViewport();
      return;
    }

    test.skip(!isDrawerViewport(testInfo), "drawer assertion only runs at <=768");
    await expect(hamburger).toBeVisible();
    await expect(sidebar).not.toBeInViewport();

    await hamburger.click();
    await expect(sidebar).toBeInViewport();

    await page.locator(".fsidebar-drawer-backdrop").click({ position: { x: 2, y: 2 } });
    await expect(sidebar).not.toBeInViewport();
  });
});
