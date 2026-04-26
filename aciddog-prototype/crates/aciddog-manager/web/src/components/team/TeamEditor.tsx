import {
  For,
  Match,
  Show,
  Switch,
  createEffect,
  createMemo,
  createSignal,
  onMount,
  type JSX,
} from "solid-js";
import { createStore, produce, type SetStoreFunction } from "solid-js/store";

import { api } from "../../api";
import type { RemoteSkillSummary } from "../../types";
import {
  AI_CORE_TYPES,
  AUTOMATON_TYPES,
  COMPACTION_STRATEGIES,
  WEBHOOK_AUTH_KINDS,
  emptyTeamConfig,
  type AgentState,
  type AiCoreState,
  type AiCoreType,
  type AutomatonState,
  type AutomatonType,
  type ChannelState,
  type CompactionState,
  type CompactionStrategy,
  type MemoryBankState,
  type TeamConfigState,
  type WebhookAuthKind,
  type WebhookState,
  type WorkerState,
  type WorkflowPropertyState,
  type WorkflowState,
  type WorkflowStepState,
} from "../../wcl/state";
import {
  EMBED_MODELS,
  USAGE_PROVIDERS,
  toolsForCoreType,
} from "../../wcl/catalogs";
import { serializeTeamConfig } from "../../wcl/serialize";
import { deserializeTeamConfig } from "../../wcl/deserialize";
import { validateWcl, type WclDiagnostic } from "../../wcl";
import {
  Badge,
  Button,
  Card,
  Checkbox,
  ChipPicker,
  Empty,
  Input,
  Select,
  Tabs,
  Textarea,
  Toast,
  type ChipOption,
  type SelectOption,
  type TabItem,
} from "../ui";

// ---------------------------------------------------------------------
// Top-level component
// ---------------------------------------------------------------------

interface Props {
  instanceId: string;
  instanceName: string;
  onClose: () => void;
}

type SectionId =
  | "team"
  | "ai_cores"
  | "agents"
  | "workers"
  | "automatons"
  | "channels"
  | "memory_banks"
  | "compactions"
  | "webhooks"
  | "workflows";

const SECTIONS: TabItem<SectionId>[] = [
  { id: "team", label: "Team" },
  { id: "ai_cores", label: "AI Cores" },
  { id: "agents", label: "Agents" },
  { id: "workers", label: "Workers" },
  { id: "automatons", label: "Automatons" },
  { id: "channels", label: "Channels" },
  { id: "memory_banks", label: "Memory Banks" },
  { id: "compactions", label: "Compactions" },
  { id: "webhooks", label: "Webhooks" },
  { id: "workflows", label: "Workflows" },
];

// Shared dependencies that every section needs. Built once at the top
// of TeamEditor so each tab can build dropdowns / chip pickers
// against the same authoritative lists (agent ids, workflow ids, the
// daemon's installed skills, etc.).
interface EditorCtx {
  config: TeamConfigState;
  setConfig: SetStoreFunction<TeamConfigState>;
  /** Skills installed on the daemon — used by the agents/workers tabs. */
  skills: () => RemoteSkillSummary[];
}

