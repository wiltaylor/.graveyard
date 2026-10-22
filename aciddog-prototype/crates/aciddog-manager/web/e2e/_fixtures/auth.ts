import type { APIRequestContext, Page } from "@playwright/test";
import { expect, request } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import { dirname } from "node:path";

export const authStatePath = "e2e/.auth/state.json";

export function missingAuthEnv(): boolean {
  return !process.env.WEB_TEST_USER || !process.env.WEB_TEST_PASS;
}

export async function writeEmptyStorageState(): Promise<void> {
  await mkdir(dirname(authStatePath), { recursive: true });
  const context = await request.newContext();
  await context.storageState({ path: authStatePath });
  await context.dispose();
}

export async function loginAndPersistStorage(
  api: APIRequestContext,
): Promise<void> {
  if (missingAuthEnv()) {
    await writeEmptyStorageState();
    return;
  }

  const response = await api.post("/api/auth/login", {
    data: {
      username: process.env.WEB_TEST_USER,
      password: process.env.WEB_TEST_PASS,
    },
  });

  expect(
    response.ok(),
    "WEB_TEST_USER/WEB_TEST_PASS must identify a seeded manager admin; see docs/mobile.md",
  ).toBe(true);

  await mkdir(dirname(authStatePath), { recursive: true });
  await api.storageState({ path: authStatePath });
}

export async function gotoAndAuth(page: Page, path = "/"): Promise<void> {
  await page.goto(path);
}
