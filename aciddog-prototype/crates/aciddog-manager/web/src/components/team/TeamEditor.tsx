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
  type WorkflowState,
} from "../../wcl/state";
import { serializeTeamConfig } from "../../wcl/serialize";
import { deserializeTeamConfig } from "../../wcl/deserialize";
import { validateWcl, type WclDiagnostic } from "../../wcl";
import {
  Badge,
  Button,
  Card,
  Checkbox,
  Empty,
  Input,
  Select,
  Tabs,
  Textarea,
  Toast,
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

export default function TeamEditor(props: Props): JSX.Element {
  const [config, setConfig] = createStore<TeamConfigState>(emptyTeamConfig());
  const [wclText, setWclText] = createSignal("");
  const [diagnostics, setDiagnostics] = createSignal<WclDiagnostic[]>([]);
  const [section, setSection] = createSignal<SectionId>("team");
  const [loading, setLoading] = createSignal(true);
  const [loadError, setLoadError] = createSignal<string | null>(null);
  const [saving, setSaving] = createSignal(false);
  const [saveToast, setSaveToast] = createSignal<{
    tone: "success" | "danger";
    text: string;
  } | null>(null);
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

  onMount(() => {
    void load();
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
              overflow: "auto",
              padding: "16px 24px",
              display: "flex",
              "flex-direction": "column",
              gap: "16px",
            }}
          >
            <SectionPanel
              section={section()}
              config={config}
              setConfig={setConfig}
            />
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
  config: TeamConfigState;
  setConfig: SetStoreFunction<TeamConfigState>;
}): JSX.Element {
  return (
    <Switch>
      <Match when={props.section === "team"}>
        <TeamSection config={props.config} setConfig={props.setConfig} />
      </Match>
      <Match when={props.section === "ai_cores"}>
        <AiCoresSection config={props.config} setConfig={props.setConfig} />
      </Match>
      <Match when={props.section === "agents"}>
        <AgentsSection config={props.config} setConfig={props.setConfig} />
      </Match>
      <Match when={props.section === "workers"}>
        <WorkersSection config={props.config} setConfig={props.setConfig} />
      </Match>
      <Match when={props.section === "automatons"}>
        <AutomatonsSection config={props.config} setConfig={props.setConfig} />
      </Match>
      <Match when={props.section === "channels"}>
        <ChannelsSection config={props.config} setConfig={props.setConfig} />
      </Match>
      <Match when={props.section === "memory_banks"}>
        <MemoryBanksSection config={props.config} setConfig={props.setConfig} />
      </Match>
      <Match when={props.section === "compactions"}>
        <CompactionsSection config={props.config} setConfig={props.setConfig} />
      </Match>
      <Match when={props.section === "webhooks"}>
        <WebhooksSection config={props.config} setConfig={props.setConfig} />
      </Match>
      <Match when={props.section === "workflows"}>
        <WorkflowsSection config={props.config} setConfig={props.setConfig} />
      </Match>
    </Switch>
  );
}

interface SectionProps {
  config: TeamConfigState;
  setConfig: SetStoreFunction<TeamConfigState>;
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
  const team = () => props.config.team;
  return (
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
            props.setConfig("team", "id", e.currentTarget.value.trim())
          }
        />
        <Input
          label="Default AI core"
          value={team().default_ai_core ?? ""}
          onInput={(e) =>
            props.setConfig(
              "team",
              "default_ai_core",
              optionalString(e.currentTarget.value.trim()),
            )
          }
        />
        <div style={{ "grid-column": "1 / -1" }}>
          <Textarea
            label="Description"
            value={team().description}
            onInput={(e) =>
              props.setConfig("team", "description", e.currentTarget.value)
            }
          />
        </div>
        <Checkbox
          label="Replace built-in thinking messages"
          checked={team().replace_thinking_messages ?? false}
          onChange={(v) =>
            props.setConfig(
              "team",
              "replace_thinking_messages",
              v ? true : undefined,
            )
          }
        />
      </div>
    </Card>
  );
}

// ---------------------------------------------------------------------
// AI cores
// ---------------------------------------------------------------------