export default function TeamEditor(props: Props): JSX.Element {
  const [config, setConfig] = createStore<TeamConfigState>(emptyTeamConfig());
  const [wclText, setWclText] = createSignal("");
  const [diagnostics, setDiagnostics] = createSignal<WclDiagnostic[]>([]);
  const [section, setSection] = createSignal<SectionId>("team");
  const [loading, setLoading] = createSignal(true);
  const [loadError, setLoadError] = createSignal<string | null>(null);
  const [saving, setSaving] = createSignal(false);
  const [skills, setSkills] = createSignal<RemoteSkillSummary[]>([]);
  const [saveToast, setSaveToast] = createSignal<{
    tone: "success" | "danger";
    text: string;
  } | null>(null);
  const ctx: EditorCtx = { config, setConfig, skills };
  // Tracks which side last produced an update so the bidirectional
  // sync doesn't ping-pong.
  let lastSource: "form" | "wcl" | "load" = "load";
  let wclDebounce: number | null = null;

  const errorCount = () =>
    diagnostics().filter((d) => d.severity === "error").length;
  const canSave = () => !saving() && !loading() && errorCount() === 0;

  // Form -> WCL: re-serialise on every form change unless the change
  // came from the WCL pane (avoid the round-trip overwriting the
  // user's literal text while they type).
  createEffect(() => {
    // Subscribe to the whole store via JSON snapshot.
    JSON.stringify(config);
    if (lastSource === "wcl") return;
    const text = serializeTeamConfig(config);
    setWclText(text);
    void runValidate(text);
  });

  async function runValidate(source: string): Promise<void> {
    try {
      const r = await validateWcl(source);
      setDiagnostics(r.diagnostics);
    } catch (e) {
      setDiagnostics([
        {
          severity: "error",
          message: e instanceof Error ? e.message : String(e),
        },
      ]);
    }
  }

  function onWclEdit(text: string): void {
    lastSource = "wcl";
    setWclText(text);
    if (wclDebounce !== null) clearTimeout(wclDebounce);
    wclDebounce = window.setTimeout(() => {
      void (async () => {
        await runValidate(text);
        if (errorCount() === 0) {
          try {
            const next = await deserializeTeamConfig(text);
            setConfig(next);
          } catch {
            // Validation said ok but deserialise threw — leave the
            // form alone; the user can keep editing the WCL.
          }
        }
        lastSource = "form";
      })();
    }, 300);
  }

  async function load(): Promise<void> {
    setLoading(true);
    setLoadError(null);
    try {
      const resp = await api.readTeamWcl(props.instanceId);
      if (!resp.ok || !resp.wcl) {
        setLoadError(resp.error ?? "could not read team WCL");
        return;
      }
      lastSource = "load";
      const text = resp.wcl;
      const next = await deserializeTeamConfig(text);
      setConfig(next);
      setWclText(text);
      await runValidate(text);
      lastSource = "form";
    } catch (e) {
      setLoadError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  async function save(): Promise<void> {
    setSaving(true);
    setSaveToast(null);
    try {
      const resp = await api.writeTeamWcl(props.instanceId, wclText());
      if (resp.ok) {
        setSaveToast({
          tone: "success",
          text: resp.summary ?? "team saved + reloaded",
        });
      } else {
        setSaveToast({
          tone: "danger",
          text: resp.error ?? "save failed",
        });
      }
    } catch (e) {
      setSaveToast({
        tone: "danger",
        text: e instanceof Error ? e.message : String(e),
      });
    } finally {
      setSaving(false);
    }
  }

  async function loadSkills(): Promise<void> {
    try {
      const list = await api.listSkillsOnInstance(props.instanceId);
      setSkills(list);
    } catch {
      // Skills are an optional dependency for the editor — if the
      // daemon doesn't expose them (older build, transport hiccup) the
      // skills chip picker just shows an empty catalog.
      setSkills([]);
    }
  }

  onMount(() => {
    void load();
    void loadSkills();
  });

  return (
    <div
      style={{
        height: "100%",
        display: "flex",
        "flex-direction": "column",
        overflow: "hidden",
      }}
    >
      <header
        style={{
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
          padding: "16px 24px",
          "border-bottom": "1px solid var(--bg-3)",
          gap: "16px",
        }}
      >
        <div>
          <h2
            style={{
              "font-size": "20px",
              "font-weight": "600",
              color: "var(--fg-0)",
              margin: "0",
            }}
          >
            Edit team — {props.instanceName}
          </h2>
          <Show when={loading()}>
            <span
              style={{
                "font-size": "12px",
                color: "var(--fg-2)",
                "margin-top": "4px",
                display: "block",
              }}
            >
              loading WCL…
            </span>
          </Show>
        </div>
        <div style={{ display: "flex", gap: "8px" }}>
          <Button variant="ghost" size="sm" icon="refresh" onClick={load}>
            Reload from daemon
          </Button>
          <Button variant="ghost" size="sm" icon="x" onClick={props.onClose}>
            Close
          </Button>
          <Button
            variant="primary"
            size="sm"
            icon="check"
            onClick={save}
            disabled={!canSave()}
          >
            {saving() ? "Saving…" : "Save"}
          </Button>
        </div>
      </header>

      <Show when={loadError()}>
        <div style={{ padding: "12px 24px" }}>
          <Toast tone="danger">{loadError()}</Toast>
        </div>
      </Show>
      <Show when={saveToast()}>
        {(t) => (
          <div style={{ padding: "12px 24px" }}>
            <Toast tone={t().tone}>{t().text}</Toast>
          </div>
        )}
      </Show>

      <div
        style={{
          flex: "1",
          display: "flex",
          "flex-direction": "column",
          overflow: "hidden",
          "min-height": "0",
        }}
      >
        {/* Top: form region. Tabs stay pinned; the entity editor
            below them gets its own scrollable viewport so a tall agent
            / workflow card doesn't push the layout off-screen. */}
        <div
          style={{
            flex: "1 1 60%",
            display: "flex",
            "flex-direction": "column",
            "min-height": "0",
            overflow: "hidden",
          }}
        >
          <div
            style={{
              padding: "16px 24px 0",
              "flex-shrink": "0",
            }}
          >
            <Tabs<SectionId>
              items={SECTIONS}
              active={section()}
              onChange={setSection}
            />
          </div>
          <div
            style={{
              flex: "1",
              "min-height": "0",
              padding: "16px 24px",
              display: "flex",
              "flex-direction": "column",
              gap: "16px",
              overflow: "hidden",
            }}
          >
            <SectionPanel section={section()} ctx={ctx} />
          </div>
        </div>

        {/* Bottom: WCL pane */}
        <div
          style={{
            flex: "1 1 40%",
            display: "flex",
            "flex-direction": "column",
            "border-top": "1px solid var(--bg-3)",
            "min-height": "0",
            background: "var(--bg-1)",
          }}
        >
          <div
            style={{
              padding: "8px 24px",
              "border-bottom": "1px solid var(--bg-3)",
              display: "flex",
              "align-items": "center",
              "justify-content": "space-between",
            }}
          >
            <span
              style={{
                "font-size": "10px",
                "font-weight": "600",
                "letter-spacing": "0.08em",
                "text-transform": "uppercase",
                color: "var(--fg-2)",
              }}
            >
              WCL
            </span>
            <DiagnosticsSummary diagnostics={diagnostics()} />
          </div>
          <textarea
            spellcheck={false}
            value={wclText()}
            onInput={(e) => onWclEdit(e.currentTarget.value)}
            style={{
              flex: "1",
              "min-height": "0",
              padding: "12px 24px",
              border: "none",
              outline: "none",
              resize: "none",
              "font-family": "var(--font-mono)",
              "font-size": "12.5px",
              "line-height": "1.5",
              color: "var(--fg-0)",
              background: "transparent",
              "white-space": "pre",
            }}
          />
          <Show when={diagnostics().length > 0}>
            <div
              style={{
                "max-height": "120px",
                overflow: "auto",
                "border-top": "1px solid var(--bg-3)",
                padding: "8px 24px",
                "font-family": "var(--font-mono)",
                "font-size": "12px",
                background: "var(--bg-0)",
                display: "flex",
                "flex-direction": "column",
                gap: "4px",
              }}
            >
              <For each={diagnostics()}>
                {(d) => (
                  <div
                    style={{
                      color:
                        d.severity === "error"
                          ? "var(--tone-danger)"
                          : "var(--tone-warning)",
                    }}
                  >
                    {d.severity}
                    {d.code ? ` [${d.code}]` : ""}: {d.message}
                  </div>
                )}
              </For>
            </div>
          </Show>
        </div>
      </div>
    </div>
  );
}

function DiagnosticsSummary(props: { diagnostics: WclDiagnostic[] }): JSX.Element {
  const counts = createMemo(() => {
    let errors = 0;
    let warnings = 0;
    for (const d of props.diagnostics) {
      if (d.severity === "error") errors++;
      else warnings++;
    }
    return { errors, warnings };
  });
  return (
    <Show
      when={counts().errors > 0 || counts().warnings > 0}
      fallback={<Badge tone="success">valid</Badge>}
    >
      <div style={{ display: "flex", gap: "6px" }}>
        <Show when={counts().errors > 0}>
          <Badge tone="danger">{counts().errors} error(s)</Badge>
        </Show>
        <Show when={counts().warnings > 0}>
          <Badge tone="warning">{counts().warnings} warning(s)</Badge>
        </Show>
      </div>
    </Show>
  );
}

// ---------------------------------------------------------------------
// Section dispatch
// ---------------------------------------------------------------------

function SectionPanel(props: {
  section: SectionId;
  ctx: EditorCtx;
}): JSX.Element {
  return (
    <Switch>
      <Match when={props.section === "team"}>
        <TeamSection ctx={props.ctx} />
      </Match>
      <Match when={props.section === "ai_cores"}>
        <AiCoresSection ctx={props.ctx} />
      </Match>
      <Match when={props.section === "agents"}>
        <AgentsSection ctx={props.ctx} />
      </Match>
      <Match when={props.section === "workers"}>
        <WorkersSection ctx={props.ctx} />
      </Match>
      <Match when={props.section === "automatons"}>
        <AutomatonsSection ctx={props.ctx} />
      </Match>
      <Match when={props.section === "channels"}>
        <ChannelsSection ctx={props.ctx} />
      </Match>
      <Match when={props.section === "memory_banks"}>
        <MemoryBanksSection ctx={props.ctx} />
      </Match>
      <Match when={props.section === "compactions"}>
        <CompactionsSection ctx={props.ctx} />
      </Match>
      <Match when={props.section === "webhooks"}>
        <WebhooksSection ctx={props.ctx} />
      </Match>
      <Match when={props.section === "workflows"}>
        <WorkflowsSection ctx={props.ctx} />
      </Match>
    </Switch>
  );
}

interface SectionProps {
  ctx: EditorCtx;
}

// ---------------------------------------------------------------------
// Shared catalog helpers — derived from the live config.
// ---------------------------------------------------------------------

function aiCoreOptions(c: TeamConfigState): SelectOption[] {
  return c.ai_cores.map((x) => ({ value: x.id, label: x.id }));
}

function compactionOptions(c: TeamConfigState): SelectOption[] {
  return c.compactions.map((x) => ({ value: x.id, label: x.id }));
}

function memoryBankChips(c: TeamConfigState): ChipOption[] {
  return c.memory_banks.map((m) => ({ value: m.id, label: m.id }));
}

function workflowChips(c: TeamConfigState): ChipOption[] {
  return c.workflows.map((w) => ({ value: w.id, label: w.id }));
}

function workerChips(c: TeamConfigState): ChipOption[] {
  return c.workers.map((w) => ({ value: w.id, label: w.id }));
}

function agentChips(c: TeamConfigState): ChipOption[] {
  return c.agents.map((a) => ({ value: a.id, label: a.id }));
}

function automatonChips(c: TeamConfigState): ChipOption[] {
  return c.automatons.map((a) => ({ value: a.id, label: a.id }));
}

function channelMemberChips(c: TeamConfigState): ChipOption[] {
  // Channel members can be any agent or automaton.
  return [...agentChips(c), ...automatonChips(c)];
}

function canMessageChips(c: TeamConfigState, selfId?: string): ChipOption[] {
  // Anyone an agent / automaton can address: every other agent +
  // automaton, plus the human operator's "user" inbox.
  const out: ChipOption[] = [{ value: "user", label: "user" }];
  for (const a of c.agents) if (a.id !== selfId) out.push({ value: a.id, label: a.id });
  for (const a of c.automatons) if (a.id !== selfId) out.push({ value: a.id, label: a.id });
  return out;
}

function deliverToOptions(c: TeamConfigState): SelectOption[] {
  return [
    ...c.agents.map((a) => ({ value: a.id, label: `${a.id} (agent)` })),
    ...c.automatons.map((a) => ({ value: a.id, label: `${a.id} (automaton)` })),
  ];
}

function webserviceMountOptions(c: TeamConfigState): SelectOption[] {
  return c.automatons
    .filter((a) => a.type === "webservice")
    .map((a) => ({ value: a.id, label: a.id }));
}

function skillChips(skills: RemoteSkillSummary[]): ChipOption[] {
  return skills.map((s) => ({ value: s.name, label: s.name, hint: s.description }));
}

/** Auto-select the first item in `items()` whenever `selected()` is null. */
function useAutoSelectFirst<T extends { id: string }>(
  items: () => T[],
  selected: () => string | null,
  setSelected: (id: string) => void,
): void {
  createEffect(() => {
    const list = items();
    if (selected() === null && list.length > 0) {
      setSelected(list[0].id);
    }
  });
}

/**
 * Splits a section into a fixed-height "pinned" region (entity selector)
 * and a scrollable body (entity form). The form can be arbitrarily tall
 * without pushing the selector off-screen.
 */
function SectionLayout(props: {
  pinned?: JSX.Element;
  scrolling: JSX.Element;
}): JSX.Element {
  return (
    <div
      style={{
        flex: "1",
        display: "flex",
        "flex-direction": "column",
        "min-height": "0",
        overflow: "hidden",
      }}
    >
      <Show when={props.pinned}>
        <div style={{ "flex-shrink": "0", "margin-bottom": "12px" }}>
          {props.pinned}
        </div>
      </Show>
      <div
        style={{
          flex: "1",
          "min-height": "0",
          overflow: "auto",
          // A little right padding so the scrollbar doesn't sit on top
          // of card content.
          "padding-right": "4px",
        }}
      >
        {props.scrolling}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------
// Helpers shared across sections
// ---------------------------------------------------------------------

function listInput(value: string[] | undefined): string {
  return (value ?? []).join(", ");
}

function parseList(text: string): string[] | undefined {
  const parts = text
    .split(",")
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
  return parts.length > 0 ? parts : undefined;
}

function optionalString(text: string): string | undefined {
  return text.length > 0 ? text : undefined;
}

function optionalNumber(text: string): number | undefined {
  if (text.length === 0) return undefined;
  const n = Number(text);
  return Number.isFinite(n) ? n : undefined;
}

function entityList<T extends { id: string }>(props: {
  items: T[];
  selectedId: string | null;
  setSelected: (id: string | null) => void;
  onAdd: () => void;
  onRemove: (idx: number) => void;
  newLabel: string;
}): JSX.Element {
  return (
    <Card padded={true}>
      <div
        style={{
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
          "margin-bottom": "8px",
        }}
      >
        <span
          style={{
            "font-size": "10px",
            "font-weight": "600",
            "letter-spacing": "0.08em",
            "text-transform": "uppercase",
            color: "var(--fg-2)",
          }}
        >
          {props.items.length} item(s)
        </span>
        <Button variant="ghost" size="sm" icon="plus" onClick={props.onAdd}>
          {props.newLabel}
        </Button>
      </div>
      <Show
        when={props.items.length > 0}
        fallback={<Empty>No entries.</Empty>}
      >
        <div
          style={{
            display: "flex",
            "flex-wrap": "wrap",
            gap: "6px",
          }}
        >
          <For each={props.items}>
            {(item, idx) => (
              <div
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "4px",
                  padding: "4px 8px",
                  border:
                    props.selectedId === item.id
                      ? "1px solid var(--accent)"
                      : "1px solid var(--bg-3)",
                  "border-radius": "4px",
                  "font-family": "var(--font-mono)",
                  "font-size": "12px",
                  cursor: "pointer",
                  background:
                    props.selectedId === item.id
                      ? "var(--bg-2)"
                      : "transparent",
                }}
                onClick={() => props.setSelected(item.id)}
              >
                <span>{item.id}</span>
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    props.onRemove(idx());
                  }}
                  style={{
                    border: "none",
                    background: "transparent",
                    color: "var(--fg-3)",
                    cursor: "pointer",
                    padding: "0 2px",
                    "font-size": "14px",
                    "line-height": "1",
                  }}
                  aria-label={`Remove ${item.id}`}
                >
                  ×
                </button>
              </div>
            )}
          </For>
        </div>
      </Show>
    </Card>
  );
}

// Generates a fresh unique id like `prefix_1` for new entities.
function freshId(prefix: string, taken: string[]): string {
  let n = 1;
  let candidate = `${prefix}_${n}`;
  const set = new Set(taken);
  while (set.has(candidate)) {
    n++;
    candidate = `${prefix}_${n}`;
  }
  return candidate;
}

// ---------------------------------------------------------------------
// Team section
// ---------------------------------------------------------------------

