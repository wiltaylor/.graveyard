import {
  For,
  Show,
  createEffect,
  createResource,
  createSignal,
  type JSX,
} from "solid-js";

import { api } from "../../api";
import type {
  InstanceSummary,
  MarketplaceAvailableSkill,
  MarketplaceSummary,
  RemoteSkillSummary,
  SkillPushResult,
  SkillSummary,
} from "../../types";
import {
  Badge,
  Button,
  Card,
  Checkbox,
  Empty,
  Icon,
  Input,
  Modal,
  Tabs,
  Toast,
  pushToast,
} from "../ui";

type TabId = "skills" | "marketplaces";

export default function SkillsView(): JSX.Element {
  const [activeTab, setActiveTab] = createSignal<TabId>("skills");
  const [skills, { refetch: refetchSkills }] = createResource<SkillSummary[]>(
    () => api.listSkills(),
    { initialValue: [] },
  );
  const [marketplaces, { refetch: refetchMarketplaces }] = createResource<
    MarketplaceSummary[]
  >(() => api.listMarketplaces(), { initialValue: [] });
  const [instances] = createResource<InstanceSummary[]>(
    () => api.listInstances(),
    { initialValue: [] },
  );

  const [showUpload, setShowUpload] = createSignal(false);
  const [showImport, setShowImport] = createSignal(false);
  const [showAddMarketplace, setShowAddMarketplace] = createSignal(false);
  const [pushTarget, setPushTarget] = createSignal<SkillSummary | null>(null);

  return (
    <div
      style={{
        "max-width": "980px",
        display: "flex",
        "flex-direction": "column",
        gap: "20px",
      }}
    >
      <header>
        <h2
          style={{
            "font-size": "22px",
            "font-weight": "600",
            color: "var(--fg-0)",
            "letter-spacing": "-0.01em",
            margin: "0",
          }}
        >
          Skills
        </h2>
        <p
          style={{
            color: "var(--fg-2)",
            "font-size": "13px",
            margin: "6px 0 0",
          }}
        >
          Author, fetch, and distribute skills to your aciddog instances.
        </p>
      </header>

      <Tabs
        items={[
          { id: "skills", label: "Catalog", icon: "box" },
          { id: "marketplaces", label: "Marketplaces", icon: "git-branch" },
        ]}
        active={activeTab()}
        onChange={setActiveTab}
      />

      <Show when={activeTab() === "skills"}>
        <SkillsList
          skills={skills() ?? []}
          loading={skills.loading}
          onUpload={() => setShowUpload(true)}
          onImport={() => setShowImport(true)}
          onPush={(s) => setPushTarget(s)}
          onUpdate={async (s) => {
            try {
              await api.updateSkill(s.id);
              await refetchSkills();
              pushToast(`Updated ${s.name}.`, "success");
            } catch (err) {
              pushToast(err instanceof Error ? err.message : String(err), "danger");
            }
          }}
          onDelete={async (s) => {
            if (!window.confirm(`Remove ${s.name} from the catalog?`)) return;
            try {
              await api.deleteSkill(s.id);
              await refetchSkills();
            } catch (err) {
              pushToast(err instanceof Error ? err.message : String(err), "danger");
            }
          }}
        />
      </Show>

      <Show when={activeTab() === "marketplaces"}>
        <MarketplacesList
          marketplaces={marketplaces() ?? []}
          loading={marketplaces.loading}
          onAdd={() => setShowAddMarketplace(true)}
          onUpdate={async (m) => {
            try {
              const events: string[] = [];
              await api.updateMarketplaceStream(m.id, (ev) => {
                if (ev.event === "progress") events.push(ev.message);
              });
              await refetchMarketplaces();
              await refetchSkills();
              pushToast(`Updated ${m.id}.`, "success");
            } catch (err) {
              pushToast(
                err instanceof Error ? err.message : String(err),
                "danger",
              );
            }
          }}
          onDelete={async (m) => {
            if (
              !window.confirm(
                `Remove marketplace ${m.id} and all skills sourced from it?`,
              )
            )
              return;
            try {
              await api.deleteMarketplace(m.id);
              await refetchMarketplaces();
              await refetchSkills();
            } catch (err) {
              pushToast(err instanceof Error ? err.message : String(err), "danger");
            }
          }}
          onSkillInstalled={async () => {
            await refetchMarketplaces();
            await refetchSkills();
          }}
        />
      </Show>

      <Modal
        open={showUpload()}
        onClose={() => setShowUpload(false)}
        title="Upload skill"
        width={520}
      >
        <SkillUpload
          onCancel={() => setShowUpload(false)}
          onUploaded={async () => {
            setShowUpload(false);
            await refetchSkills();
            pushToast("Skill uploaded.", "success");
          }}
        />
      </Modal>

      <Modal
        open={showImport()}
        onClose={() => setShowImport(false)}
        title="Import skill from instance"
        width={520}
      >
        <SkillImportDialog
          instances={instances() ?? []}
          onCancel={() => setShowImport(false)}
          onImported={async () => {
            setShowImport(false);
            await refetchSkills();
            pushToast("Skill imported.", "success");
          }}
        />
      </Modal>

      <Modal
        open={showAddMarketplace()}
        onClose={() => setShowAddMarketplace(false)}
        title="Add marketplace"
        width={520}
      >
        <MarketplaceAddDialog
          onCancel={() => setShowAddMarketplace(false)}
          onAdded={async () => {
            setShowAddMarketplace(false);
            await refetchMarketplaces();
            await refetchSkills();
            pushToast("Marketplace added.", "success");
          }}
        />
      </Modal>

      <Modal
        open={pushTarget() !== null}
        onClose={() => setPushTarget(null)}
        title={pushTarget() ? `Push ${pushTarget()!.name}` : "Push"}
        width={520}
      >
        <Show when={pushTarget()}>
          {(skill) => (
            <SkillPushDialog
              skill={skill()}
              instances={instances() ?? []}
              onCancel={() => setPushTarget(null)}
              onPushed={async (results) => {
                setPushTarget(null);
                await refetchSkills();
                renderPushToast(results);
              }}
            />
          )}
        </Show>
      </Modal>
    </div>
  );
}

