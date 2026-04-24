import {
  For,
  Show,
  createEffect,
  createSignal,
  onCleanup,
  onMount,
  type JSX,
} from "solid-js";
import cytoscape, {
  type Core as CytoscapeCore,
  type ElementDefinition,
} from "cytoscape";

import { api } from "../api";
import type {
  WireBankInfo,
  WireGraph,
  WireHit,
  WireLink,
} from "../types";

interface Props {
  instanceId: string;
}

type BankSubTab = "search" | "graph";

export default function MemoryView(props: Props): JSX.Element {
  const [banks, setBanks] = createSignal<WireBankInfo[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [selectedBank, setSelectedBank] = createSignal<string | null>(null);

  const loadBanks = async () => {
    setLoading(true);
    setError(null);
    try {
      const resp = await api.listMemoryBanks(props.instanceId);
      if (!resp.ok) {
        setError(resp.error ?? "remote returned no data");
        setBanks([]);
      } else {
        setBanks(resp.banks);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  createEffect(() => {
    void props.instanceId;
    setSelectedBank(null);
    void loadBanks();
  });

  return (
    <div class="h-full flex flex-col">
      <header class="flex items-center justify-between px-4 py-2 border-b border-slate-800 bg-slate-900/40">
        <div class="flex items-center gap-2">
          <h2 class="text-lg font-semibold">Memory</h2>
          <Show when={selectedBank()}>
            {(bank) => (
              <>
                <span class="text-slate-500">·</span>
                <span class="font-mono">{bank()}</span>
              </>
            )}
          </Show>
        </div>
        <div class="flex items-center gap-2">
          <Show when={loading()}>
            <span class="text-xs text-slate-400">loading…</span>
          </Show>
          <Show when={selectedBank()}>
            <button
              type="button"
              onClick={() => setSelectedBank(null)}
              class="text-xs text-slate-400 hover:text-slate-100"
            >
              ← back to banks
            </button>
          </Show>
          <button
            onClick={loadBanks}
            class="text-xs text-slate-400 hover:text-slate-100"
          >
            refresh
          </button>
        </div>
      </header>

      <Show when={error()}>
        <div class="mx-4 mt-2 rounded border border-red-800 bg-red-900/40 px-2 py-1 text-sm text-red-200">
          {error()}
        </div>
      </Show>

      <Show
        when={selectedBank()}
        fallback={
          <BankList
            banks={banks()}
            onSelect={(id) => setSelectedBank(id)}
          />
        }
      >
        {(bank) => (
          <BankDetail
            instanceId={props.instanceId}
            bank={bank()}
            description={
              banks().find((b) => b.id === bank())?.description ?? ""
            }
            onChange={() => loadBanks()}
          />
        )}
      </Show>
    </div>
  );
}

function BankList(props: {
  banks: WireBankInfo[];
  onSelect: (bankId: string) => void;
}): JSX.Element {
  return (
    <div class="flex-1 overflow-y-auto p-4">
      <Show
        when={props.banks.length > 0}
        fallback={
          <div class="text-sm text-slate-500 italic">
            No memory banks declared on this instance.
          </div>
        }
      >
        <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
          <For each={props.banks}>
            {(b) => (
              <button
                type="button"
                onClick={() => props.onSelect(b.id)}
                class="text-left rounded border border-slate-800 bg-slate-900/60 hover:border-slate-600 p-3 space-y-1"
              >
                <div class="flex items-center justify-between">
                  <span class="font-semibold">{b.id}</span>
                  <span class="text-xs text-slate-400">
                    {b.entry_count} entr{b.entry_count === 1 ? "y" : "ies"}
                  </span>
                </div>
                <p class="text-sm text-slate-300 whitespace-pre-wrap">
                  {b.description}
                </p>
                <Show when={b.embed_model}>
                  <p class="text-xs text-slate-500 font-mono">
                    {b.embed_model}
                  </p>
                </Show>
              </button>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}

function BankDetail(props: {
  instanceId: string;
  bank: string;
  description: string;
  onChange: () => void;
}): JSX.Element {
  const [sub, setSub] = createSignal<BankSubTab>("search");
  return (
    <div class="flex-1 flex flex-col overflow-hidden">
      <Show when={props.description}>
        <p class="mx-4 mt-2 text-xs text-slate-400">{props.description}</p>
      </Show>
      <nav class="flex items-center gap-1 px-4 pt-2 border-b border-slate-800">
        <TabBtn
          label="Search"
          active={sub() === "search"}
          onClick={() => setSub("search")}
        />
        <TabBtn
          label="Graph"
          active={sub() === "graph"}
          onClick={() => setSub("graph")}
        />
      </nav>
      <div class="flex-1 overflow-hidden">
        <Show when={sub() === "search"}>
          <SearchView
            instanceId={props.instanceId}
            bank={props.bank}
            onChange={props.onChange}
          />
        </Show>
        <Show when={sub() === "graph"}>
          <GraphView instanceId={props.instanceId} bank={props.bank} />
        </Show>
      </div>
    </div>
  );
}

function TabBtn(props: {
  label: string;
  active: boolean;
  onClick: () => void;
}): JSX.Element {
  return (
    <button
      type="button"
      onClick={props.onClick}
      class="px-3 py-1.5 text-sm border-b-2 -mb-px"
      classList={{
        "border-sky-500 text-slate-100": props.active,
        "border-transparent text-slate-400 hover:text-slate-200": !props.active,
      }}
    >
      {props.label}
    </button>
  );
}

// ---------------------------------------------------------------------------
// Search tab
// ---------------------------------------------------------------------------

function SearchView(props: {
  instanceId: string;
  bank: string;
  onChange: () => void;
}): JSX.Element {
  const [q, setQ] = createSignal("");
  const [titleContains, setTitleContains] = createSignal("");
  const [includeTags, setIncludeTags] = createSignal("");
  const [excludeTags, setExcludeTags] = createSignal("");
  const [hits, setHits] = createSignal<WireHit[]>([]);
  const [error, setError] = createSignal<string | null>(null);
  const [searching, setSearching] = createSignal(false);
  const [openEntry, setOpenEntry] = createSignal<WireHit | null>(null);
  const [showAdd, setShowAdd] = createSignal(false);

  const runSearch = async () => {
    setSearching(true);
    setError(null);
    try {
      const splitTags = (s: string) =>
        s
          .split(",")
          .map((t) => t.trim())
          .filter((t) => t.length > 0);
      const resp = await api.searchMemory(props.instanceId, props.bank, {
        q: q().trim() || undefined,
        title_contains: titleContains().trim() || undefined,
        include_tags: splitTags(includeTags()),
        exclude_tags: splitTags(excludeTags()),
        limit: 50,
      });
      if (!resp.ok) {
        setError(resp.error ?? "search failed");
        setHits([]);
      } else {
        setHits(resp.hits);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSearching(false);
    }
  };

  createEffect(() => {
    void props.bank;
    setHits([]);
    void runSearch();
  });

  return (
    <div class="h-full flex flex-col">
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void runSearch();
        }}
        class="px-4 py-3 space-y-2 border-b border-slate-800 bg-slate-900/30"
      >
        <div class="flex items-center gap-2">
          <input
            type="text"
            placeholder="Semantic query (natural language) — empty = list newest"
            value={q()}
            onInput={(e) => setQ(e.currentTarget.value)}
            class="flex-1 rounded border border-slate-700 bg-slate-950 px-2 py-1 text-sm"
          />
          <button
            type="submit"
            disabled={searching()}
            class="rounded border border-sky-700 bg-sky-900/40 hover:bg-sky-800 disabled:opacity-50 px-3 py-1 text-sm"
          >
            {searching() ? "Searching…" : "Search"}
          </button>
          <button
            type="button"
            onClick={() => setShowAdd(true)}
            class="rounded border border-emerald-700 bg-emerald-900/40 hover:bg-emerald-800 px-3 py-1 text-sm"
          >
            + new entry
          </button>
        </div>
        <div class="grid grid-cols-1 sm:grid-cols-3 gap-2 text-xs">
          <input
            type="text"
            placeholder="Title contains…"
            value={titleContains()}
            onInput={(e) => setTitleContains(e.currentTarget.value)}
            class="rounded border border-slate-700 bg-slate-950 px-2 py-1"
          />
          <input
            type="text"
            placeholder="Include tags (comma-separated)"
            value={includeTags()}
            onInput={(e) => setIncludeTags(e.currentTarget.value)}
            class="rounded border border-slate-700 bg-slate-950 px-2 py-1"
          />
          <input
            type="text"
            placeholder="Exclude tags (comma-separated)"
            value={excludeTags()}
            onInput={(e) => setExcludeTags(e.currentTarget.value)}
            class="rounded border border-slate-700 bg-slate-950 px-2 py-1"
          />
        </div>
      </form>

      <Show when={error()}>
        <div class="mx-4 mt-2 rounded border border-red-800 bg-red-900/40 px-2 py-1 text-sm text-red-200">
          {error()}
        </div>
      </Show>

      <div class="flex-1 overflow-y-auto px-4 py-3 space-y-2">
        <Show
          when={hits().length > 0}
          fallback={
            <div class="text-sm text-slate-500 italic">
              {searching() ? "" : "(no matches)"}
            </div>
          }
        >
          <For each={hits()}>
            {(h) => (
              <EntryCard hit={h} onOpen={() => setOpenEntry(h)} />
            )}
          </For>
        </Show>
      </div>

      <Show when={showAdd()}>
        <EntryModal
          instanceId={props.instanceId}
          bank={props.bank}
          mode="create"
          onClose={() => setShowAdd(false)}
          onSaved={() => {
            setShowAdd(false);
            void runSearch();
            props.onChange();
          }}
          onError={(e) => setError(e)}
        />
      </Show>

      <Show when={openEntry()}>
        {(hit) => (
          <EntryModal
            instanceId={props.instanceId}
            bank={props.bank}
            mode="edit"
            initialHit={hit()}
            allHits={hits()}
            onClose={() => setOpenEntry(null)}
            onSaved={() => {
              setOpenEntry(null);
              void runSearch();
              props.onChange();
            }}
            onError={(e) => setError(e)}
          />
        )}
      </Show>
    </div>
  );
}

function EntryCard(props: { hit: WireHit; onOpen: () => void }): JSX.Element {
  return (
    <button
      type="button"
      onClick={props.onOpen}
      class="w-full text-left rounded border border-slate-800 bg-slate-900/60 hover:border-slate-600 p-3 space-y-1"
    >
      <div class="flex items-center justify-between gap-2">
        <span class="font-medium truncate">
          {props.hit.title || "(untitled)"}
        </span>
        <span class="text-xs text-slate-500 font-mono shrink-0">
          {props.hit.id.slice(0, 8)}
          <Show when={typeof props.hit.distance === "number"}>
            {" "}
            · d={props.hit.distance!.toFixed(3)}
          </Show>
        </span>
      </div>
      <p class="text-sm whitespace-pre-wrap line-clamp-3 text-slate-300">
        {props.hit.body}
      </p>
      <Show when={props.hit.tags.length > 0}>
        <div class="flex flex-wrap gap-1">
          <For each={props.hit.tags}>
            {(t) => (
              <span class="rounded bg-slate-800 px-1.5 py-0.5 text-xs font-mono">
                {t}
              </span>
            )}
          </For>
        </div>
      </Show>
    </button>
  );
}

// ---------------------------------------------------------------------------
// Entry create / edit modal
// ---------------------------------------------------------------------------

function EntryModal(props: {
  instanceId: string;
  bank: string;
  mode: "create" | "edit";
  initialHit?: WireHit;
  allHits?: WireHit[];
  onClose: () => void;
  onSaved: () => void;
  onError: (e: string) => void;
}): JSX.Element {
  const [title, setTitle] = createSignal(props.initialHit?.title ?? "");
  const [body, setBody] = createSignal(props.initialHit?.body ?? "");
  const [tagsStr, setTagsStr] = createSignal(
    (props.initialHit?.tags ?? []).join(", "),
  );
  const [saving, setSaving] = createSignal(false);
  const [links, setLinks] = createSignal<WireLink[]>([]);
  const [linksLoading, setLinksLoading] = createSignal(false);
  const [linkTarget, setLinkTarget] = createSignal("");
  const [linkKind, setLinkKind] = createSignal("");

  const loadLinks = async () => {
    if (!props.initialHit) return;
    setLinksLoading(true);
    try {
      const resp = await api.memoryEntryLinks(
        props.instanceId,
        props.bank,
        props.initialHit.id,
      );
      if (resp.ok) setLinks(resp.links);
    } catch {
      // Non-fatal; the modal stays usable.
    } finally {
      setLinksLoading(false);
    }
  };

  createEffect(() => {
    void props.initialHit?.id;
    if (props.mode === "edit") void loadLinks();
  });

  const parseTags = () =>
    tagsStr()
      .split(",")
      .map((t) => t.trim())
      .filter((t) => t.length > 0);

  const save = async (e: SubmitEvent) => {
    e.preventDefault();
    if (body().trim() === "") return;
    setSaving(true);
    try {
      if (props.mode === "create") {
        const resp = await api.storeMemoryEntry(
          props.instanceId,
          props.bank,
          {
            body: body(),
            title: title().trim() || undefined,
            tags: parseTags(),
          },
        );
        if (!resp.ok) {
          props.onError(resp.error ?? "store failed");
          return;
        }
      } else if (props.initialHit) {
        const resp = await api.updateMemoryEntry(
          props.instanceId,
          props.bank,
          props.initialHit.id,
          {
            body: body(),
            title: title().trim() || undefined,
            tags: parseTags(),
          },
        );
        if (!resp.ok) {
          props.onError(resp.error ?? "update failed");
          return;
        }
      }
      props.onSaved();
    } catch (err) {
      props.onError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  const removeEntry = async () => {
    if (!props.initialHit) return;
    if (
      !confirm(
        `Delete entry "${props.initialHit.title || props.initialHit.id.slice(0, 8)}"? This cannot be undone.`,
      )
    )
      return;
    setSaving(true);
    try {
      const resp = await api.removeMemoryEntry(
        props.instanceId,
        props.bank,
        props.initialHit.id,
      );
      if (!resp.ok) {
        props.onError(resp.error ?? "delete failed");
        return;
      }
      props.onSaved();
    } finally {
      setSaving(false);
    }
  };

  const addLink = async () => {
    if (!props.initialHit) return;
    const to = linkTarget();
    if (!to) return;
    try {
      const resp = await api.linkMemory(props.instanceId, props.bank, {
        from: props.initialHit.id,
        to,
        kind: linkKind().trim() || undefined,
      });
      if (!resp.ok) {
        props.onError(resp.error ?? "link failed");
        return;
      }
      setLinkTarget("");
      setLinkKind("");
      void loadLinks();
    } catch (err) {
      props.onError(err instanceof Error ? err.message : String(err));
    }
  };

  const removeLink = async (peerId: string, direction: string) => {
    if (!props.initialHit) return;
    const self = props.initialHit.id;
    const payload =
      direction === "out"
        ? { from: self, to: peerId }
        : { from: peerId, to: self };
    try {
      const resp = await api.unlinkMemory(props.instanceId, props.bank, payload);
      if (!resp.ok) {
        props.onError(resp.error ?? "unlink failed");
        return;
      }
      void loadLinks();
    } catch (err) {
      props.onError(err instanceof Error ? err.message : String(err));
    }
  };

  const linkCandidates = () =>
    (props.allHits ?? []).filter(
      (h) =>
        h.id !== props.initialHit?.id &&
        !links().some(
          (l) => l.peer_id === h.id && l.direction === "out",
        ),
    );

  const title_ = () =>
    props.mode === "create"
      ? "New memory entry"
      : `Entry · ${props.initialHit!.id.slice(0, 8)}`;

  return (
    <div
      class="fixed inset-0 z-50 bg-black/60 flex items-start justify-center overflow-y-auto"
      onClick={props.onClose}
    >
      <div
        class="my-10 w-full max-w-2xl rounded border border-slate-700 bg-slate-900 p-4 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <header class="flex items-center justify-between mb-3">
          <h2 class="text-base font-semibold">{title_()}</h2>
          <button
            type="button"
            onClick={props.onClose}
            class="text-slate-400 hover:text-slate-100"
          >
            ✕
          </button>
        </header>
        <form onSubmit={save} class="space-y-3 text-sm">
          <label class="block">
            <span class="text-xs text-slate-400">Title (optional)</span>
            <input
              type="text"
              value={title()}
              onInput={(e) => setTitle(e.currentTarget.value)}
              class="mt-1 w-full rounded border border-slate-700 bg-slate-950 px-2 py-1"
            />
          </label>
          <label class="block">
            <span class="text-xs text-slate-400">Body</span>
            <textarea
              rows={6}
              required
              value={body()}
              onInput={(e) => setBody(e.currentTarget.value)}
              class="mt-1 w-full rounded border border-slate-700 bg-slate-950 px-2 py-1 font-mono text-sm"
            />
          </label>
          <label class="block">
            <span class="text-xs text-slate-400">
              Tags (comma-separated)
            </span>
            <input
              type="text"
              value={tagsStr()}
              onInput={(e) => setTagsStr(e.currentTarget.value)}
              class="mt-1 w-full rounded border border-slate-700 bg-slate-950 px-2 py-1"
            />
          </label>

          <Show when={props.mode === "edit"}>
            <section class="border-t border-slate-800 pt-3 space-y-2">
              <h3 class="text-xs uppercase tracking-wider text-slate-400">
                Links
              </h3>
              <Show when={linksLoading()}>
                <div class="text-xs text-slate-500 italic">
                  loading links…
                </div>
              </Show>
              <Show
                when={!linksLoading() && links().length > 0}
                fallback={
                  <Show when={!linksLoading()}>
                    <div class="text-xs text-slate-500 italic">
                      No links yet.
                    </div>
                  </Show>
                }
              >
                <ul class="space-y-1">
                  <For each={links()}>
                    {(l) => (
                      <li class="flex items-center justify-between rounded border border-slate-800 bg-slate-950/50 px-2 py-1 text-xs">
                        <div>
                          <span class="font-mono text-slate-500 mr-1">
                            {l.direction === "out" ? "→" : "←"}
                          </span>
                          <span class="font-medium">
                            {l.peer_title || l.peer_id.slice(0, 8)}
                          </span>
                          <Show when={l.kind}>
                            <span class="ml-2 text-slate-400">
                              [{l.kind}]
                            </span>
                          </Show>
                        </div>
                        <button
                          type="button"
                          onClick={() => removeLink(l.peer_id, l.direction)}
                          class="text-red-300 hover:text-red-100"
                        >
                          unlink
                        </button>
                      </li>
                    )}
                  </For>
                </ul>
              </Show>
              <div class="flex gap-2 pt-1">
                <select
                  value={linkTarget()}
                  onInput={(e) => setLinkTarget(e.currentTarget.value)}
                  class="flex-1 rounded border border-slate-700 bg-slate-950 px-2 py-1 text-xs"
                >
                  <option value="">(select entry to link to…)</option>
                  <For each={linkCandidates()}>
                    {(h) => (
                      <option value={h.id}>
                        {h.title || h.id.slice(0, 8)}
                      </option>
                    )}
                  </For>
                </select>
                <input
                  type="text"
                  placeholder="kind (optional)"
                  value={linkKind()}
                  onInput={(e) => setLinkKind(e.currentTarget.value)}
                  class="w-32 rounded border border-slate-700 bg-slate-950 px-2 py-1 text-xs"
                />
                <button
                  type="button"
                  onClick={addLink}
                  disabled={!linkTarget()}
                  class="rounded border border-slate-700 hover:bg-slate-800 disabled:opacity-50 px-2 py-1 text-xs"
                >
                  + link
                </button>
              </div>
            </section>
          </Show>

          <div class="flex items-center justify-between border-t border-slate-800 pt-3">
            <Show
              when={props.mode === "edit"}
              fallback={<span />}
            >
              <button
                type="button"
                onClick={removeEntry}
                disabled={saving()}
                class="text-sm text-red-300 hover:text-red-100 disabled:opacity-50"
              >
                Delete entry
              </button>
            </Show>
            <div class="flex items-center gap-2">
              <button
                type="button"
                onClick={props.onClose}
                class="text-sm text-slate-400 hover:text-slate-100"
              >
                cancel
              </button>
              <button
                type="submit"
                disabled={saving() || body().trim() === ""}
                class="rounded border border-emerald-700 bg-emerald-900/40 hover:bg-emerald-800 disabled:opacity-50 px-3 py-1 text-sm"
              >
                {saving()
                  ? "Saving…"
                  : props.mode === "create"
                    ? "Create"
                    : "Save"}
              </button>
            </div>
          </div>
        </form>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Graph tab — cytoscape-driven force layout.
// ---------------------------------------------------------------------------

function GraphView(props: {
  instanceId: string;
  bank: string;
}): JSX.Element {
  let containerRef: HTMLDivElement | undefined;
  let cy: CytoscapeCore | undefined;
  const [graph, setGraph] = createSignal<WireGraph | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [selected, setSelected] = createSignal<{
    id: string;
    title: string;
    tags: string[];
  } | null>(null);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const resp = await api.memoryGraph(props.instanceId, props.bank);
      if (!resp.ok || !resp.graph) {
        setError(resp.error ?? "remote returned no graph");
        setGraph(null);
      } else {
        setGraph(resp.graph);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  onMount(() => {
    void load();
  });

  // When bank changes, refetch.
  createEffect(() => {
    void props.bank;
    void load();
  });

  // Re-render cytoscape whenever the graph data or the DOM container
  // becomes available.
  createEffect(() => {
    const g = graph();
    if (!containerRef || !g) return;
    const elements: ElementDefinition[] = [
      ...g.nodes.map((n) => ({
        data: { id: n.id, label: n.title || n.id.slice(0, 8) },
      })),
      ...g.edges.map((e, i) => ({
        data: {
          id: `e${i}`,
          source: e.from,
          target: e.to,
          label: e.kind || "",
        },
      })),
    ];
    if (cy) cy.destroy();
    cy = cytoscape({
      container: containerRef,
      elements,
      style: [
        {
          selector: "node",
          style: {
            "background-color": "#38bdf8",
            label: "data(label)",
            color: "#e2e8f0",
            "font-size": 10,
            "text-valign": "bottom",
            "text-margin-y": 6,
            "text-outline-color": "#020617",
            "text-outline-width": 2,
            width: 18,
            height: 18,
          },
        },
        {
          selector: "node:selected",
          style: {
            "background-color": "#facc15",
            "border-width": 2,
            "border-color": "#f59e0b",
          },
        },
        {
          selector: "edge",
          style: {
            width: 1.5,
            "line-color": "#475569",
            "target-arrow-color": "#475569",
            "target-arrow-shape": "triangle",
            "curve-style": "bezier",
            label: "data(label)",
            color: "#94a3b8",
            "font-size": 8,
            "text-background-color": "#020617",
            "text-background-opacity": 0.7,
            "text-background-padding": "1",
          },
        },
      ],
      layout: {
        name: "cose",
        animate: false,
        fit: true,
        padding: 30,
      } as cytoscape.LayoutOptions,
    });
    cy.on("tap", "node", (evt) => {
      const n = evt.target;
      const id = n.id();
      const meta = g.nodes.find((nn) => nn.id === id);
      if (meta) {
        setSelected({ id, title: meta.title, tags: meta.tags });
      }
    });
    cy.on("tap", (evt) => {
      if (evt.target === cy) setSelected(null);
    });
  });

  onCleanup(() => {
    if (cy) {
      cy.destroy();
      cy = undefined;
    }
  });

  return (
    <div class="h-full flex flex-col">
      <div class="px-4 py-2 flex items-center justify-between border-b border-slate-800 bg-slate-900/30 text-xs">
        <div class="flex items-center gap-3 text-slate-400">
          <Show when={loading()}>
            <span>loading…</span>
          </Show>
          <Show when={graph()}>
            {(g) => (
              <span>
                {g().nodes.length} node{g().nodes.length === 1 ? "" : "s"}
                {" · "}
                {g().edges.length} edge{g().edges.length === 1 ? "" : "s"}
              </span>
            )}
          </Show>
        </div>
        <button
          onClick={load}
          class="text-slate-400 hover:text-slate-100"
        >
          refresh
        </button>
      </div>

      <Show when={error()}>
        <div class="mx-4 mt-2 rounded border border-red-800 bg-red-900/40 px-2 py-1 text-sm text-red-200">
          {error()}
        </div>
      </Show>

      <div class="flex-1 relative">
        <div
          ref={(el) => (containerRef = el)}
          class="absolute inset-0 bg-slate-950"
        />
        <Show when={selected()}>
          {(s) => (
            <div class="absolute top-3 right-3 w-72 rounded border border-slate-700 bg-slate-900/95 p-3 text-sm shadow-xl">
              <div class="text-xs text-slate-500 font-mono mb-1">
                {s().id.slice(0, 8)}
              </div>
              <div class="font-semibold mb-1">
                {s().title || "(untitled)"}
              </div>
              <Show when={s().tags.length > 0}>
                <div class="flex flex-wrap gap-1">
                  <For each={s().tags}>
                    {(t) => (
                      <span class="rounded bg-slate-800 px-1.5 py-0.5 text-xs font-mono">
                        {t}
                      </span>
                    )}
                  </For>
                </div>
              </Show>
            </div>
          )}
        </Show>
      </div>
    </div>
  );
}
