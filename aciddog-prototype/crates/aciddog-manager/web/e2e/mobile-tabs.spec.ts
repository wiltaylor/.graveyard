/*
Skip gates:
- Tab scroll/icon-only behavior: e48778e1-5142-4910-900f-85f3e2da1b9a
- Auth seed: WEB_TEST_USER/WEB_TEST_PASS, documented in docs/mobile.md
*/
import { expect, test } from "@playwright/test";

import { gates, isMobile, openFirstInstance, skipWithoutAuth } from "./_fixtures/viewports";

const tabs = ["Team", "Agents", "Tasks", "Workflows", "Repos", "Cron", "Inbox", "Memory"];

test.describe("mobile tabs viewport smoke", () => {
  test.beforeEach(() => {
    skipWithoutAuth();
  });

  for (const tabName of tabs) {
    test(`keeps active ${tabName} tab inside the scroll container`, async ({
      page,
    }, testInfo) => {
      test.skip(gates.tabs.skip, gates.tabs.reason);
      await openFirstInstance(page);

      const tab = page.getByRole("tab", { name: tabName });
      await tab.click();
      await expect(tab).toHaveAttribute("aria-selected", "true");

      const fullyInside = await tab.evaluate((el) => {
        const tabRect = el.getBoundingClientRect();
        const list = el.closest(".tablist");
        if (!list) return false;
        const listRect = list.getBoundingClientRect();
        const tolerance = 1;
        return (
          tabRect.left >= listRect.left - tolerance &&
          tabRect.right <= listRect.right + tolerance &&
          tabRect.top >= listRect.top - tolerance &&
          tabRect.bottom <= listRect.bottom + tolerance
        );
      });
      expect(fullyInside).toBe(true);

      if (isMobile(testInfo)) {
        await expect(tab.locator("svg")).toBeVisible();
        const labelHidden = await tab.locator("span").nth(1).evaluate((el) => {
          const styles = window.getComputedStyle(el);
          return styles.display === "none" || el.getAttribute("aria-hidden") === "true";
        });
        expect(labelHidden).toBe(true);
        await expect(tab).toHaveAttribute("aria-label", tabName);
      }
    });
  }
});