function TeamSection(props: SectionProps): JSX.Element {
  const team = () => props.ctx.config.team;
  const cores = () => aiCoreOptions(props.ctx.config);
  return (
    <SectionLayout
      scrolling={
        <Card padded={true}>
          <div
            style={{
              display: "grid",
              "grid-template-columns": "repeat(2, minmax(0, 1fr))",
              gap: "12px",
            }}
          >
            <Input
              label="Team id"
              value={team().id}
              onInput={(e) =>
                props.ctx.setConfig("team", "id", e.currentTarget.value.trim())
              }
            />
            <Select<string>
              label="Default AI core"
              value={team().default_ai_core ?? ""}
              options={[{ value: "", label: "(none)" }, ...cores()]}
              onChange={(v) =>
                props.ctx.setConfig(
                  "team",
                  "default_ai_core",
                  v === "" ? undefined : v,
                )
              }
            />
            <div style={{ "grid-column": "1 / -1" }}>
              <Textarea
                label="Description"
                value={team().description}
                onInput={(e) =>
                  props.ctx.setConfig("team", "description", e.currentTarget.value)
                }
              />
            </div>
          </div>
        </Card>
      }
    />
  );
}

// ---------------------------------------------------------------------
// AI cores
// ---------------------------------------------------------------------

function AiCoresSection(props: SectionProps): JSX.Element {
  const [selected, setSelected] = createSignal<string | null>(null);

  const items = () => props.ctx.config.ai_cores;
  const selectedItem = () => items().find((c) => c.id === selected()) ?? null;
  const selectedIdx = () => items().findIndex((c) => c.id === selected());

  useAutoSelectFirst(items, selected, setSelected);

  const add = () => {
    const id = freshId("core", items().map((c) => c.id));
    const next: AiCoreState = {
      id,
      type: "claudeCode",
    };
    props.ctx.setConfig("ai_cores", (cs) => [...cs, next]);
    setSelected(id);
  };
  const remove = (idx: number) => {
    const id = items()[idx]?.id;
    props.ctx.setConfig("ai_cores", (cs) => cs.filter((_, i) => i !== idx));
    if (selected() === id) setSelected(null);
  };

  return (
    <SectionLayout
      pinned={entityList({
        items: items(),
        selectedId: selected(),
        setSelected,
        onAdd: add,
        onRemove: remove,
        newLabel: "Add ai_core",
      })}
      scrolling={
        <Show when={selectedItem()}>
          {(c) => (
            <AiCoreForm
              core={c()}
              onChange={(patch) =>
                props.ctx.setConfig(
                  "ai_cores",
                  selectedIdx(),
                  produce((draft) => {
                    Object.assign(draft, patch);
                  }),
                )
              }
              onTypeChange={(type) => {
                // Switching `type` rebuilds the variant-specific fields
                // from a clean slate to satisfy the discriminated union.
                const id = c().id;
                const usage = c().usage_provider;
                const replacement = freshAiCoreOfType(id, type, usage);
                props.ctx.setConfig("ai_cores", selectedIdx(), replacement);
              }}
              onIdChange={(id) => {
                props.ctx.setConfig("ai_cores", selectedIdx(), "id", id);
                setSelected(id);
              }}
            />
          )}
        </Show>
      }
    />
  );
}

function freshAiCoreOfType(
  id: string,
  type: AiCoreType,
  usage_provider?: AiCoreState["usage_provider"],
): AiCoreState {
  const base = { id, usage_provider } as const;
  switch (type) {
    case "claudeCode":
      return { ...base, type };
    case "anthropicApi":
      return {
        ...base,
        type,
        apiUrl: "",
        apiKeyEnv: "",
        model: "",
      };
    case "openAiApi":
      return {
        ...base,
        type,
        apiUrl: "",
        apiKeyEnv: "",
        model: "",
      };
    case "copilotCli":
      return { ...base, type };
    case "geminiCli":
      return { ...base, type };
    case "openAiChatgpt":
      return {
        ...base,
        type,
        credential: "",
        model: "",
      };
    case "jules":
      return { ...base, type, apiKeyEnv: "" };
  }
}

function AiCoreForm(props: {
  core: AiCoreState;
  onChange: (patch: Partial<AiCoreState>) => void;
  onTypeChange: (type: AiCoreType) => void;
  onIdChange: (id: string) => void;
}): JSX.Element {
  const c = () => props.core;
  return (
    <Card title={`ai_core ${c().id}`}>
      <div
        style={{
          display: "grid",
          "grid-template-columns": "repeat(2, minmax(0, 1fr))",
          gap: "12px",
        }}
      >
        <Input
          label="ID"
          value={c().id}
          onChange={(e) => props.onIdChange(e.currentTarget.value.trim())}
        />
        <Select<AiCoreType>
          label="Type"
          value={c().type}
          options={AI_CORE_TYPES.map((t) => ({ value: t, label: t }))}
          onChange={props.onTypeChange}
        />
        <Select<string>
          label="Usage provider (optional)"
          value={c().usage_provider ?? ""}
          options={[
            { value: "", label: "(none)" },
            ...USAGE_PROVIDERS.map((p) => ({ value: p, label: p })),
          ]}
          onChange={(v) =>
            props.onChange({
              usage_provider: (v === "" ? undefined : v) as AiCoreState["usage_provider"],
            })
          }
        />
        <Show when={c().type === "anthropicApi" || c().type === "openAiApi"}>
          <>
            <Input
              label="API URL"
              value={(c() as { apiUrl?: string }).apiUrl ?? ""}
              onInput={(e) => props.onChange({ apiUrl: e.currentTarget.value } as Partial<AiCoreState>)}
            />
            <Input
              label="API key env var"
              value={(c() as { apiKeyEnv?: string }).apiKeyEnv ?? ""}
              onInput={(e) => props.onChange({ apiKeyEnv: e.currentTarget.value } as Partial<AiCoreState>)}
            />
          </>
        </Show>
        <Show when={c().type === "openAiChatgpt"}>
          <Input
            label="Credential"
            value={(c() as { credential?: string }).credential ?? ""}
            onInput={(e) => props.onChange({ credential: e.currentTarget.value } as Partial<AiCoreState>)}
          />
        </Show>
        <Show when={c().type === "jules"}>
          <>
            <Input
              label="API key env"
              value={(c() as { apiKeyEnv?: string }).apiKeyEnv ?? ""}
              onInput={(e) => props.onChange({ apiKeyEnv: e.currentTarget.value } as Partial<AiCoreState>)}
            />
            <Input
              label="Default branch"
              value={(c() as { defaultBranch?: string }).defaultBranch ?? ""}
              onInput={(e) => props.onChange({ defaultBranch: optionalString(e.currentTarget.value) } as Partial<AiCoreState>)}
            />
            <Input
              type="number"
              label="Poll interval seconds"
              value={(c() as { pollIntervalSeconds?: number }).pollIntervalSeconds ?? ""}
              onInput={(e) =>
                props.onChange({
                  pollIntervalSeconds: optionalNumber(e.currentTarget.value),
                } as Partial<AiCoreState>)
              }
            />
            <Input
              label="Automation mode"
              value={(c() as { automationMode?: string }).automationMode ?? ""}
              onInput={(e) =>
                props.onChange({
                  automationMode: optionalString(e.currentTarget.value),
                } as Partial<AiCoreState>)
              }
            />
            <Checkbox
              label="Require plan approval"
              checked={(c() as { requirePlanApproval?: boolean }).requirePlanApproval ?? false}
              onChange={(v) =>
                props.onChange({
                  requirePlanApproval: v ? true : undefined,
                } as Partial<AiCoreState>)
              }
            />
          </>
        </Show>
        <Show
          when={
            c().type === "claudeCode" ||
            c().type === "anthropicApi" ||
            c().type === "openAiApi" ||
            c().type === "copilotCli" ||
            c().type === "geminiCli" ||
            c().type === "openAiChatgpt"
          }
        >
          <>
            <Input
              label="Model"
              value={(c() as { model?: string }).model ?? ""}
              onInput={(e) =>
                props.onChange({
                  model: optionalString(e.currentTarget.value),
                } as Partial<AiCoreState>)
              }
            />
            <ChipPicker
              label="Default tools"
              value={(c() as { default_tools?: string[] }).default_tools ?? []}
              options={toolsForCoreType(c().type).map((t) => ({ value: t, label: t }))}
              allowCustom={true}
              placeholder="Type to filter…"
              onChange={(next) =>
                props.onChange({
                  default_tools: next.length > 0 ? next : undefined,
                } as Partial<AiCoreState>)
              }
            />
            <Show
              when={
                c().type === "claudeCode" ||
                c().type === "anthropicApi" ||
                c().type === "openAiApi" ||
                c().type === "openAiChatgpt"
              }
            >
              <div style={{ "grid-column": "1 / -1" }}>
                <Textarea
                  label="System prompt"
                  value={(c() as { systemPrompt?: string }).systemPrompt ?? ""}
                  onInput={(e) =>
                    props.onChange({
                      systemPrompt: optionalString(e.currentTarget.value),
                    } as Partial<AiCoreState>)
                  }
                />
              </div>
            </Show>
          </>
        </Show>
        <Show when={c().type === "openAiChatgpt"}>
          <Input
            label="Reasoning effort"
            value={(c() as { reasoningEffort?: string }).reasoningEffort ?? ""}
            onInput={(e) =>
              props.onChange({
                reasoningEffort: optionalString(e.currentTarget.value),
              } as Partial<AiCoreState>)
            }
          />
        </Show>
      </div>
    </Card>
  );
}

// ---------------------------------------------------------------------
// Agents
// ---------------------------------------------------------------------

function AgentsSection(props: SectionProps): JSX.Element {
  const [selected, setSelected] = createSignal<string | null>(null);
  const items = () => props.ctx.config.agents;
  const selectedItem = () => items().find((a) => a.id === selected()) ?? null;
  const selectedIdx = () => items().findIndex((a) => a.id === selected());

  useAutoSelectFirst(items, selected, setSelected);

  const add = () => {
    const id = freshId("agent", items().map((a) => a.id));
    const next: AgentState = {
      id,
      role: "agent",
      instructions: "",
    };
    props.ctx.setConfig("agents", (xs) => [...xs, next]);
    setSelected(id);
  };
  const remove = (idx: number) => {
    const id = items()[idx]?.id;
    props.ctx.setConfig("agents", (xs) => xs.filter((_, i) => i !== idx));
    if (selected() === id) setSelected(null);
  };

  return (
    <SectionLayout
      pinned={entityList({
        items: items(),
        selectedId: selected(),
        setSelected,
        onAdd: add,
        onRemove: remove,
        newLabel: "Add agent",
      })}
      scrolling={
        <Show when={selectedItem()}>
          {(a) => (
            <AgentForm
              ctx={props.ctx}
              agent={a()}
              onPatch={(patch) =>
                props.ctx.setConfig(
                  "agents",
                  selectedIdx(),
                  produce((draft) => {
                    Object.assign(draft, patch);
                  }),
                )
              }
              onIdChange={(id) => {
                props.ctx.setConfig("agents", selectedIdx(), "id", id);
                setSelected(id);
              }}
            />
          )}
        </Show>
      }
    />
  );
}

