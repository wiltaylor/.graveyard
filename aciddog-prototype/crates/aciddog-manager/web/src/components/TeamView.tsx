import { For, Show, createEffect, createSignal, type JSX } from "solid-js";

import { api } from "../api";
import type { WireAiCore, WireTeamInfo } from "../types";
import TeamEditor from "./team/TeamEditor";
import {
  Badge,
  Button,
  Card,
  Empty,
  Eyebrow,
  Modal,
  Stat,
  Toast,
} from "./ui";

interface Props {
  instanceId: string;
  instanceName: string;
  onRemoved: () => void;
}

export default function TeamView(props: Props): JSX.Element {
  const [info, setInfo] = createSignal<WireTeamInfo | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [showRemove, setShowRemove] = createSignal(false);
  const [removing, setRemoving] = createSignal(false);
  const [removeError, setRemoveError] = createSignal<string | null>(null);
  const [reloading, setReloading] = createSignal(false);
  const [editing, setEditing] = createSignal(false);
  const [reloadToast, setReloadToast] = createSignal<{
    tone: "success" | "danger";
    text: string;
  } | null>(null);

  const teamLabel = () => info()?.team.id ?? props.instanceName;

  const handleRemove = async () => {
    setRemoveError(null);
    setRemoving(true);
    try {
      await api.deleteInstance(props.instanceId);
      setShowRemove(false);
      props.onRemoved();
    } catch (err) {
      setRemoveError(err instanceof Error ? err.message : String(err));
    } finally {
      setRemoving(false);
    }
  };

  const handleReload = async () => {
    setReloading(true);
    setReloadToast(null);
    try {
      const resp = await api.reloadTeam(props.instanceId);
      if (resp.ok) {
        setReloadToast({
          tone: "success",
          text: resp.summary ?? "config reloaded",
        });
        // Refresh the team snapshot so any agent / core changes show up.
        void load();
      } else {
        setReloadToast({
          tone: "danger",
          text: resp.error ?? "reload failed",
        });
      }
    } catch (err) {
      setReloadToast({
        tone: "danger",
        text: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setReloading(false);
    }
  };

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const resp = await api.teamInfo(props.instanceId);
      if (!resp.ok || !resp.info) {
        setError(resp.error ?? "remote returned no data");
        setInfo(null);
      } else {
        setInfo(resp.info);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  createEffect(() => {
    void props.instanceId;
    void load();
  });

  return (
    <Show
      when={!editing()}
      fallback={
        <TeamEditor
          instanceId={props.instanceId}
          instanceName={props.instanceName}
          onClose={() => {
            setEditing(false);
            // Refresh the read-only snapshot in case the editor saved.
            void load();
          }}
        />
      }
    >
    <div
      style={{
        height: "100%",
        overflow: "auto",
        padding: "20px 24px",
        display: "flex",
        "flex-direction": "column",
        gap: "20px",
      }}
    >
      <header
        style={{
          display: "flex",
          "align-items": "flex-end",
          "justify-content": "space-between",
          gap: "16px",
        }}
      >
        <div>
          <h2
            style={{
              "font-size": "22px",
              "font-weight": "600",
              color: "var(--fg-0)",
              "letter-spacing": "-0.01em",
              margin: "0",
            }}
          >
            Team
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
              loading…
            </span>
          </Show>
        </div>
        <div style={{ display: "flex", gap: "8px" }}>
          <Button variant="ghost" size="sm" icon="refresh" onClick={load}>
            Refresh
          </Button>
          <Button
            variant="ghost"
            size="sm"
            icon="refresh"
            onClick={handleReload}
            disabled={reloading()}
          >
            {reloading() ? "Reloading…" : "Reload config"}
          </Button>
          <Button
            variant="primary"
            size="sm"
            icon="settings"
            onClick={() => setEditing(true)}
          >
            Edit team
          </Button>
          <Button
            variant="danger"
            size="sm"
            icon="x"
            onClick={() => {
              setRemoveError(null);
              setShowRemove(true);
            }}
          >
            Remove team
          </Button>
        </div>
      </header>

      <Show when={reloadToast()}>
        {(t) => <Toast tone={t().tone}>{t().text}</Toast>}
      </Show>

      <Show when={error()}>
        <Toast tone="danger">{error()}</Toast>
      </Show>

      <Show when={info()}>
        {(v) => (
          <>
            <Card
              title={v().team.id}
              action={
                <Show when={v().team.default_ai_core}>
                  <Badge tone="info">
                    default core: {v().team.default_ai_core}
                  </Badge>
                </Show>
              }
            >
              <p
                style={{
                  "font-size": "13.5px",
                  "white-space": "pre-wrap",
                  color: "var(--fg-1)",
                  "line-height": "1.55",
                  margin: "0",
                }}
              >
                {v().team.description}
              </p>
              <Show when={v().team.replace_thinking_messages}>
                <div style={{ "margin-top": "8px" }}>
                  <Badge tone="warning">
                    custom thinking messages replace built-ins
                  </Badge>
                </div>
              </Show>
            </Card>

            <section>
              <h3
                style={{
                  "font-size": "10px",
                  "font-weight": "600",
                  "letter-spacing": "0.08em",
                  "text-transform": "uppercase",
                  color: "var(--fg-2)",
                  margin: "0 0 8px",
                }}
              >
                Fleet
              </h3>
              <div
                class="kpi-row"
                style={{ "grid-template-columns": "repeat(5, 1fr)" }}
              >
                <Card padded={true}>
                  <Stat label="Agents" value={v().fleet.agents} />
                </Card>
                <Card padded={true}>
                  <Stat label="Workers" value={v().fleet.workers} />
                </Card>
                <Card padded={true}>
                  <Stat label="Automatons" value={v().fleet.automatons} />
                </Card>
                <Card padded={true}>
                  <Stat label="Channels" value={v().fleet.channels} />
                </Card>
                <Card padded={true}>
                  <Stat label="Workflows" value={v().fleet.workflows} />
                </Card>
              </div>
            </section>

            <section>
              <h3
                style={{
                  "font-size": "10px",
                  "font-weight": "600",
                  "letter-spacing": "0.08em",
                  "text-transform": "uppercase",
                  color: "var(--fg-2)",
                  margin: "0 0 8px",
                }}
              >
                AI Cores
              </h3>
              <Show
                when={v().ai_cores.length > 0}
                fallback={<Empty>No ai_core blocks declared.</Empty>}
              >
                <div
                  style={{
                    display: "flex",
                    "flex-direction": "column",
                    gap: "10px",
                  }}
                >
                  <For each={v().ai_cores}>
                    {(core) => <CoreCard core={core} />}
                  </For>
                </div>
              </Show>
            </section>
          </>
        )}
      </Show>

      <Modal
        open={showRemove()}
        onClose={() => {
          if (!removing()) setShowRemove(false);
        }}
        title="Remove team"
        width={440}
        footer={
          <>
            <Button
              variant="ghost"
              onClick={() => setShowRemove(false)}
              disabled={removing()}
            >
              Cancel
            </Button>
            <Button
              variant="danger"
              icon="x"
              onClick={handleRemove}
              disabled={removing()}
            >
              {removing() ? "Removing…" : "Remove team"}
            </Button>
          </>
        }
      >
        <div style={{ display: "flex", "flex-direction": "column", gap: "12px" }}>
          <p style={{ margin: "0", color: "var(--fg-1)", "font-size": "13.5px" }}>
            Remove team{" "}
            <span style={{ "font-family": "var(--font-mono)", color: "var(--fg-0)" }}>
              {teamLabel()}
            </span>{" "}
            from this manager?
          </p>
          <p
            style={{
              margin: "0",
              color: "var(--fg-2)",
              "font-size": "12.5px",
              "line-height": "1.5",
            }}
          >
            The remote aciddog instance keeps running — only the connection from
            this portal is dropped. You can re-add it later with the same socket
            or URL.
          </p>
          <Show when={removeError()}>
            <Toast tone="danger">{removeError()}</Toast>
          </Show>
        </div>
      </Modal>
    </div>
    </Show>
  );
}

function CoreCard(props: { core: WireAiCore }): JSX.Element {
  return (
    <Card>
      <header
        style={{
          display: "flex",
          "align-items": "center",
          "flex-wrap": "wrap",
          gap: "8px",
        }}
      >
        <h4
          style={{
            "font-size": "14px",
            "font-weight": "600",
            color: "var(--fg-0)",
            margin: "0",
          }}
        >
          {props.core.id}
        </h4>
        <Badge tone="neutral">{props.core.kind_label}</Badge>
        <Show when={props.core.is_team_default}>
          <Badge tone="success">team default</Badge>
        </Show>
        <Show when={props.core.usage_provider}>
          <Badge tone="info">usage: {props.core.usage_provider}</Badge>
        </Show>
      </header>
      <dl
        class="form-row-2"
        style={{
          "margin-top": "12px",
          "row-gap": "8px",
          "column-gap": "16px",
          "font-size": "13px",
        }}
      >
        <Show when={props.core.model}>
          <div>
            <Eyebrow>Model</Eyebrow>
            <dd
              style={{
                "font-family": "var(--font-mono)",
                color: "var(--fg-0)",
                margin: "2px 0 0",
              }}
            >
              {props.core.model}
            </dd>
          </div>
        </Show>
        <Show when={props.core.credential}>
          <div>
            <Eyebrow>Credential</Eyebrow>
            <dd
              style={{
                "font-family": "var(--font-mono)",
                color: "var(--fg-0)",
                margin: "2px 0 0",
              }}
            >
              {props.core.credential}
            </dd>
          </div>
        </Show>
        <Show
          when={props.core.default_tools && props.core.default_tools.length > 0}
        >
          <div style={{ "grid-column": "1 / -1" }}>
            <Eyebrow>Default tools</Eyebrow>
            <dd
              style={{
                display: "flex",
                "flex-wrap": "wrap",
                gap: "4px",
                margin: "4px 0 0",
              }}
            >
              <For each={props.core.default_tools ?? []}>
                {(t) => <Badge tone="neutral">{t}</Badge>}
              </For>
            </dd>
          </div>
        </Show>
        <Show when={props.core.has_system_prompt}>
          <div>
            <Eyebrow>System prompt</Eyebrow>
            <dd
              style={{
                color: "var(--fg-2)",
                "font-style": "italic",
                margin: "2px 0 0",
              }}
            >
              configured
            </dd>
          </div>
        </Show>
        <Show when={props.core.agents_using.length > 0}>
          <div style={{ "grid-column": "1 / -1" }}>
            <Eyebrow>Used by</Eyebrow>
            <dd
              style={{
                display: "flex",
                "flex-wrap": "wrap",
                gap: "4px",
                margin: "4px 0 0",
              }}
            >
              <For each={props.core.agents_using}>
                {(id) => <Badge tone="accent">{id}</Badge>}
              </For>
            </dd>
          </div>
        </Show>
      </dl>
    </Card>
  );
}