function AiCoresSection(props: SectionProps): JSX.Element {
  const [selected, setSelected] = createSignal<string | null>(null);

  const items = () => props.config.ai_cores;
  const selectedItem = () => items().find((c) => c.id === selected()) ?? null;
  const selectedIdx = () => items().findIndex((c) => c.id === selected());

  const add = () => {
    const id = freshId("core", items().map((c) => c.id));
    const next: AiCoreState = {
      id,
      type: "claudeCode",
    };
    props.setConfig("ai_cores", (cs) => [...cs, next]);
    setSelected(id);
  };
  const remove = (idx: number) => {
    const id = items()[idx]?.id;
    props.setConfig("ai_cores", (cs) => cs.filter((_, i) => i !== idx));
    if (selected() === id) setSelected(null);
  };

  return (
    <>
      {entityList({
        items: items(),
        selectedId: selected(),
        setSelected,
        onAdd: add,
        onRemove: remove,
        newLabel: "Add ai_core",
      })}
      <Show when={selectedItem()}>
        {(c) => (
          <AiCoreForm
            core={c()}
            onChange={(patch) =>
              props.setConfig(
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
              props.setConfig("ai_cores", selectedIdx(), replacement);
            }}
            onIdChange={(id) => {
              props.setConfig("ai_cores", selectedIdx(), "id", id);
              setSelected(id);
            }}
          />
        )}
      </Show>
    </>
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
        <Input
          label="Usage provider (optional)"
          value={c().usage_provider ?? ""}
          onInput={(e) =>
            props.onChange({
              usage_provider: optionalString(
                e.currentTarget.value.trim(),
              ) as AiCoreState["usage_provider"],
            })
          }
          help=":zai | :chatgpt_oauth | :openrouter | :github_copilot"
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
            <Input
              label="Default tools (comma-separated)"
              value={listInput((c() as { default_tools?: string[] }).default_tools)}
              onInput={(e) =>
                props.onChange({
                  default_tools: parseList(e.currentTarget.value),
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
  const items = () => props.config.agents;
  const selectedItem = () => items().find((a) => a.id === selected()) ?? null;
  const selectedIdx = () => items().findIndex((a) => a.id === selected());

  const add = () => {
    const id = freshId("agent", items().map((a) => a.id));
    const next: AgentState = {
      id,
      role: "agent",
      instructions: "",
    };
    props.setConfig("agents", (xs) => [...xs, next]);
    setSelected(id);
  };
  const remove = (idx: number) => {
    const id = items()[idx]?.id;
    props.setConfig("agents", (xs) => xs.filter((_, i) => i !== idx));
    if (selected() === id) setSelected(null);
  };

  return (
    <>
      {entityList({
        items: items(),
        selectedId: selected(),
        setSelected,
        onAdd: add,
        onRemove: remove,
        newLabel: "Add agent",
      })}
      <Show when={selectedItem()}>
        {(a) => (
          <AgentForm
            agent={a()}
            onPatch={(patch) =>
              props.setConfig(
                "agents",
                selectedIdx(),
                produce((draft) => {
                  Object.assign(draft, patch);
                }),
              )
            }
            onIdChange={(id) => {
              props.setConfig("agents", selectedIdx(), "id", id);
              setSelected(id);
            }}
          />
        )}
      </Show>
    </>
  );
}

function AgentForm(props: {
  agent: AgentState;
  onPatch: (patch: Partial<AgentState>) => void;
  onIdChange: (id: string) => void;
}): JSX.Element {
  const a = () => props.agent;
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
        <Input
          label="AI core (optional)"
          value={a().ai_core ?? ""}
          onInput={(e) => props.onPatch({ ai_core: optionalString(e.currentTarget.value) })}
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
        <Input
          label="Can message (comma-separated agent ids)"
          value={listInput(a().can_message)}
          onInput={(e) => props.onPatch({ can_message: parseList(e.currentTarget.value) })}
        />
        <Input
          label="Can create workers"
          value={listInput(a().can_create_workers)}
          onInput={(e) =>
            props.onPatch({ can_create_workers: parseList(e.currentTarget.value) })
          }
        />
        <Input
          label="Can run workflows"
          value={listInput(a().can_run_workflows)}
          onInput={(e) =>
            props.onPatch({ can_run_workflows: parseList(e.currentTarget.value) })
          }
        />
        <Input
          label="Tools"
          value={listInput(a().tools)}
          onInput={(e) => props.onPatch({ tools: parseList(e.currentTarget.value) })}
        />
        <Input
          label="Memory banks"
          value={listInput(a().memory_banks)}
          onInput={(e) =>
            props.onPatch({ memory_banks: parseList(e.currentTarget.value) })
          }
        />
        <Input
          label="Skills"
          value={listInput(a().skills)}
          onInput={(e) => props.onPatch({ skills: parseList(e.currentTarget.value) })}
        />
        <Input
          label="Compaction"
          value={a().compaction ?? ""}
          onInput={(e) =>
            props.onPatch({ compaction: optionalString(e.currentTarget.value) })
          }
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
        <Checkbox
          label="Supervisor"
          checked={a().supervisor ?? false}
          onChange={(v) => props.onPatch({ supervisor: v ? true : undefined })}
        />
        <Checkbox
          label="Sysadmin"
          checked={a().sysadmin ?? false}
          onChange={(v) => props.onPatch({ sysadmin: v ? true : undefined })}
        />
      </div>
    </Card>
  );
}

// ---------------------------------------------------------------------
// Workers
// ---------------------------------------------------------------------

function WorkersSection(props: SectionProps): JSX.Element {
  const [selected, setSelected] = createSignal<string | null>(null);
  const items = () => props.config.workers;
  const selectedItem = () => items().find((w) => w.id === selected()) ?? null;
  const selectedIdx = () => items().findIndex((w) => w.id === selected());

  const add = () => {
    const id = freshId("worker", items().map((w) => w.id));
    const next: WorkerState = { id, role: "worker", instructions: "" };
    props.setConfig("workers", (xs) => [...xs, next]);
    setSelected(id);
  };
  const remove = (idx: number) => {
    const id = items()[idx]?.id;
    props.setConfig("workers", (xs) => xs.filter((_, i) => i !== idx));
    if (selected() === id) setSelected(null);
  };

  return (
    <>
      {entityList({
        items: items(),
        selectedId: selected(),
        setSelected,
        onAdd: add,
        onRemove: remove,
        newLabel: "Add worker",
      })}
      <Show when={selectedItem()}>
        {(w) => (
          <WorkerForm
            worker={w()}
            onPatch={(patch) =>
              props.setConfig(
                "workers",
                selectedIdx(),
                produce((draft) => {
                  Object.assign(draft, patch);
                }),
              )
            }
            onIdChange={(id) => {
              props.setConfig("workers", selectedIdx(), "id", id);
              setSelected(id);
            }}
          />
        )}
      </Show>
    </>
  );
}

function WorkerForm(props: {
  worker: WorkerState;
  onPatch: (patch: Partial<WorkerState>) => void;
  onIdChange: (id: string) => void;
}): JSX.Element {
  const w = () => props.worker;
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
        <Input
          label="AI core (optional)"
          value={w().ai_core ?? ""}
          onInput={(e) => props.onPatch({ ai_core: optionalString(e.currentTarget.value) })}
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
        <Input
          label="Tools"
          value={listInput(w().tools)}
          onInput={(e) => props.onPatch({ tools: parseList(e.currentTarget.value) })}
        />
        <Input
          label="Can create workers"
          value={listInput(w().can_create_workers)}
          onInput={(e) =>
            props.onPatch({ can_create_workers: parseList(e.currentTarget.value) })
          }
        />
        <Input
          label="Can run workflows"
          value={listInput(w().can_run_workflows)}
          onInput={(e) =>
            props.onPatch({ can_run_workflows: parseList(e.currentTarget.value) })
          }
        />
        <Input
          label="Memory banks"
          value={listInput(w().memory_banks)}
          onInput={(e) =>
            props.onPatch({ memory_banks: parseList(e.currentTarget.value) })
          }
        />
        <Input
          label="Skills"
          value={listInput(w().skills)}
          onInput={(e) => props.onPatch({ skills: parseList(e.currentTarget.value) })}
        />
        <Input
          label="Compaction"
          value={w().compaction ?? ""}
          onInput={(e) =>
            props.onPatch({ compaction: optionalString(e.currentTarget.value) })
          }
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
  const items = () => props.config.automatons;
  const selectedItem = () => items().find((a) => a.id === selected()) ?? null;
  const selectedIdx = () => items().findIndex((a) => a.id === selected());

  const add = () => {
    const id = freshId("automaton", items().map((a) => a.id));
    const next: AutomatonState = {
      id,
      role: "automaton",
      type: "script",
      command: "",
    };
    props.setConfig("automatons", (xs) => [...xs, next]);
    setSelected(id);
  };
  const remove = (idx: number) => {
    const id = items()[idx]?.id;
    props.setConfig("automatons", (xs) => xs.filter((_, i) => i !== idx));
    if (selected() === id) setSelected(null);
  };

  return (
    <>
      {entityList({
        items: items(),
        selectedId: selected(),
        setSelected,
        onAdd: add,
        onRemove: remove,
        newLabel: "Add automaton",
      })}
      <Show when={selectedItem()}>
        {(a) => (
          <AutomatonForm
            auto={a()}
            onPatch={(patch) =>
              props.setConfig(
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
              props.setConfig("automatons", selectedIdx(), replacement);
            }}
            onIdChange={(id) => {
              props.setConfig("automatons", selectedIdx(), "id", id);
              setSelected(id);
            }}
          />
        )}
      </Show>
    </>
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
        <Input
          label="Can message"
          value={listInput(a().can_message)}
          onInput={(e) =>
            props.onPatch({ can_message: parseList(e.currentTarget.value) } as Partial<AutomatonState>)
          }
        />
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
        <Checkbox
          label="Supervisor"
          checked={a().supervisor ?? false}
          onChange={(v) => props.onPatch({ supervisor: v ? true : undefined } as Partial<AutomatonState>)}
        />
        <Checkbox
          label="Sysadmin"
          checked={a().sysadmin ?? false}
          onChange={(v) => props.onPatch({ sysadmin: v ? true : undefined } as Partial<AutomatonState>)}
        />
      </div>
    </Card>
  );
}

// ---------------------------------------------------------------------
// Channels
// ---------------------------------------------------------------------

function ChannelsSection(props: SectionProps): JSX.Element {
  const [selected, setSelected] = createSignal<string | null>(null);
  const items = () => props.config.channels;
  const selectedItem = () => items().find((c) => c.id === selected()) ?? null;
  const selectedIdx = () => items().findIndex((c) => c.id === selected());

  const add = () => {
    const id = freshId("channel", items().map((c) => c.id));
    const next: ChannelState = { id, members: [] };
    props.setConfig("channels", (xs) => [...xs, next]);
    setSelected(id);
  };
  const remove = (idx: number) => {
    const id = items()[idx]?.id;
    props.setConfig("channels", (xs) => xs.filter((_, i) => i !== idx));
    if (selected() === id) setSelected(null);
  };

  return (
    <>
      {entityList({
        items: items(),
        selectedId: selected(),
        setSelected,
        onAdd: add,
        onRemove: remove,
        newLabel: "Add channel",
      })}
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
                    props.setConfig("channels", selectedIdx(), "id", e.currentTarget.value.trim());
                    setSelected(e.currentTarget.value.trim());
                  }}
                />
                <Input
                  label="Description"
                  value={v.description ?? ""}
                  onInput={(e) =>
                    props.setConfig(
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
                      props.setConfig(
                        "channels",
                        selectedIdx(),
                        "welcome",
                        optionalString(e.currentTarget.value),
                      )
                    }
                  />
                </div>
                <div style={{ "grid-column": "1 / -1" }}>
                  <Input
                    label="Members (comma-separated agent ids)"
                    value={listInput(v.members)}
                    onInput={(e) =>
                      props.setConfig(
                        "channels",
                        selectedIdx(),
                        "members",
                        parseList(e.currentTarget.value) ?? [],
                      )
                    }
                  />
                </div>
              </div>
            </Card>
          );
        }}
      </Show>
    </>
  );
}

