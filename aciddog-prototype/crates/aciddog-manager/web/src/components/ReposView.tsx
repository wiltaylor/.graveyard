import {
  For,
  Match,
  Show,
  Switch,
  createEffect,
  createMemo,
  createResource,
  createSignal,
  onCleanup,
  onMount,
  type JSX,
} from "solid-js";

import { getRepoDiff, listRepos } from "../api";
import type {
  RepoDiffResponse,
  WireDiffLine,
  WireRepository,
  WireWorktree,
} from "../types";
import { Badge, Button, Empty, Toast } from "./ui";

interface Props {
  instanceId: string;
}

type Mode =
  | { kind: "list" }
  | { kind: "diff"; repoId: string; branch: string };

interface DiffState {
  key: string;
  loading: boolean;
  response: RepoDiffResponse | null;
  error: string | null;
}

function diffKey(repoId: string, branch: string): string {
  return `${repoId}\u0000${branch}`;
}

function branchLabel(worktree: WireWorktree): string {
  return worktree.branch ?? "(detached) — no branch to diff";
}

function defaultBranchLabel(repo: WireRepository): string {
  return repo.default_branch ?? "(no default)";
}

export default function ReposView(props: Props): JSX.Element {
  const [repos, { refetch }] = createResource(
    () => props.instanceId,
    listRepos,
  );
  const [mode, setMode] = createSignal<Mode>({ kind: "list" });
  const [diffCache, setDiffCache] = createSignal<
    Record<string, RepoDiffResponse>
  >({});
  const [diffState, setDiffState] = createSignal<DiffState | null>(null);

  createEffect(() => {
    void props.instanceId;
    setMode({ kind: "list" });
    setDiffCache({});
    setDiffState(null);
  });

  onMount(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && mode().kind === "diff") {
        event.preventDefault();
        setMode({ kind: "list" });
      }
    };
    window.addEventListener("keydown", onKeyDown);
    onCleanup(() => window.removeEventListener("keydown", onKeyDown));
  });

  const repoList = createMemo(() => repos()?.repos ?? []);

  const selectedRepo = createMemo(() => {
    const m = mode();
    if (m.kind !== "diff") return null;
    return repoList().find((repo) => repo.id === m.repoId) ?? null;
  });

  createEffect(() => {
    const m = mode();
    if (m.kind !== "diff") {
      setDiffState(null);
      return;
    }

    const key = diffKey(m.repoId, m.branch);
    const cached = diffCache()[key];
    if (cached) {
      setDiffState({ key, loading: false, response: cached, error: null });
      return;
    }

    setDiffState({ key, loading: true, response: null, error: null });
    void getRepoDiff(props.instanceId, m.repoId, m.branch)
      .then((response) => {
        const current = mode();
        if (
          current.kind !== "diff" ||
          diffKey(current.repoId, current.branch) !== key
        ) {
          return;
        }
        setDiffCache((cache) => ({ ...cache, [key]: response }));
        setDiffState({ key, loading: false, response, error: null });
      })
      .catch((err) => {
        const current = mode();
        if (
          current.kind !== "diff" ||
          diffKey(current.repoId, current.branch) !== key
        ) {
          return;
        }
        setDiffState({
          key,
          loading: false,
          response: null,
          error: err instanceof Error ? err.message : String(err),
        });
      });
  });

  const openDiff = (repo: WireRepository, worktree: WireWorktree) => {
    if (!worktree.branch) return;
    setMode({ kind: "diff", repoId: repo.id, branch: worktree.branch });
  };

  return (
    <div class="h-full min-h-0 flex flex-col bg-bg-0">
      <Switch>
        <Match when={mode().kind === "list"}>
          <header class="flex items-center justify-between px-6 py-3 border-b border-border-subtle bg-bg-1">
            <div class="flex items-center gap-3">
              <h2 class="text-base font-semibold text-fg-0">Repos</h2>
              <Show when={repos.loading}>
                <Badge tone="neutral" dot>
                  loading
                </Badge>
              </Show>
            </div>
            <Button size="sm" variant="ghost" icon="refresh" onClick={refetch}>
              Refresh
            </Button>
          </header>

          <Show when={repos.error}>
            <div class="mx-6 mt-3">
              <Toast tone="danger">{String(repos.error)}</Toast>
            </div>
          </Show>

          <Show when={repos()?.ok === false}>
            <div class="mx-6 mt-3">
              <Toast tone="danger">
                {repos()?.error ?? "failed to fetch repositories"}
              </Toast>
            </div>
          </Show>

          <div class="flex-1 min-h-0 overflow-auto px-6 py-4">
            <Show
              when={repoList().length > 0}
              fallback={
                <Empty title="No repositories">
                  This instance did not report any repositories.
                </Empty>
              }
            >
              <div class="space-y-3">
                <For each={repoList()}>
                  {(repo) => (
                    <RepoCard repo={repo} onOpenDiff={openDiff} />
                  )}
                </For>
              </div>
            </Show>
          </div>
        </Match>

        <Match when={mode().kind === "diff"}>
          <DiffPane
            mode={mode() as Extract<Mode, { kind: "diff" }>}
            repo={selectedRepo()}
            state={diffState()}
            onBack={() => setMode({ kind: "list" })}
          />
        </Match>
      </Switch>
    </div>
  );
}

