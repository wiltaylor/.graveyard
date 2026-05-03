/*
Skip gates:
- Auth seed: WEB_TEST_USER/WEB_TEST_PASS, documented in docs/mobile.md
- Desktop control has no T1-T5 skip gate and should pass on main.
*/
import { expect, test } from "@playwright/test";

import {
  cataloguePages,
  expectNoHorizontalOverflow,
  isDesktop,
  skipWithoutAuth,
} from "./_fixtures/viewports";

test.describe("desktop control layout", () => {
  test.beforeEach(() => {
    skipWithoutAuth();
  });

  for (const cataloguePage of cataloguePages) {
    test(`${cataloguePage.name} keeps the desktop shell stable`, async ({
      page,
    }, testInfo) => {
      test.skip(!isDesktop(testInfo), "desktop control runs only in desktop-1280");

      await cataloguePage.open(page);
      await expect(page.locator(".fsidebar")).toBeInViewport();
      await expect(page.locator(".ftopbar-hamburger")).toBeHidden();
      await expectNoHorizontalOverflow(page);
    });
  }
});
