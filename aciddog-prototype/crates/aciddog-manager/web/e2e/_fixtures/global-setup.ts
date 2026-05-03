import { request, type FullConfig } from "@playwright/test";

import { loginAndPersistStorage } from "./auth";

export default async function globalSetup(config: FullConfig): Promise<void> {
  const baseURL = config.projects[0].use.baseURL ?? "http://127.0.0.1:4173";
  const api = await request.newContext({ baseURL });
  try {
    await loginAndPersistStorage(api);
  } finally {
    await api.dispose();
  }
}