// ---------------------------------------------------------------------
// Memory banks
// ---------------------------------------------------------------------

function MemoryBanksSection(props: SectionProps): JSX.Element {
  const [selected, setSelected] = createSignal<string | null>(null);
  const items = () => props.config.memory_banks;
  const selectedItem = () => items().find((m) => m.id === selected()) ?? null;
  const selectedIdx = () => items().findIndex((m) => m.id === selected());

  const add = () => {
    const id = freshId("memory_bank", items().map((m) => m.id));
    const next: MemoryBankState = { id, description: "" };
    props.setConfig("memory_banks", (xs) => [...xs, next]);
    setSelected(id);
  };
  const remove = (idx: number) => {
    const id = items()[idx]?.id;
    props.setConfig("memory_banks", (xs) => xs.filter((_, i) => i !== idx));
    if (selected() === id) setSelected(null);
  };

  return (
    <>
      {entityList({
        items: items(),
        selectedId: selected(),
        setSelected,
        onAdd: add,
        onRemove: remove,
        newLabel: "Add memory bank",
      })}
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
                    props.setConfig(
                      "memory_banks",
                      selectedIdx(),
                      "id",
                      e.currentTarget.value.trim(),
                    );
                    setSelected(e.currentTarget.value.trim());
                  }}
                />
                <Input
                  label="Embed model (optional)"
                  value={v.embed_model ?? ""}
                  onInput={(e) =>
                    props.setConfig(
                      "memory_banks",
                      selectedIdx(),
                      "embed_model",
                      optionalString(e.currentTarget.value),
                    )
                  }
                />
                <div style={{ "grid-column": "1 / -1" }}>
                  <Textarea
                    label="Description"
                    value={v.description}
                    onInput={(e) =>
                      props.setConfig(
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
    </>
  );
}

