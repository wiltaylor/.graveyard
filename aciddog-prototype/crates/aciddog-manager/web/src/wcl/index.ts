// Thin wrapper around the `wcl_wasm` npm package.
//
// The WASM module needs an async `init()` before any of its parse
// functions can be called. We init once on first use; subsequent calls
// reuse the same instance.
//
// The package's default loader expects to fetch the .wasm sibling via
// `new URL('wcl_wasm_bg.wasm', import.meta.url)`. Vite resolves that in
// dev mode but doesn't always bundle the asset for prod, so we import
// the .wasm explicitly with `?url` and pass the resolved URL into
// `init()`.

import init, { parse, query as runQuery } from "wcl_wasm";
// @ts-expect-error - Vite handles the `?url` suffix at build time.
import wasmUrl from "wcl_wasm/wcl_wasm_bg.wasm?url";
// Inline the aciddog.wcl schema so `import <aciddog.wcl>` resolves in
// the browser without a network round-trip. Vite's `?raw` suffix
// imports the file as a string at build time.
// @ts-expect-error - Vite handles the `?raw` suffix at build time.
import aciddogSchema from "../../../../../wcl/aciddog.wcl?raw";

// `import <aciddog.wcl>` resolves through the WCL library search path
// (XDG dirs on the daemon host). In the browser those paths don't
// exist, so we intercept any lookup ending in `aciddog.wcl` and serve
// the bundled schema instead.
const PARSE_OPTIONS = {
  importResolver: (path: string): string | null => {
    if (path.endsWith("/aciddog.wcl") || path === "aciddog.wcl") {
      return aciddogSchema as string;
    }
    return null;
  },
};

export interface WclDiagnostic {
  severity: "error" | "warning";
  message: string;
  code?: string;
}

export interface WclParseResult {
  values: Record<string, unknown>;
  hasErrors: boolean;
  diagnostics: WclDiagnostic[];
}

let initPromise: Promise<void> | null = null;

export function initWcl(): Promise<void> {
  if (!initPromise) {
    initPromise = init({ module_or_path: wasmUrl }).then(() => undefined);
  }
  return initPromise;
}

export async function parseWcl(source: string): Promise<WclParseResult> {
  await initWcl();
  const doc = parse(source, PARSE_OPTIONS) as WclParseResult;
  return doc;
}

export async function queryWcl(source: string, q: string): Promise<unknown> {
  await initWcl();
  return runQuery(source, q, PARSE_OPTIONS);
}

export interface ValidationResult {
  ok: boolean;
  diagnostics: WclDiagnostic[];
}

export async function validateWcl(source: string): Promise<ValidationResult> {
  const doc = await parseWcl(source);
  const errors = doc.diagnostics.filter((d) => d.severity === "error");
  return {
    ok: errors.length === 0 && !doc.hasErrors,
    diagnostics: doc.diagnostics,
  };
}