function RepoCard(props: {
  repo: WireRepository;
  onOpenDiff: (repo: WireRepository, worktree: WireWorktree) => void;
}): JSX.Element {
  return (
    <details class="fcard" open>
      <summary
        class="flex cursor-pointer select-none items-center justify-between gap-3 px-4 py-3"
        title={props.repo.path}
      >
        <div class="min-w-0">
          <div class="truncate text-sm font-semibold text-fg-0">
            {props.repo.id}
          </div>
          <div class="mt-1 text-xs text-fg-2">
            default: {defaultBranchLabel(props.repo)}
          </div>
        </div>
        <Badge tone="neutral">{props.repo.worktrees.length} worktrees</Badge>
      </summary>
      <div class="border-t border-border-subtle">
        <For each={props.repo.worktrees}>
          {(worktree) => {
            const interactive = () => worktree.branch !== null;
            const isDefault = () =>
              worktree.branch !== null &&
              worktree.branch === props.repo.default_branch;
            return (
              <button
                type="button"
                class={`w-full px-4 py-2.5 text-left border-b border-border-subtle last:border-b-0 ${
                  interactive()
                    ? "cursor-pointer hover:bg-bg-2"
                    : "cursor-default opacity-70"
                }`}
                title={worktree.path}
                disabled={!interactive()}
                onClick={() => props.onOpenDiff(props.repo, worktree)}
              >
                <div class="flex items-center justify-between gap-3">
                  <span class="min-w-0 truncate font-mono text-xs text-fg-1">
                    {branchLabel(worktree)}
                  </span>
                  <Show when={isDefault()}>
                    <Badge tone="success">(default)</Badge>
                  </Show>
                </div>
              </button>
            );
          }}
        </For>
      </div>
    </details>
  );
}

function DiffPane(props: {
  mode: Extract<Mode, { kind: "diff" }>;
  repo: WireRepository | null;
  state: DiffState | null;
  onBack: () => void;
}): JSX.Element {
  const title = () => {
    const repo = props.repo;
    const defaultBranch = repo ? defaultBranchLabel(repo) : "(default)";
    return `${props.mode.repoId} · ${props.mode.branch} ← ${defaultBranch}`;
  };

  const response = () => props.state?.response ?? null;

  return (
    <div class="h-full min-h-0 flex flex-col bg-bg-0">
      <header class="sticky top-0 z-10 flex items-center justify-between gap-3 border-b border-border-subtle bg-bg-1 px-6 py-3">
        <div class="min-w-0">
          <h2 class="truncate text-base font-semibold text-fg-0" title={title()}>
            {title()}
          </h2>
          <Show when={props.state?.loading}>
            <div class="mt-1 text-xs text-fg-2">Loading diff…</div>
          </Show>
        </div>
        <Button size="sm" variant="ghost" icon="arrow-right" onClick={props.onBack}>
          Back
        </Button>
      </header>

      <Show when={props.state?.error}>
        {(error) => (
          <div class="mx-6 mt-3">
            <Toast tone="danger">{error()}</Toast>
          </div>
        )}
      </Show>

      <div class="flex-1 min-h-0 overflow-auto">
        <Show when={response()}>
          {(resp) => (
            <Switch>
              <Match when={resp().outcome === "diff"}>
                <DiffLines lines={(resp() as { outcome: "diff"; lines: WireDiffLine[] }).lines} />
              </Match>
              <Match when={resp().outcome === "no_forward_changes"}>
                <div class="flex h-full items-center justify-center px-6 text-sm text-fg-2">
                  No forward changes.
                </div>
              </Match>
              <Match when={resp().outcome === "detached"}>
                <div class="mx-6 mt-4 rounded border border-warning/30 bg-warning-bg px-4 py-3 text-sm text-warning-fg">
                  Detached worktree: no branch to diff.
                </div>
              </Match>
              <Match when={resp().outcome === "error"}>
                <RepoDiffError response={resp() as Extract<RepoDiffResponse, { outcome: "error" }>} />
              </Match>
            </Switch>
          )}
        </Show>
      </div>
    </div>
  );
}

function DiffLines(props: { lines: WireDiffLine[] }): JSX.Element {
  return (
    <pre class="min-h-full overflow-visible whitespace-pre-wrap break-words bg-bg-0 p-4 font-mono text-xs leading-relaxed text-fg-1">
      <For each={props.lines}>
        {(line) => (
          <>
            <span class={`diff-line--${line.kind}`}>{line.text}</span>
            {"\n"}
          </>
        )}
      </For>
    </pre>
  );
}

function RepoDiffError(props: {
  response: Extract<RepoDiffResponse, { outcome: "error" }>;
}): JSX.Element {
  return (
    <div class="m-6 rounded border border-danger/40 bg-danger-bg p-4 text-danger-fg">
      <div class="mb-3 text-sm font-semibold">Diff command failed.</div>
      <pre class="whitespace-pre-wrap break-words font-mono text-xs leading-relaxed">
        <Show when={props.response.command}>
          {(command) => <>$ {command()}{'\n\n'}</>}
        </Show>
        {props.response.stderr}
      </pre>
    </div>
  );
}