// ---------------------------------------------------------------------
// Compactions
// ---------------------------------------------------------------------

function CompactionsSection(props: SectionProps): JSX.Element {
  const [selected, setSelected] = createSignal<string | null>(null);
  const items = () => props.config.compactions;
  const selectedItem = () => items().find((c) => c.id === selected()) ?? null;
  const selectedIdx = () => items().findIndex((c) => c.id === selected());

  const add = () => {
    const id = freshId("compaction", items().map((c) => c.id));
    const next: CompactionState = { id, strategy: "harness" };
    props.setConfig("compactions", (xs) => [...xs, next]);
    setSelected(id);
  };
  const remove = (idx: number) => {
    const id = items()[idx]?.id;
    props.setConfig("compactions", (xs) => xs.filter((_, i) => i !== idx));
    if (selected() === id) setSelected(null);
  };

  return (
    <>
      {entityList({
        items: items(),
        selectedId: selected(),
        setSelected,
        onAdd: add,
        onRemove: remove,
        newLabel: "Add compaction",
      })}
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
            props.setConfig("compactions", selectedIdx(), next);
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
                    props.setConfig(
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
                        props.setConfig(
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
                  <Input
                    label="AI core (optional)"
                    value={(v as { ai_core?: string }).ai_core ?? ""}
                    onInput={(e) =>
                      props.setConfig(
                        "compactions",
                        selectedIdx(),
                        "ai_core" as never,
                        optionalString(e.currentTarget.value) as never,
                      )
                    }
                  />
                </Show>
              </div>
            </Card>
          );
        }}
      </Show>
    </>
  );
}