function AgentForm(props: {
  ctx: EditorCtx;
  agent: AgentState;
  onPatch: (patch: Partial<AgentState>) => void;
  onIdChange: (id: string) => void;
}): JSX.Element {
  const a = () => props.agent;
  const cfg = () => props.ctx.config;
  // Determine which tools the agent's selected core supports — the
  // chip picker filters by core type so the user can't pick a tool the
  // backend would reject. Falls back to the full set when no core is
  // assigned.
  const coreType = (): AiCoreType | null => {
    const id = a().ai_core ?? cfg().team.default_ai_core;
    if (!id) return null;
    return cfg().ai_cores.find((c) => c.id === id)?.type ?? null;
  };
  const toolOpts = (): ChipOption[] => {
    const t = coreType();
    return (t ? toolsForCoreType(t) : []).map((x) => ({ value: x, label: x }));
  };
  return (
    <Card title={`agent ${a().id}`}>
      <div
        style={{
          display: "grid",
          "grid-template-columns": "repeat(2, minmax(0, 1fr))",
          gap: "12px",
        }}
      >
        <Input
          label="ID"
          value={a().id}
          onChange={(e) => props.onIdChange(e.currentTarget.value.trim())}
        />
        <Input
          label="Role"
          value={a().role}
          onInput={(e) => props.onPatch({ role: e.currentTarget.value })}
        />
        <Input
          label="Model (optional)"
          value={a().model ?? ""}
          onInput={(e) => props.onPatch({ model: optionalString(e.currentTarget.value) })}
        />
        <Select<string>
          label="AI core (optional)"
          value={a().ai_core ?? ""}
          options={[
            { value: "", label: "(team default)" },
            ...aiCoreOptions(cfg()),
          ]}
          onChange={(v) => props.onPatch({ ai_core: v === "" ? undefined : v })}
        />
        <div style={{ "grid-column": "1 / -1" }}>
          <Textarea
            label="Role description"
            value={a().role_description ?? ""}
            onInput={(e) =>
              props.onPatch({ role_description: optionalString(e.currentTarget.value) })
            }
          />
        </div>
        <div style={{ "grid-column": "1 / -1" }}>
          <Textarea
            label="Instructions"
            value={a().instructions}
            onInput={(e) => props.onPatch({ instructions: e.currentTarget.value })}
          />
        </div>
        <div style={{ "grid-column": "1 / -1" }}>
          <ChipPicker
            label="Can message"
            value={a().can_message ?? []}
            options={canMessageChips(cfg(), a().id)}
            allowCustom={true}
            placeholder="Search agents, automatons, or `user`…"
            onChange={(next) =>
              props.onPatch({ can_message: next.length > 0 ? next : undefined })
            }
          />
        </div>
        <ChipPicker
          label="Can create workers"
          value={a().can_create_workers ?? []}
          options={workerChips(cfg())}
          allowCustom={true}
          onChange={(next) =>
            props.onPatch({ can_create_workers: next.length > 0 ? next : undefined })
          }
        />
        <ChipPicker
          label="Can run workflows"
          value={a().can_run_workflows ?? []}
          options={workflowChips(cfg())}
          allowCustom={true}
          onChange={(next) =>
            props.onPatch({ can_run_workflows: next.length > 0 ? next : undefined })
          }
        />
        <ChipPicker
          label="Tools"
          value={a().tools ?? []}
          options={toolOpts()}
          allowCustom={true}
          onChange={(next) => props.onPatch({ tools: next.length > 0 ? next : undefined })}
        />
        <ChipPicker
          label="Memory banks"
          value={a().memory_banks ?? []}
          options={memoryBankChips(cfg())}
          allowCustom={true}
          onChange={(next) =>
            props.onPatch({ memory_banks: next.length > 0 ? next : undefined })
          }
        />
        <div style={{ "grid-column": "1 / -1" }}>
          <ChipPicker
            label="Skills"
            value={a().skills ?? []}
            options={skillChips(props.ctx.skills())}
            allowCustom={true}
            placeholder="Skills installed on this team's instance…"
            onChange={(next) =>
              props.onPatch({ skills: next.length > 0 ? next : undefined })
            }
          />
        </div>
        <Select<string>
          label="Compaction"
          value={a().compaction ?? ""}
          options={[
            { value: "", label: "(none)" },
            ...compactionOptions(cfg()),
          ]}
          onChange={(v) => props.onPatch({ compaction: v === "" ? undefined : v })}
        />
        <Select<"" | "ignore" | "compact" | "reset">
          label="On reload"
          value={a().on_reload ?? ""}
          options={[
            { value: "", label: "(default: ignore)" },
            { value: "ignore", label: "ignore" },
            { value: "compact", label: "compact" },
            { value: "reset", label: "reset" },
          ]}
          onChange={(v) =>
            props.onPatch({
              on_reload: v === "" ? undefined : (v as "ignore" | "compact" | "reset"),
            })
          }
        />
      </div>
      <RolesCard
        supervisor={a().supervisor ?? false}
        sysadmin={a().sysadmin ?? false}
        onSupervisor={(v) => props.onPatch({ supervisor: v ? true : undefined })}
        onSysadmin={(v) => props.onPatch({ sysadmin: v ? true : undefined })}
      />
    </Card>
  );
}

