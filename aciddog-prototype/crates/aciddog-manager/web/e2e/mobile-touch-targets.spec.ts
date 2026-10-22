/*
Skip gates:
- Topbar collapse: c69c1499-3328-433d-b878-ba075d7050d2
- Touch target and iOS font-size contract: 48d16c8a-fc51-4c44-8915-3b4dfa1d2a67
- Auth seed: WEB_TEST_USER/WEB_TEST_PASS, documented in docs/mobile.md
*/
import { expect, test } from "@playwright/test";

import {
  gates,
  isMobile,
  openFirstInstance,
  skipWithoutAuth,
  viewportProjects,
} from "./_fixtures/viewports";

const touchTargetSelectors = [".tab", ".ftopbar-icon-btn", ".fbtn", ".tn"];

test.describe("mobile touch targets", () => {
  test.beforeEach(() => {
    skipWithoutAuth();
  });

  for (const selector of touchTargetSelectors) {
    test(`${selector} visible targets are at least 44px tall`, async ({
      page,
    }, testInfo) => {
      test.skip(gates.touchTargets.skip, gates.touchTargets.reason);
      test.skip(!isMobile(testInfo), "44px touch target contract is mobile-only in v1");
      await openFirstInstance(page);

      const failures = await page.locator(selector).evaluateAll((elements) =>
        elements
          .filter((el) => {
            const rect = el.getBoundingClientRect();
            const styles = window.getComputedStyle(el);
            return rect.width > 0 && rect.height > 0 && styles.visibility !== "hidden";
          })
          .map((el) => ({
            text: el.textContent?.trim() ?? "",
            height: el.getBoundingClientRect().height,
          }))
          .filter((row) => row.height < 44),
      );

      expect(failures).toEqual([]);
    });
  }

  test("webkit mobile search input prevents iOS zoom", async ({ page }, testInfo) => {
    test.skip(gates.touchTargets.skip, gates.touchTargets.reason);
    test.skip(
      testInfo.project.name !== viewportProjects.webkitMobile414,
      "iOS zoom-prevention assertion runs in the webkit mobile project",
    );
    await openFirstInstance(page);

    const fontSize = await page.locator(".ftopbar-search input").evaluate((input) =>
      Number.parseFloat(window.getComputedStyle(input).fontSize),
    );
    expect(fontSize).toBeGreaterThanOrEqual(16);
  });
});