function renderPushToast(results: SkillPushResult[]): void {
  const okCount = results.filter((r) => r.ok).length;
  const failCount = results.length - okCount;
  if (failCount === 0) {
    pushToast(
      `Pushed to ${okCount} instance${okCount === 1 ? "" : "s"}. Reload from each Team page to apply.`,
      "success",
    );
    return;
  }
  const firstError = results.find((r) => !r.ok)?.error ?? "unknown error";
  pushToast(
    `Pushed to ${okCount}/${results.length}. First failure: ${firstError}`,
    "danger",
  );
}

interface SkillsListProps {
  skills: SkillSummary[];
  loading: boolean;
  onUpload: () => void;
  onImport: () => void;
  onPush: (s: SkillSummary) => void;
  onUpdate: (s: SkillSummary) => void;
  onDelete: (s: SkillSummary) => void;
}

function SkillsList(props: SkillsListProps): JSX.Element {
  return (
    <Card
      title="Catalog"
      padded={false}
      action={
        <span style={{ display: "inline-flex", gap: "8px" }}>
          <Button
            size="sm"
            variant="secondary"
            icon="download"
            onClick={() => props.onImport()}
          >
            Import from instance
          </Button>
          <Button
            size="sm"
            variant="primary"
            icon="upload"
            onClick={() => props.onUpload()}
          >
            Upload zip
          </Button>
        </span>
      }
    >
      <Show
        when={!props.loading}
        fallback={
          <div
            style={{ padding: "16px", "font-size": "13px", color: "var(--fg-2)" }}
          >
            Loading…
          </div>
        }
      >
        <Show
          when={props.skills.length > 0}
          fallback={
            <div style={{ padding: "20px" }}>
              <Empty>
                No skills yet — upload a zip, import from a running instance, or
                add a marketplace.
              </Empty>
            </div>
          }
        >
          <table class="ftable">
            <thead>
              <tr>
                <th>Name</th>
                <th>Source</th>
                <th>Last pushed</th>
                <th style={{ "text-align": "right" }}>Actions</th>
              </tr>
            </thead>
            <tbody>
              <For each={props.skills}>
                {(s) => (
                  <tr>
                    <td>
                      <div
                        style={{
                          display: "flex",
                          "flex-direction": "column",
                          gap: "2px",
                        }}
                      >
                        <span style={{ "font-weight": "500" }}>{s.name}</span>
                        <Show when={s.description}>
                          <span
                            style={{
                              "font-size": "12px",
                              color: "var(--fg-2)",
                            }}
                          >
                            {s.description}
                          </span>
                        </Show>
                      </div>
                    </td>
                    <td>
                      <span
                        style={{
                          display: "inline-flex",
                          gap: "6px",
                          "align-items": "center",
                        }}
                      >
                        <Badge tone={badgeForSource(s.source_kind)}>
                          {s.source_kind}
                        </Badge>
                        <Show when={s.source_ref}>
                          <span
                            style={{
                              "font-size": "12px",
                              color: "var(--fg-2)",
                            }}
                          >
                            {s.source_ref}
                          </span>
                        </Show>
                        <Show when={s.update_available}>
                          <Badge tone="warning">update available</Badge>
                        </Show>
                      </span>
                    </td>
                    <td>
                      <Show
                        when={s.last_pushed_to.length > 0}
                        fallback={
                          <span
                            style={{
                              "font-size": "12px",
                              color: "var(--fg-2)",
                            }}
                          >
                            never
                          </span>
                        }
                      >
                        <span
                          style={{ display: "inline-flex", gap: "4px", "flex-wrap": "wrap" }}
                        >
                          <For each={s.last_pushed_to}>
                            {(p) => (
                              <Badge tone={p.up_to_date ? "success" : "warning"}>
                                {p.instance_name}
                              </Badge>
                            )}
                          </For>
                        </span>
                      </Show>
                    </td>
                    <td style={{ "text-align": "right" }}>
                      <span style={{ display: "inline-flex", gap: "6px" }}>
                        <Show when={s.update_available}>
                          <Button
                            size="sm"
                            variant="secondary"
                            onClick={() => props.onUpdate(s)}
                          >
                            Update
                          </Button>
                        </Show>
                        <Button
                          size="sm"
                          variant="primary"
                          onClick={() => props.onPush(s)}
                        >
                          Push…
                        </Button>
                        <Button
                          size="sm"
                          variant="danger"
                          onClick={() => props.onDelete(s)}
                        >
                          Remove
                        </Button>
                      </span>
                    </td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </Show>
      </Show>
    </Card>
  );
}

function badgeForSource(kind: string): "accent" | "neutral" | "info" {
  switch (kind) {
    case "uploaded":
      return "accent";
    case "imported":
      return "info";
    case "marketplace":
      return "neutral";
    default:
      return "neutral";
  }
}

interface MarketplacesListProps {
  marketplaces: MarketplaceSummary[];
  loading: boolean;
  onAdd: () => void;
  onUpdate: (m: MarketplaceSummary) => void;
  onDelete: (m: MarketplaceSummary) => void;
  onSkillInstalled: () => void;
}

function MarketplacesList(props: MarketplacesListProps): JSX.Element {
  const [expanded, setExpanded] = createSignal<Set<string>>(new Set());
  const toggle = (id: string) => {
    const next = new Set(expanded());
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setExpanded(next);
  };
  return (
    <Card
      title="Marketplaces"
      padded={false}
      action={
        <Button
          size="sm"
          variant="primary"
          icon="plus"
          onClick={() => props.onAdd()}
        >
          Add marketplace
        </Button>
      }
    >
      <Show
        when={!props.loading}
        fallback={
          <div
            style={{ padding: "16px", "font-size": "13px", color: "var(--fg-2)" }}
          >
            Loading…
          </div>
        }
      >
        <Show
          when={props.marketplaces.length > 0}
          fallback={
            <div style={{ padding: "20px" }}>
              <Empty>
                Add an Anthropic-format marketplace (a git repo whose root
                contains <code>.claude-plugin/marketplace.json</code>) to pull
                in skills.
              </Empty>
            </div>
          }
        >
          <table class="ftable">
            <thead>
              <tr>
                <th style={{ width: "32px" }}></th>
                <th>Name</th>
                <th>Git URL</th>
                <th>Synced</th>
                <th>Skills</th>
                <th style={{ "text-align": "right" }}>Actions</th>
              </tr>
            </thead>
            <tbody>
              <For each={props.marketplaces}>
                {(m) => (
                  <>
                    <tr>
                      <td>
                        <button
                          type="button"
                          class="ftopbar-icon-btn"
                          aria-label={
                            expanded().has(m.id) ? "Collapse" : "Expand"
                          }
                          title={expanded().has(m.id) ? "Collapse" : "Expand"}
                          onClick={() => toggle(m.id)}
                        >
                          <Icon
                            name={
                              expanded().has(m.id)
                                ? "chevron-down"
                                : "chevron-right"
                            }
                            size={14}
                          />
                        </button>
                      </td>
                      <td style={{ "font-weight": "500" }}>{m.id}</td>
                      <td class="col-mono">{m.git_url}</td>
                      <td style={{ "font-size": "12px", color: "var(--fg-2)" }}>
                        {m.last_synced_at
                          ? new Date(m.last_synced_at).toLocaleString()
                          : "never"}
                      </td>
                      <td>
                        <span
                          style={{
                            display: "inline-flex",
                            gap: "6px",
                            "align-items": "center",
                          }}
                        >
                          <Badge tone="success">
                            {m.installed_skill_count} installed
                          </Badge>
                          <span
                            style={{
                              "font-size": "12px",
                              color: "var(--fg-2)",
                            }}
                          >
                            of {m.skill_count}
                          </span>
                        </span>
                      </td>
                      <td style={{ "text-align": "right" }}>
                        <span style={{ display: "inline-flex", gap: "6px" }}>
                          <Button
                            size="sm"
                            variant="secondary"
                            icon="refresh"
                            onClick={() => props.onUpdate(m)}
                          >
                            Update
                          </Button>
                          <Button
                            size="sm"
                            variant="danger"
                            onClick={() => props.onDelete(m)}
                          >
                            Remove
                          </Button>
                        </span>
                      </td>
                    </tr>
                    <Show when={expanded().has(m.id)}>
                      <tr>
                        <td colspan={6} style={{ background: "var(--bg-2)" }}>
                          <MarketplaceSkills
                            marketplaceId={m.id}
                            onInstalled={props.onSkillInstalled}
                          />
                        </td>
                      </tr>
                    </Show>
                  </>
                )}
              </For>
            </tbody>
          </table>
        </Show>
      </Show>
    </Card>
  );
}

interface MarketplaceSkillsProps {
  marketplaceId: string;
  onInstalled: () => void;
}

function MarketplaceSkills(props: MarketplaceSkillsProps): JSX.Element {
  const [skills, { refetch }] = createResource<MarketplaceAvailableSkill[]>(
    () => api.listMarketplaceSkills(props.marketplaceId),
    { initialValue: [] },
  );
  const [busy, setBusy] = createSignal<string | null>(null);
  const [filter, setFilter] = createSignal("");
  const filtered = () => {
    const all = skills() ?? [];
    const q = filter().trim().toLowerCase();
    if (!q) return all;
    return all.filter((s) =>
      s.name.toLowerCase().includes(q) ||
      s.plugin.toLowerCase().includes(q) ||
      s.description.toLowerCase().includes(q),
    );
  };
  const install = async (s: MarketplaceAvailableSkill) => {
    setBusy(s.name);
    try {
      await api.installMarketplaceSkill(props.marketplaceId, s.name);
      await refetch();
      props.onInstalled();
      pushToast(`Installed ${s.name}.`, "success");
    } catch (err) {
      pushToast(err instanceof Error ? err.message : String(err), "danger");
    } finally {
      setBusy(null);
    }
  };
  return (
    <div style={{ padding: "12px 16px" }}>
      <Show
        when={!skills.loading}
        fallback={
          <div style={{ "font-size": "12px", color: "var(--fg-2)" }}>
            Loading skills…
          </div>
        }
      >
        <Show
          when={(skills() ?? []).length > 0}
          fallback={
            <div style={{ "font-size": "12px", color: "var(--fg-2)" }}>
              No skills available in this marketplace.
            </div>
          }
        >
          <div
            style={{
              display: "flex",
              "align-items": "center",
              gap: "8px",
              "margin-bottom": "10px",
            }}
          >
            <Input
              placeholder="Filter by skill, plugin, or description…"
              value={filter()}
              onInput={(e) => setFilter(e.currentTarget.value)}
              autocomplete="off"
            />
            <span style={{ "font-size": "12px", color: "var(--fg-2)" }}>
              {filtered().length} of {(skills() ?? []).length}
            </span>
          </div>
          <Show
            when={filtered().length > 0}
            fallback={
              <div
                style={{
                  "font-size": "12px",
                  color: "var(--fg-2)",
                  padding: "12px 0",
                }}
              >
                No skills match `{filter()}`.
              </div>
            }
          >
          <table class="ftable">
            <thead>
              <tr>
                <th>Skill</th>
                <th>Plugin</th>
                <th>Status</th>
                <th style={{ "text-align": "right" }}>Actions</th>
              </tr>
            </thead>
            <tbody>
              <For each={filtered()}>
                {(s) => (
                  <tr>
                    <td>
                      <div
                        style={{
                          display: "flex",
                          "flex-direction": "column",
                          gap: "2px",
                        }}
                      >
                        <span style={{ "font-weight": "500" }}>{s.name}</span>
                        <Show when={s.description}>
                          <span
                            style={{
                              "font-size": "12px",
                              color: "var(--fg-2)",
                            }}
                          >
                            {s.description}
                          </span>
                        </Show>
                      </div>
                    </td>
                    <td>
                      <span
                        style={{ "font-size": "12px", color: "var(--fg-2)" }}
                      >
                        {s.plugin}
                      </span>
                    </td>
                    <td>
                      <Show
                        when={s.installed}
                        fallback={
                          <Badge tone="neutral">not installed</Badge>
                        }
                      >
                        <Show
                          when={s.up_to_date}
                          fallback={<Badge tone="warning">update available</Badge>}
                        >
                          <Badge tone="success">installed</Badge>
                        </Show>
                      </Show>
                    </td>
                    <td style={{ "text-align": "right" }}>
                      <Button
                        size="sm"
                        variant={s.installed ? "secondary" : "primary"}
                        disabled={busy() === s.name || (s.installed && !!s.up_to_date)}
                        onClick={() => install(s)}
                      >
                        {busy() === s.name
                          ? "Installing…"
                          : s.installed
                          ? s.up_to_date
                            ? "Installed"
                            : "Update"
                          : "Install"}
                      </Button>
                    </td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
          </Show>
        </Show>
      </Show>
    </div>
  );
}

interface SkillUploadProps {
  onCancel: () => void;
  onUploaded: () => void;
}

function SkillUpload(props: SkillUploadProps): JSX.Element {
  const [file, setFile] = createSignal<File | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [err, setErr] = createSignal<string | null>(null);
  const submit = async (e: SubmitEvent) => {
    e.preventDefault();
    const f = file();
    if (!f) return;
    setBusy(true);
    setErr(null);
    try {
      await api.uploadSkill(f);
      props.onUploaded();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <form
      onSubmit={submit}
      style={{ display: "flex", "flex-direction": "column", gap: "12px" }}
    >
      <p style={{ "font-size": "13px", color: "var(--fg-2)", margin: "0" }}>
        Drop a <code>.zip</code> archive containing exactly one top-level
        directory (the skill name) with a <code>SKILL.md</code> at its root.
      </p>
      <input
        type="file"
        accept=".zip,application/zip,application/x-zip-compressed"
        onChange={(e) => {
          const list = e.currentTarget.files;
          setFile(list && list.length > 0 ? list.item(0) : null);
        }}
      />
      <Show when={err()}>
        <Toast tone="danger">{err()}</Toast>
      </Show>
      <div style={{ display: "flex", gap: "8px", "justify-content": "flex-end" }}>
        <Button variant="secondary" type="button" onClick={() => props.onCancel()}>
          Cancel
        </Button>
        <Button
          variant="primary"
          type="submit"
          disabled={busy() || !file()}
          icon="upload"
        >
          {busy() ? "Uploading…" : "Upload"}
        </Button>
      </div>
    </form>
  );
}

interface SkillImportDialogProps {
  instances: InstanceSummary[];
  onCancel: () => void;
  onImported: () => void;
}

function SkillImportDialog(props: SkillImportDialogProps): JSX.Element {
  const [instanceId, setInstanceId] = createSignal<string | null>(null);
  const [skills, { refetch }] = createResource<RemoteSkillSummary[]>(async () => {
    const id = instanceId();
    if (!id) return [];
    return await api.listSkillsOnInstance(id);
  }, { initialValue: [] });
  const [picked, setPicked] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [err, setErr] = createSignal<string | null>(null);

  const submit = async () => {
    const id = instanceId();
    const name = picked();
    if (!id || !name) return;
    setBusy(true);
    setErr(null);
    try {
      await api.importSkillFromInstance(id, name);
      props.onImported();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div style={{ display: "flex", "flex-direction": "column", gap: "12px" }}>
      <label
        style={{ "font-size": "12px", color: "var(--fg-2)" }}
      >
        Instance
      </label>
      <select
        class="finput"
        value={instanceId() ?? ""}
        onChange={(e) => {
          setInstanceId(e.currentTarget.value || null);
          setPicked(null);
          refetch();
        }}
      >
        <option value="">Select an instance…</option>
        <For each={props.instances}>
          {(i) => <option value={i.id}>{i.name}</option>}
        </For>
      </select>
      <Show when={instanceId() !== null}>
        <label
          style={{ "font-size": "12px", color: "var(--fg-2)" }}
        >
          Skill
        </label>
        <Show
          when={!skills.loading}
          fallback={
            <div style={{ "font-size": "12px", color: "var(--fg-2)" }}>
              Loading…
            </div>
          }
        >
          <Show
            when={(skills() ?? []).length > 0}
            fallback={
              <div style={{ "font-size": "12px", color: "var(--fg-2)" }}>
                No skills installed on this instance.
              </div>
            }
          >
            <select
              class="finput"
              value={picked() ?? ""}
              onChange={(e) => setPicked(e.currentTarget.value || null)}
            >
              <option value="">Select a skill…</option>
              <For each={skills() ?? []}>
                {(s) => (
                  <option value={s.name}>
                    {s.name}
                    {s.description ? ` — ${s.description}` : ""}
                  </option>
                )}
              </For>
            </select>
          </Show>
        </Show>
      </Show>
      <Show when={err()}>
        <Toast tone="danger">{err()}</Toast>
      </Show>
      <div style={{ display: "flex", gap: "8px", "justify-content": "flex-end" }}>
        <Button variant="secondary" type="button" onClick={() => props.onCancel()}>
          Cancel
        </Button>
        <Button
          variant="primary"
          type="button"
          disabled={busy() || !picked()}
          onClick={submit}
        >
          {busy() ? "Importing…" : "Import"}
        </Button>
      </div>
    </div>
  );
}

interface SkillPushDialogProps {
  skill: SkillSummary;
  instances: InstanceSummary[];
  onCancel: () => void;
  onPushed: (results: SkillPushResult[]) => void;
}

function SkillPushDialog(props: SkillPushDialogProps): JSX.Element {
  const [selected, setSelected] = createSignal<Set<string>>(new Set());
  const [busy, setBusy] = createSignal(false);
  const [err, setErr] = createSignal<string | null>(null);

  const toggle = (id: string) => {
    const next = new Set(selected());
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setSelected(next);
  };

  const submit = async () => {
    const ids = Array.from(selected());
    if (ids.length === 0) return;
    setBusy(true);
    setErr(null);
    try {
      const resp = await api.pushSkill(props.skill.id, ids);
      props.onPushed(resp.results);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div style={{ display: "flex", "flex-direction": "column", gap: "12px" }}>
      <p style={{ "font-size": "13px", color: "var(--fg-2)", margin: "0" }}>
        Choose instances to write <code>{props.skill.name}</code> to. Files are
        copied; the daemon doesn't auto-reload — open Team and click Reload to
        pick up the change.
      </p>
      <Show
        when={props.instances.length > 0}
        fallback={
          <Empty>No instances registered. Add one in the sidebar first.</Empty>
        }
      >
        <div style={{ display: "flex", "flex-direction": "column", gap: "6px" }}>
          <For each={props.instances}>
            {(i) => (
              <Checkbox
                label={i.name}
                checked={selected().has(i.id)}
                onChange={() => toggle(i.id)}
              />
            )}
          </For>
        </div>
      </Show>
      <Show when={err()}>
        <Toast tone="danger">{err()}</Toast>
      </Show>
      <div style={{ display: "flex", gap: "8px", "justify-content": "flex-end" }}>
        <Button variant="secondary" type="button" onClick={() => props.onCancel()}>
          Cancel
        </Button>
        <Button
          variant="primary"
          type="button"
          disabled={busy() || selected().size === 0}
          onClick={submit}
        >
          {busy() ? "Pushing…" : `Push to ${selected().size}`}
        </Button>
      </div>
    </div>
  );
}

interface MarketplaceAddDialogProps {
  onCancel: () => void;
  onAdded: () => void;
}

function MarketplaceAddDialog(props: MarketplaceAddDialogProps): JSX.Element {
  const [gitUrl, setGitUrl] = createSignal("");
  const [refSpec, setRefSpec] = createSignal("");
  const [name, setName] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [err, setErr] = createSignal<string | null>(null);
  const [progressLog, setProgressLog] = createSignal<string[]>([]);
  let logRef: HTMLDivElement | undefined;
  // Auto-follow the tail: every time a new line lands, scroll the box
  // to the bottom so the latest message is visible without the user
  // having to drag the scrollbar.
  createEffect(() => {
    progressLog();
    if (logRef) {
      logRef.scrollTop = logRef.scrollHeight;
    }
  });

  const submit = async (e: SubmitEvent) => {
    e.preventDefault();
    if (!gitUrl().trim()) return;
    setBusy(true);
    setErr(null);
    setProgressLog([]);
    try {
      await api.addMarketplaceStream(
        {
          gitUrl: gitUrl().trim(),
          ref: refSpec().trim() || undefined,
          name: name().trim() || undefined,
        },
        (ev) => {
          if (ev.event === "progress") {
            setProgressLog([...progressLog(), ev.message]);
          }
        },
      );
      props.onAdded();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form
      onSubmit={submit}
      style={{ display: "flex", "flex-direction": "column", gap: "12px" }}
    >
      <Input
        label="Git URL"
        required
        value={gitUrl()}
        onInput={(e) => setGitUrl(e.currentTarget.value)}
        placeholder="https://github.com/anthropics/example-marketplace.git"
        autocomplete="off"
        disabled={busy()}
      />
      <Input
        label="Ref (branch/tag, optional)"
        value={refSpec()}
        onInput={(e) => setRefSpec(e.currentTarget.value)}
        placeholder="main"
        autocomplete="off"
        disabled={busy()}
      />
      <Input
        label="Local name override (optional)"
        value={name()}
        onInput={(e) => setName(e.currentTarget.value)}
        placeholder="defaults to the marketplace.json `name` field"
        autocomplete="off"
        disabled={busy()}
      />
      <Show when={progressLog().length > 0 || busy()}>
        <div
          ref={(el) => (logRef = el)}
          style={{
            border: "1px solid var(--border-subtle)",
            "border-radius": "6px",
            background: "var(--bg-2)",
            padding: "10px 12px",
            "font-size": "12px",
            color: "var(--fg-1)",
            "font-family": "var(--mono, monospace)",
            "max-height": "180px",
            overflow: "auto",
            display: "flex",
            "flex-direction": "column",
            gap: "4px",
          }}
        >
          <For each={progressLog()}>
            {(line, i) => (
              <div style={{ display: "flex", gap: "6px" }}>
                <Show
                  when={busy() && i() === progressLog().length - 1}
                  fallback={
                    <Icon
                      name="check"
                      size={12}
                      style={{ color: "var(--accent-success, var(--fg-2))" }}
                    />
                  }
                >
                  <Icon name="refresh" size={12} />
                </Show>
                <span>{line}</span>
              </div>
            )}
          </For>
          <Show when={busy() && progressLog().length === 0}>
            <div style={{ color: "var(--fg-2)" }}>Connecting…</div>
          </Show>
        </div>
      </Show>
      <Show when={err()}>
        <Toast tone="danger">{err()}</Toast>
      </Show>
      <div style={{ display: "flex", gap: "8px", "justify-content": "flex-end" }}>
        <Button
          variant="secondary"
          type="button"
          onClick={() => props.onCancel()}
          disabled={busy()}
        >
          Cancel
        </Button>
        <Button
          variant="primary"
          type="submit"
          disabled={busy() || !gitUrl().trim()}
        >
          {busy() ? "Working…" : "Add"}
        </Button>
      </div>
    </form>
  );
}