function RolesCard(props: {
  supervisor: boolean;
  sysadmin: boolean;
  onSupervisor: (v: boolean) => void;
  onSysadmin: (v: boolean) => void;
}): JSX.Element {
  return (
    <div
      style={{
        "margin-top": "16px",
        padding: "12px",
        border: "1px solid var(--border)",
        "border-radius": "var(--r-md)",
        background: "var(--bg-1)",
      }}
    >
      <span
        style={{
          display: "block",
          "font-size": "10px",
          "font-weight": "600",
          "letter-spacing": "0.08em",
          "text-transform": "uppercase",
          color: "var(--fg-2)",
          "margin-bottom": "8px",
        }}
      >
        Special roles
      </span>
      <div style={{ display: "flex", "flex-direction": "column", gap: "6px" }}>
        <Checkbox
          label="Supervisor"
          description="Unlocks fleet admin commands (agent list/stop, worker list, etc.) and bypasses can_message restrictions."
          checked={props.supervisor}
          onChange={props.onSupervisor}
        />
        <Checkbox
          label="Sysadmin"
          description="Adds server shutdown / exit / reload on top of supervisor; implies supervisor."
          checked={props.sysadmin}
          onChange={props.onSysadmin}
        />
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------
// Workers
// ---------------------------------------------------------------------

function WorkersSection(props: SectionProps): JSX.Element {
  const [selected, setSelected] = createSignal<string | null>(null);
  const items = () => props.ctx.config.workers;
  const selectedItem = () => items().find((w) => w.id === selected()) ?? null;
  const selectedIdx = () => items().findIndex((w) => w.id === selected());

  useAutoSelectFirst(items, selected, setSelected);

  const add = () => {
    const id = freshId("worker", items().map((w) => w.id));
    const next: WorkerState = { id, role: "worker", instructions: "" };
    props.ctx.setConfig("workers", (xs) => [...xs, next]);
    setSelected(id);
  };
  const remove = (idx: number) => {
    const id = items()[idx]?.id;
    props.ctx.setConfig("workers", (xs) => xs.filter((_, i) => i !== idx));
    if (selected() === id) setSelected(null);
  };

  return (
    <SectionLayout
      pinned={entityList({
        items: items(),
        selectedId: selected(),
        setSelected,
        onAdd: add,
        onRemove: remove,
        newLabel: "Add worker",
      })}
      scrolling={
        <Show when={selectedItem()}>
          {(w) => (
            <WorkerForm
              ctx={props.ctx}
              worker={w()}
              onPatch={(patch) =>
                props.ctx.setConfig(
                  "workers",
                  selectedIdx(),
                  produce((draft) => {
                    Object.assign(draft, patch);
                  }),
                )
              }
              onIdChange={(id) => {
                props.ctx.setConfig("workers", selectedIdx(), "id", id);
                setSelected(id);
              }}
            />
          )}
        </Show>
      }
    />
  );
}

function WorkerForm(props: {
  ctx: EditorCtx;
  worker: WorkerState;
  onPatch: (patch: Partial<WorkerState>) => void;
  onIdChange: (id: string) => void;
}): JSX.Element {
  const w = () => props.worker;
  const cfg = () => props.ctx.config;
  const coreType = (): AiCoreType | null => {
    const id = w().ai_core ?? cfg().team.default_ai_core;
    if (!id) return null;
    return cfg().ai_cores.find((c) => c.id === id)?.type ?? null;
  };
  const toolOpts = (): ChipOption[] => {
    const t = coreType();
    return (t ? toolsForCoreType(t) : []).map((x) => ({ value: x, label: x }));
  };
  return (
    <Card title={`worker ${w().id}`}>
      <div
        style={{
          display: "grid",
          "grid-template-columns": "repeat(2, minmax(0, 1fr))",
          gap: "12px",
        }}
      >
        <Input
          label="ID"
          value={w().id}
          onChange={(e) => props.onIdChange(e.currentTarget.value.trim())}
        />
        <Input
          label="Role"
          value={w().role}
          onInput={(e) => props.onPatch({ role: e.currentTarget.value })}
        />
        <Input
          label="Model (optional)"
          value={w().model ?? ""}
          onInput={(e) => props.onPatch({ model: optionalString(e.currentTarget.value) })}
        />
        <Select<string>
          label="AI core (optional)"
          value={w().ai_core ?? ""}
          options={[
            { value: "", label: "(team default)" },
            ...aiCoreOptions(cfg()),
          ]}
          onChange={(v) => props.onPatch({ ai_core: v === "" ? undefined : v })}
        />
        <div style={{ "grid-column": "1 / -1" }}>
          <Textarea
            label="Role description"
            value={w().role_description ?? ""}
            onInput={(e) =>
              props.onPatch({ role_description: optionalString(e.currentTarget.value) })
            }
          />
        </div>
        <div style={{ "grid-column": "1 / -1" }}>
          <Textarea
            label="Instructions"
            value={w().instructions}
            onInput={(e) => props.onPatch({ instructions: e.currentTarget.value })}
          />
        </div>
        <ChipPicker
          label="Tools"
          value={w().tools ?? []}
          options={toolOpts()}
          allowCustom={true}
          onChange={(next) => props.onPatch({ tools: next.length > 0 ? next : undefined })}
        />
        <ChipPicker
          label="Can create workers"
          value={w().can_create_workers ?? []}
          options={workerChips(cfg())}
          allowCustom={true}
          onChange={(next) =>
            props.onPatch({ can_create_workers: next.length > 0 ? next : undefined })
          }
        />
        <ChipPicker
          label="Can run workflows"
          value={w().can_run_workflows ?? []}
          options={workflowChips(cfg())}
          allowCustom={true}
          onChange={(next) =>
            props.onPatch({ can_run_workflows: next.length > 0 ? next : undefined })
          }
        />
        <ChipPicker
          label="Memory banks"
          value={w().memory_banks ?? []}
          options={memoryBankChips(cfg())}
          allowCustom={true}
          onChange={(next) =>
            props.onPatch({ memory_banks: next.length > 0 ? next : undefined })
          }
        />
        <div style={{ "grid-column": "1 / -1" }}>
          <ChipPicker
            label="Skills"
            value={w().skills ?? []}
            options={skillChips(props.ctx.skills())}
            allowCustom={true}
            placeholder="Skills installed on this team's instance…"
            onChange={(next) =>
              props.onPatch({ skills: next.length > 0 ? next : undefined })
            }
          />
        </div>
        <Select<string>
          label="Compaction"
          value={w().compaction ?? ""}
          options={[
            { value: "", label: "(none)" },
            ...compactionOptions(cfg()),
          ]}
          onChange={(v) => props.onPatch({ compaction: v === "" ? undefined : v })}
        />
      </div>
    </Card>
  );
}

// ---------------------------------------------------------------------
// Automatons
// ---------------------------------------------------------------------

function AutomatonsSection(props: SectionProps): JSX.Element {
  const [selected, setSelected] = createSignal<string | null>(null);
  const items = () => props.ctx.config.automatons;
  const selectedItem = () => items().find((a) => a.id === selected()) ?? null;
  const selectedIdx = () => items().findIndex((a) => a.id === selected());

  useAutoSelectFirst(items, selected, setSelected);

  const add = () => {
    const id = freshId("automaton", items().map((a) => a.id));
    const next: AutomatonState = {
      id,
      role: "automaton",
      type: "script",
      command: "",
    };
    props.ctx.setConfig("automatons", (xs) => [...xs, next]);
    setSelected(id);
  };
  const remove = (idx: number) => {
    const id = items()[idx]?.id;
    props.ctx.setConfig("automatons", (xs) => xs.filter((_, i) => i !== idx));
    if (selected() === id) setSelected(null);
  };

  return (
    <SectionLayout
      pinned={entityList({
        items: items(),
        selectedId: selected(),
        setSelected,
        onAdd: add,
        onRemove: remove,
        newLabel: "Add automaton",
      })}
      scrolling={
        <Show when={selectedItem()}>
          {(a) => (
            <AutomatonForm
              ctx={props.ctx}
              auto={a()}
              onPatch={(patch) =>
                props.ctx.setConfig(
                  "automatons",
                  selectedIdx(),
                  produce((draft) => {
                    Object.assign(draft, patch);
                  }),
                )
              }
              onTypeChange={(type) => {
                const cur = a();
                const replacement = freshAutomatonOfType(cur.id, cur.role, type);
                replacement.role_description = cur.role_description;
                replacement.can_message = cur.can_message;
                replacement.supervisor = cur.supervisor;
                replacement.sysadmin = cur.sysadmin;
                props.ctx.setConfig("automatons", selectedIdx(), replacement);
              }}
              onIdChange={(id) => {
                props.ctx.setConfig("automatons", selectedIdx(), "id", id);
                setSelected(id);
              }}
            />
          )}
        </Show>
      }
    />
  );
}

function freshAutomatonOfType(
  id: string,
  role: string,
  type: AutomatonType,
): AutomatonState {
  const base = { id, role };
  switch (type) {
    case "script":
      return { ...base, type, command: "" };
    case "webservice":
      return { ...base, type };
    case "remote":
      return { ...base, type, remote_target: "", deliver_to: "" };
  }
}

function AutomatonForm(props: {
  ctx: EditorCtx;
  auto: AutomatonState;
  onPatch: (patch: Partial<AutomatonState>) => void;
  onTypeChange: (type: AutomatonType) => void;
  onIdChange: (id: string) => void;
}): JSX.Element {
  const a = () => props.auto;
  return (
    <Card title={`automaton ${a().id}`}>
      <div
        style={{
          display: "grid",
          "grid-template-columns": "repeat(2, minmax(0, 1fr))",
          gap: "12px",
        }}
      >
        <Input
          label="ID"
          value={a().id}
          onChange={(e) => props.onIdChange(e.currentTarget.value.trim())}
        />
        <Select<AutomatonType>
          label="Type"
          value={a().type}
          options={AUTOMATON_TYPES.map((t) => ({ value: t, label: t }))}
          onChange={props.onTypeChange}
        />
        <Input
          label="Role"
          value={a().role}
          onInput={(e) => props.onPatch({ role: e.currentTarget.value } as Partial<AutomatonState>)}
        />
        <div style={{ "grid-column": "1 / -1" }}>
          <ChipPicker
            label="Can message"
            value={a().can_message ?? []}
            options={canMessageChips(props.ctx.config, a().id)}
            allowCustom={true}
            placeholder="Search agents, automatons, or `user`…"
            onChange={(next) =>
              props.onPatch({
                can_message: next.length > 0 ? next : undefined,
              } as Partial<AutomatonState>)
            }
          />
        </div>
        <div style={{ "grid-column": "1 / -1" }}>
          <Textarea
            label="Role description"
            value={a().role_description ?? ""}
            onInput={(e) =>
              props.onPatch({ role_description: optionalString(e.currentTarget.value) } as Partial<AutomatonState>)
            }
          />
        </div>
        <Show when={a().type === "script"}>
          <>
            <Input
              label="Command"
              value={(a() as { command?: string }).command ?? ""}
              onInput={(e) => props.onPatch({ command: e.currentTarget.value } as Partial<AutomatonState>)}
            />
            <Input
              label="Args (comma-separated)"
              value={listInput((a() as { args?: string[] }).args)}
              onInput={(e) =>
                props.onPatch({ args: parseList(e.currentTarget.value) } as Partial<AutomatonState>)
              }
            />
            <Input
              label="Working directory"
              value={(a() as { cwd?: string }).cwd ?? ""}
              onInput={(e) => props.onPatch({ cwd: optionalString(e.currentTarget.value) } as Partial<AutomatonState>)}
            />
            <Input
              label="Env (KEY=VAL, comma-separated)"
              value={listInput((a() as { env?: string[] }).env)}
              onInput={(e) => props.onPatch({ env: parseList(e.currentTarget.value) } as Partial<AutomatonState>)}
            />
            <Input
              type="number"
              label="Timeout seconds"
              value={(a() as { timeout_seconds?: number }).timeout_seconds ?? ""}
              onInput={(e) =>
                props.onPatch({ timeout_seconds: optionalNumber(e.currentTarget.value) } as Partial<AutomatonState>)
              }
            />
          </>
        </Show>
        <Show when={a().type === "webservice"}>
          <>
            <Input
              label="Bind"
              value={(a() as { bind?: string }).bind ?? ""}
              onInput={(e) => props.onPatch({ bind: optionalString(e.currentTarget.value) } as Partial<AutomatonState>)}
            />
            <Input
              label="Socket path"
              value={(a() as { socket_path?: string }).socket_path ?? ""}
              onInput={(e) => props.onPatch({ socket_path: optionalString(e.currentTarget.value) } as Partial<AutomatonState>)}
            />
            <Input
              label="TLS cert"
              value={(a() as { tls_cert?: string }).tls_cert ?? ""}
              onInput={(e) => props.onPatch({ tls_cert: optionalString(e.currentTarget.value) } as Partial<AutomatonState>)}
            />
            <Input
              label="TLS key"
              value={(a() as { tls_key?: string }).tls_key ?? ""}
              onInput={(e) => props.onPatch({ tls_key: optionalString(e.currentTarget.value) } as Partial<AutomatonState>)}
            />
            <Input
              label="API key env"
              value={(a() as { api_key_env?: string }).api_key_env ?? ""}
              onInput={(e) => props.onPatch({ api_key_env: optionalString(e.currentTarget.value) } as Partial<AutomatonState>)}
            />
          </>
        </Show>
        <Show when={a().type === "remote"}>
          <>
            <Input
              label="URL"
              value={(a() as { url?: string }).url ?? ""}
              onInput={(e) => props.onPatch({ url: optionalString(e.currentTarget.value) } as Partial<AutomatonState>)}
            />
            <Input
              label="Socket path"
              value={(a() as { socket_path?: string }).socket_path ?? ""}
              onInput={(e) => props.onPatch({ socket_path: optionalString(e.currentTarget.value) } as Partial<AutomatonState>)}
            />
            <Input
              label="API key env"
              value={(a() as { api_key_env?: string }).api_key_env ?? ""}
              onInput={(e) => props.onPatch({ api_key_env: optionalString(e.currentTarget.value) } as Partial<AutomatonState>)}
            />
            <Input
              label="TLS CA"
              value={(a() as { tls_ca?: string }).tls_ca ?? ""}
              onInput={(e) => props.onPatch({ tls_ca: optionalString(e.currentTarget.value) } as Partial<AutomatonState>)}
            />
            <Input
              label="Remote target"
              value={(a() as { remote_target?: string }).remote_target ?? ""}
              onInput={(e) => props.onPatch({ remote_target: e.currentTarget.value } as Partial<AutomatonState>)}
            />
            <Input
              label="Deliver to"
              value={(a() as { deliver_to?: string }).deliver_to ?? ""}
              onInput={(e) => props.onPatch({ deliver_to: e.currentTarget.value } as Partial<AutomatonState>)}
            />
            <Input
              label="Proxy socket path"
              value={(a() as { proxy_socket_path?: string }).proxy_socket_path ?? ""}
              onInput={(e) =>
                props.onPatch({ proxy_socket_path: optionalString(e.currentTarget.value) } as Partial<AutomatonState>)
              }
            />
          </>
        </Show>
      </div>
      <RolesCard
        supervisor={a().supervisor ?? false}
        sysadmin={a().sysadmin ?? false}
        onSupervisor={(v) =>
          props.onPatch({ supervisor: v ? true : undefined } as Partial<AutomatonState>)
        }
        onSysadmin={(v) =>
          props.onPatch({ sysadmin: v ? true : undefined } as Partial<AutomatonState>)
        }
      />
    </Card>
  );
}

// ---------------------------------------------------------------------
// Channels
// ---------------------------------------------------------------------

function ChannelsSection(props: SectionProps): JSX.Element {
  const [selected, setSelected] = createSignal<string | null>(null);
  const items = () => props.ctx.config.channels;
  const selectedItem = () => items().find((c) => c.id === selected()) ?? null;
  const selectedIdx = () => items().findIndex((c) => c.id === selected());

  useAutoSelectFirst(items, selected, setSelected);

  const add = () => {
    const id = freshId("channel", items().map((c) => c.id));
    const next: ChannelState = { id, members: [] };
    props.ctx.setConfig("channels", (xs) => [...xs, next]);
    setSelected(id);
  };
  const remove = (idx: number) => {
    const id = items()[idx]?.id;
    props.ctx.setConfig("channels", (xs) => xs.filter((_, i) => i !== idx));
    if (selected() === id) setSelected(null);
  };

  return (
    <SectionLayout
      pinned={entityList({
        items: items(),
        selectedId: selected(),
        setSelected,
        onAdd: add,
        onRemove: remove,
        newLabel: "Add channel",
      })}
      scrolling={
        <Show when={selectedItem()}>
          {(c) => {
            const v = c();
            return (
              <Card title={`channel ${v.id}`}>
                <div
                  style={{
                    display: "grid",
                    "grid-template-columns": "repeat(2, minmax(0, 1fr))",
                    gap: "12px",
                  }}
                >
                  <Input
                    label="ID"
                    value={v.id}
                    onChange={(e) => {
                      props.ctx.setConfig("channels", selectedIdx(), "id", e.currentTarget.value.trim());
                      setSelected(e.currentTarget.value.trim());
                    }}
                  />
                  <Input
                    label="Description"
                    value={v.description ?? ""}
                    onInput={(e) =>
                      props.ctx.setConfig(
                        "channels",
                        selectedIdx(),
                        "description",
                        optionalString(e.currentTarget.value),
                      )
                    }
                  />
                  <div style={{ "grid-column": "1 / -1" }}>
                    <Textarea
                      label="Welcome banner"
                      value={v.welcome ?? ""}
                      onInput={(e) =>
                        props.ctx.setConfig(
                          "channels",
                          selectedIdx(),
                          "welcome",
                          optionalString(e.currentTarget.value),
                        )
                      }
                    />
                  </div>
                  <div style={{ "grid-column": "1 / -1" }}>
                    <ChipPicker
                      label="Members"
                      value={v.members}
                      options={channelMemberChips(props.ctx.config)}
                      allowCustom={true}
                      placeholder="Search agents and automatons…"
                      onChange={(next) =>
                        props.ctx.setConfig("channels", selectedIdx(), "members", next)
                      }
                    />
                  </div>
                </div>
              </Card>
            );
          }}
        </Show>
      }
    />
  );
}

// ---------------------------------------------------------------------
// Memory banks
// ---------------------------------------------------------------------

function MemoryBanksSection(props: SectionProps): JSX.Element {
  const [selected, setSelected] = createSignal<string | null>(null);
  const items = () => props.ctx.config.memory_banks;
  const selectedItem = () => items().find((m) => m.id === selected()) ?? null;
  const selectedIdx = () => items().findIndex((m) => m.id === selected());

  useAutoSelectFirst(items, selected, setSelected);

  const add = () => {
    const id = freshId("memory_bank", items().map((m) => m.id));
    const next: MemoryBankState = { id, description: "" };
    props.ctx.setConfig("memory_banks", (xs) => [...xs, next]);
    setSelected(id);
  };
  const remove = (idx: number) => {
    const id = items()[idx]?.id;
    props.ctx.setConfig("memory_banks", (xs) => xs.filter((_, i) => i !== idx));
    if (selected() === id) setSelected(null);
  };

  return (
    <SectionLayout
      pinned={entityList({
        items: items(),
        selectedId: selected(),
        setSelected,
        onAdd: add,
        onRemove: remove,
        newLabel: "Add memory bank",
      })}
      scrolling={
        <Show when={selectedItem()}>
          {(m) => {
            const v = m();
            return (
              <Card title={`memory_bank ${v.id}`}>
                <div
                  style={{
                    display: "grid",
                    "grid-template-columns": "repeat(2, minmax(0, 1fr))",
                    gap: "12px",
                  }}
                >
                  <Input
                    label="ID"
                    value={v.id}
                    onChange={(e) => {
                      props.ctx.setConfig(
                        "memory_banks",
                        selectedIdx(),
                        "id",
                        e.currentTarget.value.trim(),
                      );
                      setSelected(e.currentTarget.value.trim());
                    }}
                  />
                  <Select<string>
                    label="Embed model"
                    value={v.embed_model ?? ""}
                    options={[
                      { value: "", label: "(default: BGE-small-en-v1.5)" },
                      ...EMBED_MODELS.map((m) => ({ value: m, label: m })),
                    ]}
                    onChange={(val) =>
                      props.ctx.setConfig(
                        "memory_banks",
                        selectedIdx(),
                        "embed_model",
                        val === "" ? undefined : val,
                      )
                    }
                  />
                  <div style={{ "grid-column": "1 / -1" }}>
                    <Textarea
                      label="Description"
                      value={v.description}
                      onInput={(e) =>
                        props.ctx.setConfig(
                          "memory_banks",
                          selectedIdx(),
                          "description",
                          e.currentTarget.value,
                        )
                      }
                    />
                  </div>
                </div>
              </Card>
            );
          }}
        </Show>
      }
    />
  );
}

// ---------------------------------------------------------------------
// Compactions
// ---------------------------------------------------------------------

function CompactionsSection(props: SectionProps): JSX.Element {
  const [selected, setSelected] = createSignal<string | null>(null);
  const items = () => props.ctx.config.compactions;
  const selectedItem = () => items().find((c) => c.id === selected()) ?? null;
  const selectedIdx = () => items().findIndex((c) => c.id === selected());

  useAutoSelectFirst(items, selected, setSelected);

  const add = () => {
    const id = freshId("compaction", items().map((c) => c.id));
    const next: CompactionState = { id, strategy: "harness" };
    props.ctx.setConfig("compactions", (xs) => [...xs, next]);
    setSelected(id);
  };
  const remove = (idx: number) => {
    const id = items()[idx]?.id;
    props.ctx.setConfig("compactions", (xs) => xs.filter((_, i) => i !== idx));
    if (selected() === id) setSelected(null);
  };

  return (
    <SectionLayout
      pinned={entityList({
        items: items(),
        selectedId: selected(),
        setSelected,
        onAdd: add,
        onRemove: remove,
        newLabel: "Add compaction",
      })}
      scrolling={
        <Show when={selectedItem()}>
          {(c) => {
            const v = c();
            const setStrategy = (strategy: CompactionStrategy) => {
              const id = v.id;
              const next: CompactionState =
                strategy === "harness" || strategy === "drop"
                  ? { id, strategy }
                  : strategy === "promptAndDrop"
                    ? { id, strategy, prompt: "" }
                    : { id, strategy, prompt: "" };
              props.ctx.setConfig("compactions", selectedIdx(), next);
            };
            return (
              <Card title={`compaction ${v.id}`}>
                <div
                  style={{
                    display: "grid",
                    "grid-template-columns": "repeat(2, minmax(0, 1fr))",
                    gap: "12px",
                  }}
                >
                  <Input
                    label="ID"
                    value={v.id}
                    onChange={(e) => {
                      props.ctx.setConfig(
                        "compactions",
                        selectedIdx(),
                        "id",
                        e.currentTarget.value.trim(),
                      );
                      setSelected(e.currentTarget.value.trim());
                    }}
                  />
                  <Select<CompactionStrategy>
                    label="Strategy"
                    value={v.strategy}
                    options={COMPACTION_STRATEGIES.map((s) => ({ value: s, label: s }))}
                    onChange={setStrategy}
                  />
                  <Show when={v.strategy === "promptAndDrop" || v.strategy === "summarise"}>
                    <div style={{ "grid-column": "1 / -1" }}>
                      <Textarea
                        label="Prompt"
                        value={(v as { prompt?: string }).prompt ?? ""}
                        onInput={(e) =>
                          props.ctx.setConfig(
                            "compactions",
                            selectedIdx(),
                            "prompt" as never,
                            e.currentTarget.value as never,
                          )
                        }
                      />
                    </div>
                  </Show>
                  <Show when={v.strategy === "summarise"}>
                    <Select<string>
                      label="AI core (optional)"
                      value={(v as { ai_core?: string }).ai_core ?? ""}
                      options={[
                        { value: "", label: "(team default)" },
                        ...aiCoreOptions(props.ctx.config),
                      ]}
                      onChange={(val) =>
                        props.ctx.setConfig(
                          "compactions",
                          selectedIdx(),
                          "ai_core" as never,
                          (val === "" ? undefined : val) as never,
                        )
                      }
                    />
                  </Show>
                </div>
              </Card>
            );
          }}
        </Show>
      }
    />
  );
}

// ---------------------------------------------------------------------
// Webhooks
// ---------------------------------------------------------------------

function WebhooksSection(props: SectionProps): JSX.Element {
  const [selected, setSelected] = createSignal<string | null>(null);
  const items = () => props.ctx.config.webhooks;
  const selectedItem = () => items().find((w) => w.id === selected()) ?? null;
  const selectedIdx = () => items().findIndex((w) => w.id === selected());

  useAutoSelectFirst(items, selected, setSelected);

  const add = () => {
    const id = freshId("webhook", items().map((w) => w.id));
    const next: WebhookState = {
      id,
      mount: "",
      path: "/hook",
      deliver_to: "",
    };
    props.ctx.setConfig("webhooks", (xs) => [...xs, next]);
    setSelected(id);
  };
  const remove = (idx: number) => {
    const id = items()[idx]?.id;
    props.ctx.setConfig("webhooks", (xs) => xs.filter((_, i) => i !== idx));
    if (selected() === id) setSelected(null);
  };

  return (
    <SectionLayout
      pinned={entityList({
        items: items(),
        selectedId: selected(),
        setSelected,
        onAdd: add,
        onRemove: remove,
        newLabel: "Add webhook",
      })}
      scrolling={
        <Show when={selectedItem()}>
          {(w) => {
            const v = w();
            const set = <K extends keyof WebhookState>(
              key: K,
              value: WebhookState[K],
            ) => {
              props.ctx.setConfig("webhooks", selectedIdx(), key, value);
            };
            return (
              <Card title={`webhook ${v.id}`}>
                <div
                  style={{
                    display: "grid",
                    "grid-template-columns": "repeat(2, minmax(0, 1fr))",
                    gap: "12px",
                  }}
                >
                  <Input
                    label="ID"
                    value={v.id}
                    onChange={(e) => {
                      set("id", e.currentTarget.value.trim());
                      setSelected(e.currentTarget.value.trim());
                    }}
                  />
                  <Select<string>
                    label="Mount"
                    value={v.mount}
                    options={[
                      { value: "", label: "(select a webservice automaton)" },
                      ...webserviceMountOptions(props.ctx.config),
                    ]}
                    onChange={(val) => set("mount", val)}
                    help="Only `:webservice` automatons can host webhooks."
                  />
                  <Input
                    label="Path"
                    value={v.path}
                    onInput={(e) => set("path", e.currentTarget.value)}
                  />
                  <Select<string>
                    label="Deliver to"
                    value={v.deliver_to}
                    options={[
                      { value: "", label: "(select an agent or automaton)" },
                      ...deliverToOptions(props.ctx.config),
                    ]}
                    onChange={(val) => set("deliver_to", val)}
                  />
                  <Input
                    type="number"
                    label="Max body bytes"
                    value={v.max_body_bytes ?? ""}
                    onInput={(e) =>
                      set("max_body_bytes", optionalNumber(e.currentTarget.value))
                    }
                  />
                  <Select<WebhookAuthKind>
                    label="Auth kind"
                    value={v.auth_kind ?? "none"}
                    options={WEBHOOK_AUTH_KINDS.map((k) => ({ value: k, label: k }))}
                    onChange={(k) => set("auth_kind", k)}
                  />
                  <Show
                    when={v.auth_kind === "header" || v.auth_kind === "hmac_header"}
                  >
                    <Input
                      label="Auth header"
                      value={v.auth_header ?? ""}
                      onInput={(e) =>
                        set("auth_header", optionalString(e.currentTarget.value))
                      }
                    />
                  </Show>
                  <Show when={v.auth_kind === "json_field"}>
                    <Input
                      label="Auth JSON field"
                      value={v.auth_field ?? ""}
                      onInput={(e) =>
                        set("auth_field", optionalString(e.currentTarget.value))
                      }
                    />
                  </Show>
                  <Show
                    when={
                      v.auth_kind === "bearer" ||
                      v.auth_kind === "header" ||
                      v.auth_kind === "json_field" ||
                      v.auth_kind === "hmac_header"
                    }
                  >
                    <Input
                      label="Auth secret env var"
                      value={v.auth_secret_env ?? ""}
                      onInput={(e) =>
                        set("auth_secret_env", optionalString(e.currentTarget.value))
                      }
                    />
                  </Show>
                  <Show when={v.auth_kind === "hmac_header"}>
                    <Select<"sha1" | "sha256">
                      label="HMAC algorithm"
                      value={v.auth_algo ?? "sha256"}
                      options={[
                        { value: "sha1", label: "sha1" },
                        { value: "sha256", label: "sha256" },
                      ]}
                      onChange={(a) => set("auth_algo", a)}
                    />
                  </Show>
                </div>
              </Card>
            );
          }}
        </Show>
      }
    />
  );
}

// ---------------------------------------------------------------------
// Workflows (simple textarea for property/step JSON for v1)
// ---------------------------------------------------------------------

function WorkflowsSection(props: SectionProps): JSX.Element {
  const [selected, setSelected] = createSignal<string | null>(null);
  const items = () => props.ctx.config.workflows;
  const selectedIdx = () => items().findIndex((w) => w.id === selected());

  useAutoSelectFirst(items, selected, setSelected);

  const add = () => {
    const id = freshId("workflow", items().map((w) => w.id));
    const next: WorkflowState = {
      id,
      description: "",
      properties: [],
      steps: [],
    };
    props.ctx.setConfig("workflows", (xs) => [...xs, next]);
    setSelected(id);
  };
  const remove = (idx: number) => {
    const id = items()[idx]?.id;
    props.ctx.setConfig("workflows", (xs) => xs.filter((_, i) => i !== idx));
    if (selected() === id) setSelected(null);
  };

  return (
    <SectionLayout
      pinned={entityList({
        items: items(),
        selectedId: selected(),
        setSelected,
        onAdd: add,
        onRemove: remove,
        newLabel: "Add workflow",
      })}
      scrolling={
        <Show when={selectedIdx() >= 0}>
          <WorkflowForm
            ctx={props.ctx}
            idx={selectedIdx()}
            onIdChange={(id) => setSelected(id)}
          />
        </Show>
      }
    />
  );
}

function WorkflowForm(props: {
  ctx: EditorCtx;
  idx: number;
  onIdChange: (id: string) => void;
}): JSX.Element {
  const w = (): WorkflowState => props.ctx.config.workflows[props.idx];
  const setW = <K extends keyof WorkflowState>(key: K, value: WorkflowState[K]) => {
    props.ctx.setConfig("workflows", props.idx, key, value);
  };
  return (
    <Card title={`workflow ${w().id}`}>
      <div
        style={{
          display: "grid",
          "grid-template-columns": "repeat(2, minmax(0, 1fr))",
          gap: "12px",
        }}
      >
        <Input
          label="ID"
          value={w().id}
          onChange={(e) => {
            const next = e.currentTarget.value.trim();
            setW("id", next);
            props.onIdChange(next);
          }}
        />
        <Input
          type="number"
          label="Max retries"
          value={w().max_retries ?? ""}
          onInput={(e) => setW("max_retries", optionalNumber(e.currentTarget.value))}
        />
        <div style={{ "grid-column": "1 / -1" }}>
          <Textarea
            label="Description"
            value={w().description}
            onInput={(e) => setW("description", e.currentTarget.value)}
          />
        </div>
      </div>

      <PropertiesEditor
        properties={w().properties}
        onAdd={() => {
          const id = freshId("prop", w().properties.map((p) => p.id));
          props.ctx.setConfig(
            "workflows",
            props.idx,
            "properties",
            (ps) => [...ps, { id, kind: "text" } as WorkflowPropertyState],
          );
        }}
        onRemove={(i) =>
          props.ctx.setConfig(
            "workflows",
            props.idx,
            "properties",
            (ps) => ps.filter((_, x) => x !== i),
          )
        }
        onMove={(i, dir) =>
          props.ctx.setConfig(
            "workflows",
            props.idx,
            "properties",
            (ps) => moveItem(ps, i, dir),
          )
        }
        onPatch={(i, patch) =>
          props.ctx.setConfig(
            "workflows",
            props.idx,
            "properties",
            i,
            produce((draft: WorkflowPropertyState) => {
              Object.assign(draft, patch);
            }),
          )
        }
        onReplace={(i, next) =>
          props.ctx.setConfig("workflows", props.idx, "properties", i, next)
        }
      />

      <StepsEditor
        ctx={props.ctx}
        steps={w().steps}
        properties={w().properties}
        onAdd={() => {
          const id = freshId("step", w().steps.map((s) => s.id));
          props.ctx.setConfig(
            "workflows",
            props.idx,
            "steps",
            (ss) => [...ss, { id, kind: "command", command: "" } as WorkflowStepState],
          );
        }}
        onRemove={(i) =>
          props.ctx.setConfig(
            "workflows",
            props.idx,
            "steps",
            (ss) => ss.filter((_, x) => x !== i),
          )
        }
        onMove={(i, dir) =>
          props.ctx.setConfig(
            "workflows",
            props.idx,
            "steps",
            (ss) => moveItem(ss, i, dir),
          )
        }
        onPatch={(i, patch) =>
          props.ctx.setConfig(
            "workflows",
            props.idx,
            "steps",
            i,
            produce((draft: WorkflowStepState) => {
              Object.assign(draft, patch);
            }),
          )
        }
        onReplace={(i, next) =>
          props.ctx.setConfig("workflows", props.idx, "steps", i, next)
        }
      />
    </Card>
  );
}

function moveItem<T>(arr: T[], i: number, dir: -1 | 1): T[] {
  const j = i + dir;
  if (j < 0 || j >= arr.length) return arr;
  const next = arr.slice();
  [next[i], next[j]] = [next[j], next[i]];
  return next;
}

const PROPERTY_KINDS = ["text", "number", "select", "multiSelect", "hidden"] as const;
const STEP_KINDS = ["command", "worker", "conditional", "check"] as const;
const CHECK_OPS = ["equals", "notEquals", "contains", "empty", "notEmpty"] as const;

function PropertiesEditor(props: {
  properties: WorkflowPropertyState[];
  onAdd: () => void;
  onRemove: (idx: number) => void;
  onMove: (idx: number, dir: -1 | 1) => void;
  onPatch: (idx: number, patch: Partial<WorkflowPropertyState>) => void;
  onReplace: (idx: number, next: WorkflowPropertyState) => void;
}): JSX.Element {
  return (
    <SubBlockSection
      title="Properties"
      count={props.properties.length}
      onAdd={props.onAdd}
      newLabel="Add property"
    >
      <For each={props.properties}>
        {(p, i) => (
          <SubBlockRow
            order={i()}
            total={props.properties.length}
            onUp={() => props.onMove(i(), -1)}
            onDown={() => props.onMove(i(), 1)}
            onRemove={() => props.onRemove(i())}
          >
            <div
              style={{
                display: "grid",
                "grid-template-columns": "repeat(3, minmax(0, 1fr))",
                gap: "8px",
              }}
            >
              <Input
                label="ID"
                value={p.id}
                onChange={(e) => props.onPatch(i(), { id: e.currentTarget.value.trim() })}
              />
              <Select<(typeof PROPERTY_KINDS)[number]>
                label="Kind"
                value={p.kind}
                options={PROPERTY_KINDS.map((k) => ({ value: k, label: k }))}
                onChange={(k) => {
                  // Reset variant-specific fields on kind change.
                  const next: WorkflowPropertyState = {
                    id: p.id,
                    kind: k,
                    label: p.label,
                    required: p.required,
                  };
                  props.onReplace(i(), next);
                }}
              />
              <Input
                label="Label"
                value={p.label ?? ""}
                onInput={(e) => props.onPatch(i(), { label: optionalString(e.currentTarget.value) })}
              />
              <Show when={p.kind === "number"}>
                <>
                  <Input
                    type="number"
                    label="Min"
                    value={p.min ?? ""}
                    onInput={(e) => props.onPatch(i(), { min: optionalNumber(e.currentTarget.value) })}
                  />
                  <Input
                    type="number"
                    label="Max"
                    value={p.max ?? ""}
                    onInput={(e) => props.onPatch(i(), { max: optionalNumber(e.currentTarget.value) })}
                  />
                </>
              </Show>
              <Show when={p.kind === "select" || p.kind === "multiSelect"}>
                <div style={{ "grid-column": "1 / -1" }}>
                  <ChipPicker
                    label="Options"
                    value={p.options ?? []}
                    options={(p.options ?? []).map((o) => ({ value: o, label: o }))}
                    allowCustom={true}
                    placeholder="Type and press Enter…"
                    onChange={(next) =>
                      props.onPatch(i(), { options: next.length > 0 ? next : undefined })
                    }
                  />
                </div>
              </Show>
              <Show when={p.kind === "multiSelect"}>
                <div style={{ "grid-column": "1 / -1" }}>
                  <ChipPicker
                    label="Default values"
                    value={Array.isArray(p.default) ? p.default : []}
                    options={(p.options ?? []).map((o) => ({ value: o, label: o }))}
                    onChange={(next) =>
                      props.onPatch(i(), { default: next.length > 0 ? next : undefined })
                    }
                  />
                </div>
              </Show>
              <Show when={p.kind !== "multiSelect"}>
                <Input
                  label={p.kind === "number" ? "Default (number)" : "Default"}
                  value={
                    p.default === undefined
                      ? ""
                      : Array.isArray(p.default)
                        ? p.default.join(", ")
                        : String(p.default)
                  }
                  onInput={(e) => {
                    const raw = e.currentTarget.value;
                    if (p.kind === "number") {
                      props.onPatch(i(), { default: optionalNumber(raw) });
                    } else {
                      props.onPatch(i(), { default: optionalString(raw) });
                    }
                  }}
                />
              </Show>
              <Checkbox
                label="Required"
                checked={p.required ?? false}
                onChange={(v) => props.onPatch(i(), { required: v ? true : undefined })}
              />
            </div>
          </SubBlockRow>
        )}
      </For>
    </SubBlockSection>
  );
}

function StepsEditor(props: {
  ctx: EditorCtx;
  steps: WorkflowStepState[];
  properties: WorkflowPropertyState[];
  onAdd: () => void;
  onRemove: (idx: number) => void;
  onMove: (idx: number, dir: -1 | 1) => void;
  onPatch: (idx: number, patch: Partial<WorkflowStepState>) => void;
  onReplace: (idx: number, next: WorkflowStepState) => void;
}): JSX.Element {
  const stepIdOptions = (): SelectOption[] =>
    props.steps.map((s) => ({ value: s.id, label: s.id }));
  const propertyChips = (): ChipOption[] =>
    props.properties.map((p) => ({ value: p.id, label: p.id }));
  const _ = propertyChips; // surface for future hint chips
  void _;
  return (
    <SubBlockSection
      title="Steps"
      count={props.steps.length}
      onAdd={props.onAdd}
      newLabel="Add step"
    >
      <For each={props.steps}>
        {(s, i) => (
          <SubBlockRow
            order={i()}
            total={props.steps.length}
            onUp={() => props.onMove(i(), -1)}
            onDown={() => props.onMove(i(), 1)}
            onRemove={() => props.onRemove(i())}
          >
            <div
              style={{
                display: "grid",
                "grid-template-columns": "repeat(2, minmax(0, 1fr))",
                gap: "8px",
              }}
            >
              <Input
                label="ID"
                value={s.id}
                onChange={(e) => props.onPatch(i(), { id: e.currentTarget.value.trim() })}
              />
              <Select<(typeof STEP_KINDS)[number]>
                label="Kind"
                value={s.kind}
                options={STEP_KINDS.map((k) => ({ value: k, label: k }))}
                onChange={(k) => {
                  // Reset variant fields on kind change.
                  const next: WorkflowStepState = { id: s.id, kind: k };
                  if (k === "command") next.command = "";
                  if (k === "worker") {
                    next.worker = "";
                    next.prompt = "";
                  }
                  if (k === "conditional") {
                    next.worker = "";
                    next.prompt = "";
                    next.target = "";
                  }
                  if (k === "check") {
                    next.variable = "";
                    next.op = "equals";
                  }
                  props.onReplace(i(), next);
                }}
              />
              <Show when={s.kind === "command"}>
                <>
                  <div style={{ "grid-column": "1 / -1" }}>
                    <Input
                      label="Command"
                      value={s.command ?? ""}
                      onInput={(e) => props.onPatch(i(), { command: e.currentTarget.value })}
                    />
                  </div>
                  <Input
                    label="Working directory (optional)"
                    value={s.cwd ?? ""}
                    onInput={(e) => props.onPatch(i(), { cwd: optionalString(e.currentTarget.value) })}
                  />
                </>
              </Show>
              <Show when={s.kind === "worker" || s.kind === "conditional"}>
                <Select<string>
                  label="Worker"
                  value={s.worker ?? ""}
                  options={[
                    { value: "", label: "(select worker)" },
                    ...workerChips(props.ctx.config).map((c) => ({
                      value: c.value,
                      label: c.value,
                    })),
                  ]}
                  onChange={(v) => props.onPatch(i(), { worker: v === "" ? undefined : v })}
                />
                <div style={{ "grid-column": "1 / -1" }}>
                  <Textarea
                    label="Prompt"
                    value={s.prompt ?? ""}
                    onInput={(e) => props.onPatch(i(), { prompt: e.currentTarget.value })}
                  />
                </div>
              </Show>
              <Show when={s.kind === "check"}>
                <>
                  <Select<string>
                    label="Variable"
                    value={s.variable ?? ""}
                    options={[
                      { value: "", label: "(pick a workflow property)" },
                      ...props.properties.map((p) => ({ value: p.id, label: p.id })),
                    ]}
                    onChange={(v) => props.onPatch(i(), { variable: v === "" ? undefined : v })}
                  />
                  <Select<(typeof CHECK_OPS)[number]>
                    label="Op"
                    value={s.op ?? "equals"}
                    options={CHECK_OPS.map((o) => ({ value: o, label: o }))}
                    onChange={(o) => props.onPatch(i(), { op: o })}
                  />
                  <Show when={s.op !== "empty" && s.op !== "notEmpty"}>
                    <Input
                      label="Value"
                      value={s.value ?? ""}
                      onInput={(e) => props.onPatch(i(), { value: optionalString(e.currentTarget.value) })}
                    />
                  </Show>
                </>
              </Show>
              <Show when={s.kind === "conditional" || s.kind === "check"}>
                <Select<string>
                  label="Jump target (step id)"
                  value={s.target ?? ""}
                  options={[
                    { value: "", label: "(none)" },
                    ...stepIdOptions(),
                  ]}
                  onChange={(v) => props.onPatch(i(), { target: v === "" ? undefined : v })}
                />
              </Show>
            </div>
          </SubBlockRow>
        )}
      </For>
    </SubBlockSection>
  );
}

function SubBlockSection(props: {
  title: string;
  count: number;
  onAdd: () => void;
  newLabel: string;
  children: JSX.Element;
}): JSX.Element {
  return (
    <div
      style={{
        "margin-top": "16px",
        padding: "12px",
        border: "1px solid var(--border)",
        "border-radius": "var(--r-md)",
        background: "var(--bg-1)",
      }}
    >
      <div
        style={{
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
          "margin-bottom": "8px",
        }}
      >
        <span
          style={{
            "font-size": "10px",
            "font-weight": "600",
            "letter-spacing": "0.08em",
            "text-transform": "uppercase",
            color: "var(--fg-2)",
          }}
        >
          {props.title} · {props.count}
        </span>
        <Button variant="ghost" size="sm" icon="plus" onClick={props.onAdd}>
          {props.newLabel}
        </Button>
      </div>
      <Show when={props.count > 0} fallback={<Empty>No entries.</Empty>}>
        <div style={{ display: "flex", "flex-direction": "column", gap: "10px" }}>
          {props.children}
        </div>
      </Show>
    </div>
  );
}

function SubBlockRow(props: {
  order: number;
  total: number;
  onUp: () => void;
  onDown: () => void;
  onRemove: () => void;
  children: JSX.Element;
}): JSX.Element {
  return (
    <div
      style={{
        display: "grid",
        "grid-template-columns": "auto 1fr auto",
        gap: "8px",
        "align-items": "start",
        padding: "8px",
        border: "1px solid var(--border-subtle)",
        "border-radius": "var(--r-md)",
        background: "var(--bg-0)",
      }}
    >
      <div
        style={{
          display: "flex",
          "flex-direction": "column",
          gap: "4px",
          "align-items": "center",
        }}
      >
        <span
          style={{
            "font-family": "var(--font-mono)",
            "font-size": "11px",
            color: "var(--fg-2)",
          }}
        >
          {props.order + 1}
        </span>
        <button
          type="button"
          class="fbtn fbtn-ghost fbtn-sm"
          onClick={props.onUp}
          disabled={props.order === 0}
          aria-label="Move up"
          style={{ padding: "2px 4px" }}
        >
          ↑
        </button>
        <button
          type="button"
          class="fbtn fbtn-ghost fbtn-sm"
          onClick={props.onDown}
          disabled={props.order >= props.total - 1}
          aria-label="Move down"
          style={{ padding: "2px 4px" }}
        >
          ↓
        </button>
      </div>
      <div>{props.children}</div>
      <button
        type="button"
        onClick={props.onRemove}
        aria-label="Remove"
        style={{
          border: "none",
          background: "transparent",
          color: "var(--fg-3)",
          cursor: "pointer",
          padding: "0 4px",
          "font-size": "16px",
          "line-height": "1",
        }}
      >
        ×
      </button>
    </div>
  );
}
