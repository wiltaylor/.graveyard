import type { Locator, Page, TestInfo } from "@playwright/test";
import { expect, test } from "@playwright/test";

import { gotoAndAuth, missingAuthEnv } from "./auth";

export { gotoAndAuth };

export const viewportProjects = {
  mobile375: "mobile-375",
  mobile414: "mobile-414",
  tablet768: "tablet-768",
  desktop1280: "desktop-1280",
  webkitMobile414: "webkit-mobile-414",
} as const;

export const mobileProjectNames = new Set([
  viewportProjects.mobile375,
  viewportProjects.mobile414,
  viewportProjects.webkitMobile414,
]);

export const drawerProjectNames = new Set([
  viewportProjects.mobile375,
  viewportProjects.mobile414,
  viewportProjects.tablet768,
  viewportProjects.webkitMobile414,
]);

export const seededAuthSkipReason =
  "requires WEB_TEST_USER/WEB_TEST_PASS and a seeded admin; see docs/mobile.md";

export const gates = {
  appShellBreakpoints: {
    ticket: "40aa10ce-86b7-4e17-8c0c-1fd9bfacc1c6",
    reason: "depends on task 40aa10ce-86b7-4e17-8c0c-1fd9bfacc1c6",
    skip: true,
  },
  topbarCollapse: {
    ticket: "c69c1499-3328-433d-b878-ba075d7050d2",
    reason: "depends on task c69c1499-3328-433d-b878-ba075d7050d2",
    skip: true,
  },
  drawer: {
    ticket: "ec979f18-db77-4ee2-b75b-9b1dfa64c62b",
    reason: "depends on task ec979f18-db77-4ee2-b75b-9b1dfa64c62b",
    skip: false,
  },
  tabs: {
    ticket: "e48778e1-5142-4910-900f-85f3e2da1b9a",
    reason: "depends on task e48778e1-5142-4910-900f-85f3e2da1b9a",
    skip: false,
  },
  touchTargets: {
    ticket: "48d16c8a-fc51-4c44-8915-3b4dfa1d2a67",
    reason: "depends on task 48d16c8a-fc51-4c44-8915-3b4dfa1d2a67",
    skip: true,
  },
};

export const cataloguePages = [
  {
    name: "Login",
    open: async (page: Page) => {
      await page.goto("/");
    },
  },
  {
    name: "Instances",
    open: async (page: Page) => {
      await gotoAndAuth(page);
    },
  },
  {
    name: "Team",
    open: async (page: Page) => {
      await openInstanceTab(page, "Team");
    },
  },
  {
    name: "Agents and Chat",
    open: async (page: Page) => {
      await openInstanceTab(page, "Agents");
    },
  },
  {
    name: "Tasks",
    open: async (page: Page) => {
      await openInstanceTab(page, "Tasks");
    },
  },
  {
    name: "Workflows",
    open: async (page: Page) => {
      await openInstanceTab(page, "Workflows");
    },
  },
  {
    name: "Cron",
    open: async (page: Page) => {
      await openInstanceTab(page, "Cron");
    },
  },
  {
    name: "Settings > Users",
    open: async (page: Page) => {
      await openSettings(page, "Users");
    },
  },
  {
    name: "Settings > Skills",
    open: async (page: Page) => {
      await openSettings(page, "Skills");
    },
  },
] as const;

export function skipWithoutAuth(): void {
  test.skip(missingAuthEnv(), seededAuthSkipReason);
}

export function isDesktop(info: TestInfo): boolean {
  return info.project.name === viewportProjects.desktop1280;
}

export function isDrawerViewport(info: TestInfo): boolean {
  return drawerProjectNames.has(info.project.name);
}

export function isMobile(info: TestInfo): boolean {
  return mobileProjectNames.has(info.project.name);
}

export async function expectNoHorizontalOverflow(page: Page): Promise<void> {
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          document.documentElement.scrollWidth <=
          document.documentElement.clientWidth,
      ),
    )
    .toBe(true);
}

export async function firstInstanceRow(page: Page): Promise<Locator | null> {
  await gotoAndAuth(page);
  const rows = page.locator(".fsidebar .tn").filter({
    hasNotText: /^(Web UI|Users|Skills|Agents|Channels|Loading|No instances)/,
  });
  if ((await rows.count()) === 0) return null;
  return rows.first();
}

export async function openFirstInstance(page: Page): Promise<void> {
  const row = await firstInstanceRow(page);
  test.skip(row === null, "requires at least one registered manager instance");
  await row!.click();
}

export async function openInstanceTab(page: Page, tabName: string): Promise<void> {
  await openFirstInstance(page);
  await page.getByRole("tab", { name: tabName }).click();
}

export async function openSettings(page: Page, name: "Users" | "Skills"): Promise<void> {
  await gotoAndAuth(page);
  await page.locator(".fsidebar .tn", { hasText: name }).click();
}