// ---------------------------------------------------------------------
// Webhooks
// ---------------------------------------------------------------------

function WebhooksSection(props: SectionProps): JSX.Element {
  const [selected, setSelected] = createSignal<string | null>(null);
  const items = () => props.config.webhooks;
  const selectedItem = () => items().find((w) => w.id === selected()) ?? null;
  const selectedIdx = () => items().findIndex((w) => w.id === selected());

  const add = () => {
    const id = freshId("webhook", items().map((w) => w.id));
    const next: WebhookState = {
      id,
      mount: "",
      path: "/hook",
      deliver_to: "",
    };
    props.setConfig("webhooks", (xs) => [...xs, next]);
    setSelected(id);
  };
  const remove = (idx: number) => {
    const id = items()[idx]?.id;
    props.setConfig("webhooks", (xs) => xs.filter((_, i) => i !== idx));
    if (selected() === id) setSelected(null);
  };

  return (
    <>
      {entityList({
        items: items(),
        selectedId: selected(),
        setSelected,
        onAdd: add,
        onRemove: remove,
        newLabel: "Add webhook",
      })}
      <Show when={selectedItem()}>
        {(w) => {
          const v = w();
          const set = <K extends keyof WebhookState>(key: K, value: WebhookState[K]) => {
            props.setConfig("webhooks", selectedIdx(), key, value);
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
                <Input
                  label="Mount (webservice automaton id)"
                  value={v.mount}
                  onInput={(e) => set("mount", e.currentTarget.value)}
                />
                <Input
                  label="Path"
                  value={v.path}
                  onInput={(e) => set("path", e.currentTarget.value)}
                />
                <Input
                  label="Deliver to"
                  value={v.deliver_to}
                  onInput={(e) => set("deliver_to", e.currentTarget.value)}
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
                    onInput={(e) => set("auth_header", optionalString(e.currentTarget.value))}
                  />
                </Show>
                <Show when={v.auth_kind === "json_field"}>
                  <Input
                    label="Auth JSON field"
                    value={v.auth_field ?? ""}
                    onInput={(e) => set("auth_field", optionalString(e.currentTarget.value))}
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
                    onInput={(e) => set("auth_secret_env", optionalString(e.currentTarget.value))}
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
    </>
  );
}

// ---------------------------------------------------------------------
// Workflows (simple textarea for property/step JSON for v1)
// ---------------------------------------------------------------------

function WorkflowsSection(props: SectionProps): JSX.Element {
  const [selected, setSelected] = createSignal<string | null>(null);
  const items = () => props.config.workflows;
  const selectedItem = () => items().find((w) => w.id === selected()) ?? null;
  const selectedIdx = () => items().findIndex((w) => w.id === selected());

  const add = () => {
    const id = freshId("workflow", items().map((w) => w.id));
    const next: WorkflowState = {
      id,
      description: "",
      properties: [],
      steps: [],
    };
    props.setConfig("workflows", (xs) => [...xs, next]);
    setSelected(id);
  };
  const remove = (idx: number) => {
    const id = items()[idx]?.id;
    props.setConfig("workflows", (xs) => xs.filter((_, i) => i !== idx));
    if (selected() === id) setSelected(null);
  };

  return (
    <>
      {entityList({
        items: items(),
        selectedId: selected(),
        setSelected,
        onAdd: add,
        onRemove: remove,
        newLabel: "Add workflow",
      })}
      <Show when={selectedItem()}>
        {(w) => {
          const v = w();
          return (
            <Card title={`workflow ${v.id}`}>
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
                    props.setConfig(
                      "workflows",
                      selectedIdx(),
                      "id",
                      e.currentTarget.value.trim(),
                    );
                    setSelected(e.currentTarget.value.trim());
                  }}
                />
                <Input
                  type="number"
                  label="Max retries"
                  value={v.max_retries ?? ""}
                  onInput={(e) =>
                    props.setConfig(
                      "workflows",
                      selectedIdx(),
                      "max_retries",
                      optionalNumber(e.currentTarget.value),
                    )
                  }
                />
                <div style={{ "grid-column": "1 / -1" }}>
                  <Textarea
                    label="Description"
                    value={v.description}
                    onInput={(e) =>
                      props.setConfig(
                        "workflows",
                        selectedIdx(),
                        "description",
                        e.currentTarget.value,
                      )
                    }
                  />
                </div>
                <div style={{ "grid-column": "1 / -1" }}>
                  <span
                    style={{
                      "font-size": "12px",
                      color: "var(--fg-2)",
                      display: "block",
                      "margin-bottom": "4px",
                    }}
                  >
                    {v.properties.length} property block(s),{" "}
                    {v.steps.length} step block(s) — edit in the WCL pane
                    below for now.
                  </span>
                </div>
              </div>
            </Card>
          );
        }}
      </Show>
    </>
  );
}
