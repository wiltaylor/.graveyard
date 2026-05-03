import {
  For,
  Match,
  Show,
  Switch,
  createEffect,
  createMemo,
  createSignal,
  type JSX,
} from "solid-js";

import { api } from "../api";
import type { AgentRow, Task, TaskStatus, TodoStatus } from "../types";
import { Badge, Button, Icon, Menu, Toast } from "./ui";

interface Props {
  instanceId: string;
  agents: AgentRow[];
}

const STATUS_ORDER: TaskStatus[] = [
  "planning",
  "open",
  "assigned",
  "approved",
  "in_progress",
  "review",
  "done",
  "cancelled",
];

const STATUS_LABEL: Record<TaskStatus, string> = {
  planning: "Planning",
  open: "Open",
  assigned: "Assigned",
  approved: "Approved",
  in_progress: "In progress",
  review: "Review",
  done: "Done",
  cancelled: "Cancelled",
};

const STATUS_ACCENT: Record<TaskStatus, string> = {
  planning: "border-fuchsia-400/30 bg-fuchsia-400/10 text-fuchsia-200",
  open: "border-border-subtle bg-bg-0/40 text-fg-1",
  assigned: "border-sky-400/30 bg-sky-400/10 text-sky-200",
  approved: "border-success/30 bg-success-bg text-success-fg",
  in_progress: "border-accent/30 bg-accent-bg text-accent-fg",
  review: "border-warning/30 bg-warning-bg text-warning-fg",
  done: "border-success/20 bg-success-bg/50 text-success-fg",
  cancelled: "border-danger/30 bg-danger-bg text-danger-fg",
};

const TODO_ORDER: TodoStatus[] = ["pending", "in_progress", "completed"];

const TODO_LABEL: Record<TodoStatus, string> = {
  pending: "Pending",
  in_progress: "In progress",
  completed: "Completed",
};

/// Reserved recipient id mirroring `aciddog::config::USER_INBOX_ID`.
const USER_ASSIGNEE_ID = "user";

type AssigneeKind = "agent" | "automaton" | "worker" | "group" | "user";

interface AssigneeOption {
  value: string;
  label: string;
  kind: AssigneeKind;
}

function distinctGroups(agents: AgentRow[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const a of agents) {
    if (a.group && !seen.has(a.group)) {
      seen.add(a.group);
      out.push(a.group);
    }
  }
  return out;
}

function buildAssigneeOptions(agents: AgentRow[]): AssigneeOption[] {
  const out: AssigneeOption[] = [];
  for (const a of agents) {
    if (
      (a.kind === "agent" || a.kind === "automaton") &&
      // :webservice automatons receive HTTP requests, not task assignments.
      a.automaton_backend !== "webservice"
    ) {
      out.push({
        value: a.id,
        label: `${a.id} (${a.kind})`,
        kind: a.kind,
      });
    }
  }
  for (const g of distinctGroups(agents)) {
    out.push({ value: g, label: `👥 ${g}`, kind: "group" });
  }
  out.push({
    value: USER_ASSIGNEE_ID,
    label: `🧑 ${USER_ASSIGNEE_ID}`,
    kind: "user",
  });
  return out;
}

function decorateAssignee(
  name: string | null | undefined,
  agents: AgentRow[],
): string {
  if (!name) return "unassigned";
  if (name === USER_ASSIGNEE_ID) return `🧑 ${name}`;
  if (distinctGroups(agents).includes(name)) return `👥 ${name}`;
  return name;
}

export default function TasksView(props: Props): JSX.Element {
  const [tasks, setTasks] = createSignal<Task[]>([]);
  const [error, setError] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [openTaskId, setOpenTaskId] = createSignal<string | null>(null);
  const [showAddModal, setShowAddModal] = createSignal(false);

  const loadTasks = async () => {
    setLoading(true);
    setError(null);
    try {
      const resp = await api.listTasks(props.instanceId);
      if (!resp.ok) {
        setError(resp.error ?? "remote returned no data");
        setTasks([]);
      } else {
        setTasks(resp.tasks);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  createEffect(() => {
    void props.instanceId;
    setOpenTaskId(null);
    setShowAddModal(false);
    void loadTasks();
  });

  const tasksByStatus = createMemo(() => {
    const out: Record<TaskStatus, Task[]> = {
      planning: [],
      open: [],
      assigned: [],
      approved: [],
      in_progress: [],
      review: [],
      done: [],
      cancelled: [],
    };
    for (const t of tasks()) {
      const bucket = out[t.status];
      if (bucket) bucket.push(t);
    }
    return out;
  });
  const [statusFilter, setStatusFilter] =
    createSignal<TaskStatus | "all">("all");

  const openTask = createMemo<Task | null>(() => {
    const id = openTaskId();
    if (!id) return null;
    return tasks().find((t) => t.id === id) ?? null;
  });

  /// Replace a task in local state after a successful mutation — saves
  /// a refetch round-trip for single-task changes. For bulk shape
  /// changes (add/delete) we still refetch to stay authoritative.
  const patchLocal = (id: string, patcher: (t: Task) => Task) => {
    setTasks(tasks().map((t) => (t.id === id ? patcher(t) : t)));
  };

  const moveTask = async (id: string, newStatus: TaskStatus) => {
    const prev = tasks().find((t) => t.id === id);
    if (!prev || prev.status === newStatus) return;
    // Optimistic update.
    patchLocal(id, (t) => ({ ...t, status: newStatus }));
    const resp = await api.patchTask(props.instanceId, id, {
      status: newStatus,
    });
    if (!resp.ok) {
      // Roll back.
      patchLocal(id, (t) => ({ ...t, status: prev.status }));
      setError(resp.error ?? "status update failed");
    }
  };

  return (
    <div class="h-full flex flex-col">
      <header class="flex items-center justify-between gap-3 px-6 py-3 border-b border-border-subtle bg-bg-1">
        <div class="flex items-center gap-3">
          <h2 class="text-base font-semibold text-fg-0">Tasks</h2>
          <Show when={loading()}>
            <Badge tone="neutral" dot>loading</Badge>
          </Show>
        </div>
        {/* TODO(T4): replace native <select> with overflow pill row when icon tab strip primitive lands. */}
        <select
          value={statusFilter()}
          onInput={(e) =>
            setStatusFilter(e.currentTarget.value as TaskStatus | "all")
          }
          class="md:hidden rounded border border-border bg-bg-0 px-2 py-1 text-sm min-w-[10rem]"
        >
          <option value="all">All statuses</option>
          <For each={STATUS_ORDER}>
            {(s) => <option value={s}>{STATUS_LABEL[s]}</option>}
          </For>
        </select>
        <div class="flex items-center gap-2">
          <Button size="sm" variant="ghost" icon="refresh" onClick={loadTasks}>
            Refresh
          </Button>
          <Button
            size="sm"
            variant="primary"
            icon="plus"
            onClick={() => setShowAddModal(true)}
          >
            New task
          </Button>
        </div>
      </header>

      <Show when={error()}>
        <div class="mx-6 mt-3">
          <Toast tone="danger">{error()}</Toast>
        </div>
      </Show>

      <div class="hidden md:flex flex-1 flex-col">
        <div class="flex-1 overflow-x-auto overflow-y-hidden px-4 py-3">
          <div class="grid grid-cols-8 gap-3 h-full min-w-[98rem]">
            <For each={STATUS_ORDER}>
              {(status) => (
                <KanbanColumn
                  status={status}
                  tasks={tasksByStatus()[status]}
                  agents={props.agents}
                  onDrop={(taskId) => moveTask(taskId, status)}
                  onCardClick={(taskId) => setOpenTaskId(taskId)}
                />
              )}
            </For>
          </div>
        </div>
      </div>

      <div class="md:hidden flex-1 overflow-y-auto">
        <MobileTaskList
          tasksByStatus={tasksByStatus()}
          agents={props.agents}
          statusFilter={statusFilter()}
          onShowAll={() => setStatusFilter("all")}
          onCardClick={(id) => setOpenTaskId(id)}
          onMove={moveTask}
        />
      </div>

      <Show when={openTask()}>
        {(task) => (
          <TaskDetailModal
            instanceId={props.instanceId}
            task={task()}
            allTasks={tasks()}
            agents={props.agents}
            onClose={() => setOpenTaskId(null)}
            onPatched={() => {
              void loadTasks();
            }}
            onDeleted={() => {
              setOpenTaskId(null);
              void loadTasks();
            }}
            onError={(e) => setError(e)}
          />
        )}
      </Show>

      <Show when={showAddModal()}>
        <AddTaskModal
          instanceId={props.instanceId}
          agents={props.agents}
          onClose={() => setShowAddModal(false)}
          onCreated={() => {
            setShowAddModal(false);
            void loadTasks();
          }}
          onError={(e) => setError(e)}
        />
      </Show>
    </div>
  );
}

function KanbanColumn(props: {
  status: TaskStatus;
  tasks: Task[];
  agents: AgentRow[];
  onDrop: (taskId: string) => void;
  onCardClick: (taskId: string) => void;
}): JSX.Element {
  const [dragOver, setDragOver] = createSignal(false);

  return (
    <section
      class={`flex flex-col rounded border min-w-[14rem] ${
        dragOver() ? "border-accent bg-accent-bg text-accent-fg" : STATUS_ACCENT[props.status]
      }`}
      onDragOver={(e) => {
        e.preventDefault();
        setDragOver(true);
      }}
      onDragLeave={() => setDragOver(false)}
      onDrop={(e) => {
        e.preventDefault();
        setDragOver(false);
        const id = e.dataTransfer?.getData("text/plain");
        if (id) props.onDrop(id);
      }}
    >
      <header class="px-2 py-1.5 border-b border-current/20 flex items-center justify-between">
        <h3 class="text-xs uppercase tracking-wider">
          {STATUS_LABEL[props.status]}
        </h3>
        <span class="text-xs opacity-70">{props.tasks.length}</span>
      </header>
      <div class="flex-1 overflow-y-auto p-2 space-y-2">
        <For each={props.tasks}>
          {(t) => (
            <TaskCard
              task={t}
              agents={props.agents}
              onClick={() => props.onCardClick(t.id)}
            />
          )}
        </For>
      </div>
    </section>
  );
}

function TaskCard(props: {
  task: Task;
  agents: AgentRow[];
  onClick: () => void;
}): JSX.Element {
  return (
    <article
      draggable={true}
      onDragStart={(e) => {
        e.dataTransfer?.setData("text/plain", props.task.id);
        if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
      }}
      onClick={props.onClick}
      class="rounded border border-border bg-bg-1 p-2 text-sm cursor-pointer hover:border-border-strong"
    >
      <div class="font-medium line-clamp-2">{props.task.title}</div>
      <div class="mt-1 flex items-center justify-between text-xs text-fg-2">
        <span>{decorateAssignee(props.task.assignee, props.agents)}</span>
        <span>
          <Show when={props.task.todos.length > 0}>
            <span>
              {props.task.todos.filter((t) => t.status === "completed").length}
              /{props.task.todos.length} todos
            </span>
          </Show>
          <Show when={props.task.blocked_by.length > 0}>
            <span class="ml-2 text-warning-fg">
              {props.task.blocked_by.length} blocking
            </span>
          </Show>
        </span>
      </div>
      <Show when={props.task.status === "planning" && props.task.planned_assignee}>
        <div class="mt-1 text-xs text-fg-3 italic">
          → on approve: {decorateAssignee(props.task.planned_assignee, props.agents)}
        </div>
      </Show>
    </article>
  );
}

function initialOpenState(
  tasksByStatus: Record<TaskStatus, Task[]>,
): Record<TaskStatus, boolean> {
  const out = {} as Record<TaskStatus, boolean>;
  for (const status of STATUS_ORDER) {
    out[status] =
      tasksByStatus[status].length >= 1 &&
      status !== "done" &&
      status !== "cancelled";
  }
  return out;
}

// TODO(T8): mobile Tasks list smoke test.
function MobileTaskList(props: {
  tasksByStatus: Record<TaskStatus, Task[]>;
  agents: AgentRow[];
  statusFilter: TaskStatus | "all";
  onShowAll: () => void;
  onCardClick: (id: string) => void;
  onMove: (id: string, s: TaskStatus) => void;
}): JSX.Element {
  const [openSections, setOpenSections] = createSignal<
    Record<TaskStatus, boolean>
  >(initialOpenState(props.tasksByStatus));
  let taskCounts = Object.fromEntries(
    STATUS_ORDER.map((status) => [
      status,
      props.tasksByStatus[status].length,
    ]),
  ) as Record<TaskStatus, number>;

  createEffect(() => {
    const nextCounts = Object.fromEntries(
      STATUS_ORDER.map((status) => [
        status,
        props.tasksByStatus[status].length,
      ]),
    ) as Record<TaskStatus, number>;
    const prevCounts = taskCounts;

    setOpenSections((prev) => {
      const next = { ...prev };
      for (const status of STATUS_ORDER) {
        if (
          prevCounts[status] === 0 &&
          nextCounts[status] > 0 &&
          status !== "done" &&
          status !== "cancelled"
        ) {
          next[status] = true;
        }
      }
      return next;
    });
    taskCounts = nextCounts;
  });

  const visibleStatuses = createMemo(() =>
    props.statusFilter === "all" ? STATUS_ORDER : [props.statusFilter],
  );

  const isOpen = (status: TaskStatus) =>
    props.statusFilter !== "all" || openSections()[status];

  const toggleSection = (status: TaskStatus) => {
    if (props.statusFilter !== "all") return;
    setOpenSections((prev) => ({ ...prev, [status]: !prev[status] }));
  };

  return (
    <div class="px-3 py-3">
      <Show when={props.statusFilter !== "all"}>
        <div class="px-3 py-1 text-xs text-fg-3">
          Filtered to {STATUS_LABEL[props.statusFilter as TaskStatus]}.{" "}
          <button
            type="button"
            onClick={props.onShowAll}
            class="underline hover:text-fg-1"
          >
            Show all
          </button>
        </div>
      </Show>
      <For each={visibleStatuses()}>
        {(status) => {
          const sectionId = `tasks-mobile-section-${status}`;
          const tasks = () => props.tasksByStatus[status];
          const sectionOpen = () => isOpen(status);

          return (
            <section class={`rounded border ${STATUS_ACCENT[status]} mb-2`}>
              <button
                type="button"
                class="min-h-[44px] w-full px-3 gap-2 text-left flex items-center"
                aria-expanded={sectionOpen()}
                aria-controls={sectionId}
                onClick={() => toggleSection(status)}
              >
                <Icon
                  name="chevron-right"
                  size={16}
                  class={sectionOpen() ? "transform rotate-90" : ""}
                />
                <span class="text-xs uppercase tracking-wider">
                  {STATUS_LABEL[status]}
                </span>
                <span class="ml-auto text-xs opacity-70">
                  {tasks().length}
                </span>
              </button>
              <Show when={sectionOpen()}>
                <div id={sectionId}>
                  <Show when={tasks().length === 0}>
                    <div class="px-3 py-2 text-xs text-fg-3 italic">
                      No tasks.
                    </div>
                  </Show>
                  <For each={tasks()}>
                    {(task) => (
                      <MobileTaskCard
                        task={task}
                        agents={props.agents}
                        onCardClick={props.onCardClick}
                        onMove={props.onMove}
                      />
                    )}
                  </For>
                </div>
              </Show>
            </section>
          );
        }}
      </For>
    </div>
  );
}

function MobileTaskCard(props: {
  task: Task;
  agents: AgentRow[];
  onCardClick: (id: string) => void;
  onMove: (id: string, s: TaskStatus) => void;
}): JSX.Element {
  return (
    <article class="flex gap-2 px-3 py-2 border-t border-border-subtle">
      <button
        type="button"
        onClick={() => props.onCardClick(props.task.id)}
        class="min-h-[44px] flex-1 text-left text-sm"
      >
        <div class="font-medium line-clamp-2">{props.task.title}</div>
        <div class="mt-1 flex items-center justify-between text-xs text-fg-2">
          <span>{decorateAssignee(props.task.assignee, props.agents)}</span>
          <span>
            <Show when={props.task.todos.length > 0}>
              <span>
                {
                  props.task.todos.filter((t) => t.status === "completed")
                    .length
                }
                /{props.task.todos.length} todos
              </span>
            </Show>
            <Show when={props.task.blocked_by.length > 0}>
              <span class="ml-2 text-warning-fg">
                {props.task.blocked_by.length} blocking
              </span>
            </Show>
          </span>
        </div>
        <Show
          when={props.task.status === "planning" && props.task.planned_assignee}
        >
          <div class="mt-1 text-xs text-fg-3 italic">
            → on approve:{" "}
            {decorateAssignee(props.task.planned_assignee, props.agents)}
          </div>
        </Show>
      </button>
      <Menu
        align="end"
        sections={[
          {
            title: "Move to…",
            items: STATUS_ORDER.filter((s) => s !== props.task.status).map(
              (s) => ({
                id: s,
                label: `Move to ${STATUS_LABEL[s]}`,
                danger: s === "cancelled",
                onSelect: () => props.onMove(props.task.id, s),
              }),
            ),
          },
        ]}
        trigger={(api) => (
          <button
            type="button"
            aria-label="Change status"
            aria-haspopup="menu"
            class="min-h-[44px] min-w-[44px] flex items-center justify-center text-fg-2 hover:text-fg-0"
            onClick={(e) => {
              e.stopPropagation();
              api.toggle();
            }}
          >
            <Icon name="more-horizontal" size={16} />
          </button>
        )}
      />
    </article>
  );
}

function AddTaskModal(props: {
  instanceId: string;
  agents: AgentRow[];
  onClose: () => void;
  onCreated: () => void;
  onError: (e: string) => void;
}): JSX.Element {
  const [title, setTitle] = createSignal("");
  const [description, setDescription] = createSignal("");
  const [assignee, setAssignee] = createSignal("");
  const [submitting, setSubmitting] = createSignal(false);

  const assigneeCandidates = () => buildAssigneeOptions(props.agents);

  const submit = async (e: SubmitEvent) => {
    e.preventDefault();
    if (title().trim() === "") return;
    setSubmitting(true);
    try {
      const resp = await api.createTask(props.instanceId, {
        title: title().trim(),
        description: description().trim() || undefined,
        assignee: assignee() || undefined,
      });
      if (!resp.ok) {
        props.onError(resp.error ?? "create failed");
        return;
      }
      props.onCreated();
    } catch (err) {
      props.onError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <ModalShell onClose={props.onClose} title="New task">
      <form onSubmit={submit} class="space-y-3">
        <label class="block">
          <span class="text-xs text-fg-2">Title</span>
          <input
            type="text"
            required
            value={title()}
            onInput={(e) => setTitle(e.currentTarget.value)}
            class="mt-1 w-full rounded border border-border bg-bg-0 px-2 py-1 text-sm"
          />
        </label>
        <label class="block">
          <span class="text-xs text-fg-2">Description</span>
          <textarea
            rows={3}
            value={description()}
            onInput={(e) => setDescription(e.currentTarget.value)}
            class="mt-1 w-full rounded border border-border bg-bg-0 px-2 py-1 text-sm"
          />
        </label>
        <label class="block">
          <span class="text-xs text-fg-2">Assignee</span>
          <select
            value={assignee()}
            onInput={(e) => setAssignee(e.currentTarget.value)}
            class="mt-1 w-full rounded border border-border bg-bg-0 px-2 py-1 text-sm"
          >
            <option value="">(unassigned)</option>
            <For each={assigneeCandidates()}>
              {(a) => <option value={a.value}>{a.label}</option>}
            </For>
          </select>
        </label>
        <div class="flex justify-end gap-2">
          <button
            type="button"
            onClick={props.onClose}
            class="text-sm text-fg-2 hover:text-fg-0"
          >
            cancel
          </button>
          <button
            type="submit"
            disabled={submitting() || title().trim() === ""}
            class="rounded border border-success bg-success-bg hover:bg-success disabled:opacity-50 px-3 py-1 text-sm"
          >
            {submitting() ? "Creating…" : "Create"}
          </button>
        </div>
      </form>
    </ModalShell>
  );
}

function TaskDetailModal(props: {
  instanceId: string;
  task: Task;
  allTasks: Task[];
  agents: AgentRow[];
  onClose: () => void;
  onPatched: () => void;
  onDeleted: () => void;
  onError: (e: string) => void;
}): JSX.Element {
  const [title, setTitle] = createSignal(props.task.title);
  const [description, setDescription] = createSignal(
    props.task.description ?? "",
  );
  const [saving, setSaving] = createSignal(false);
  const [newTodo, setNewTodo] = createSignal("");
  const [newComment, setNewComment] = createSignal("");
  const [editingCommentId, setEditingCommentId] = createSignal<string | null>(
    null,
  );
  const [editingCommentBody, setEditingCommentBody] = createSignal("");
  const [blockedBySelect, setBlockedBySelect] = createSignal("");

  createEffect(() => {
    // Keep the inline fields in sync when the underlying task changes
    // (refetch after a mutation).
    setTitle(props.task.title);
    setDescription(props.task.description ?? "");
  });

  const saveTitle = async () => {
    const t = title().trim();
    if (!t || t === props.task.title) return;
    setSaving(true);
    const resp = await api.patchTask(props.instanceId, props.task.id, {
      title: t,
    });
    setSaving(false);
    if (!resp.ok) props.onError(resp.error ?? "title update failed");
    else props.onPatched();
  };

  const saveDescription = async () => {
    const d = description();
    if (d === (props.task.description ?? "")) return;
    setSaving(true);
    const resp = await api.patchTask(props.instanceId, props.task.id, {
      description: d,
    });
    setSaving(false);
    if (!resp.ok) props.onError(resp.error ?? "description update failed");
    else props.onPatched();
  };

  const changeStatus = async (status: TaskStatus) => {
    const resp = await api.patchTask(props.instanceId, props.task.id, {
      status,
    });
    if (!resp.ok) props.onError(resp.error ?? "status update failed");
    else props.onPatched();
  };

  const changeAssignee = async (assignee: string) => {
    const resp = await api.patchTask(
      props.instanceId,
      props.task.id,
      assignee === ""
        ? { unassign: true }
        : { assignee },
    );
    if (!resp.ok) props.onError(resp.error ?? "assignee update failed");
    else props.onPatched();
  };

  const addTodo = async () => {
    const body = newTodo().trim();
    if (!body) return;
    const resp = await api.addTodo(props.instanceId, props.task.id, body);
    if (!resp.ok) props.onError(resp.error ?? "add todo failed");
    else {
      setNewTodo("");
      props.onPatched();
    }
  };

  const toggleTodoStatus = async (todoId: string, cur: TodoStatus) => {
    const cycle: Record<TodoStatus, TodoStatus> = {
      pending: "in_progress",
      in_progress: "completed",
      completed: "pending",
    };
    const next = cycle[cur];
    const resp = await api.patchTodo(
      props.instanceId,
      props.task.id,
      todoId,
      { status: next },
    );
    if (!resp.ok) props.onError(resp.error ?? "todo update failed");
    else props.onPatched();
  };

  const removeTodo = async (todoId: string) => {
    const resp = await api.deleteTodo(
      props.instanceId,
      props.task.id,
      todoId,
    );
    if (!resp.ok) props.onError(resp.error ?? "todo delete failed");
    else props.onPatched();
  };

  const addComment = async () => {
    const body = newComment().trim();
    if (!body) return;
    const resp = await api.addComment(props.instanceId, props.task.id, body);
    if (!resp.ok) props.onError(resp.error ?? "add comment failed");
    else {
      setNewComment("");
      props.onPatched();
    }
  };

  const saveEditedComment = async () => {
    const id = editingCommentId();
    if (!id) return;
    const body = editingCommentBody().trim();
    if (!body) return;
    const resp = await api.patchComment(
      props.instanceId,
      props.task.id,
      id,
      body,
    );
    if (!resp.ok) {
      props.onError(resp.error ?? "comment update failed");
    } else {
      setEditingCommentId(null);
      setEditingCommentBody("");
      props.onPatched();
    }
  };

  const removeComment = async (commentId: string) => {
    const resp = await api.deleteComment(
      props.instanceId,
      props.task.id,
      commentId,
    );
    if (!resp.ok) props.onError(resp.error ?? "comment delete failed");
    else props.onPatched();
  };

  const addBlockedBy = async () => {
    const id = blockedBySelect();
    if (!id) return;
    const resp = await api.patchTask(props.instanceId, props.task.id, {
      add_blocked_by: [id],
    });
    if (!resp.ok) props.onError(resp.error ?? "add dependency failed");
    else {
      setBlockedBySelect("");
      props.onPatched();
    }
  };

  const removeBlockedBy = async (depId: string) => {
    const resp = await api.patchTask(props.instanceId, props.task.id, {
      remove_blocked_by: [depId],
    });
    if (!resp.ok) props.onError(resp.error ?? "remove dependency failed");
    else props.onPatched();
  };

  const deleteTask = async () => {
    if (!confirm(`Delete task "${props.task.title}"? This cannot be undone.`))
      return;
    const resp = await api.deleteTask(props.instanceId, props.task.id);
    if (!resp.ok) props.onError(resp.error ?? "delete failed");
    else props.onDeleted();
  };

  const approveTask = async () => {
    const target = props.task.planned_assignee;
    if (!target) {
      props.onError("approve: task has no planned_assignee");
      return;
    }
    const resp = await api.approveTask(
      props.instanceId,
      props.task.id,
      target,
    );
    if (!resp.ok) props.onError(resp.error ?? "approve failed");
    else props.onPatched();
  };

  const rejectTask = async () => {
    const reason = window.prompt(
      "Why are you rejecting this plan? Your reason becomes a comment and the task is sent back to the proposing agent for revision.",
    );
    if (reason === null) return;
    const trimmed = reason.trim();
    if (!trimmed) {
      props.onError("reject: a reason is required");
      return;
    }
    const resp = await api.rejectTask(props.instanceId, props.task.id, trimmed);
    if (!resp.ok) props.onError(resp.error ?? "reject failed");
    else props.onPatched();
  };

  const taskById = (id: string): Task | undefined =>
    props.allTasks.find((t) => t.id === id);

  const assigneeCandidates = () => buildAssigneeOptions(props.agents);

  return (
    <ModalShell onClose={props.onClose} title={`Task · ${props.task.id.slice(0, 8)}`}>
      <div class="space-y-4 text-sm">
        <section class="space-y-2">
          <label class="block">
            <span class="text-xs text-fg-2">Title</span>
            <input
              type="text"
              value={title()}
              onInput={(e) => setTitle(e.currentTarget.value)}
              onBlur={saveTitle}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  (e.currentTarget as HTMLInputElement).blur();
                }
              }}
              class="mt-1 w-full rounded border border-border bg-bg-0 px-2 py-1"
            />
          </label>
          <label class="block">
            <span class="text-xs text-fg-2">Description</span>
            <textarea
              rows={4}
              value={description()}
              onInput={(e) => setDescription(e.currentTarget.value)}
              onBlur={saveDescription}
              class="mt-1 w-full rounded border border-border bg-bg-0 px-2 py-1"
            />
          </label>
          <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
            <label class="block">
              <span class="text-xs text-fg-2">Status</span>
              <select
                value={props.task.status}
                onInput={(e) =>
                  changeStatus(e.currentTarget.value as TaskStatus)
                }
                class="mt-1 w-full rounded border border-border bg-bg-0 px-2 py-1"
              >
                <For each={STATUS_ORDER}>
                  {(s) => <option value={s}>{STATUS_LABEL[s]}</option>}
                </For>
              </select>
            </label>
            <label class="block">
              <span class="text-xs text-fg-2">Assignee</span>
              <select
                value={props.task.assignee ?? ""}
                onInput={(e) => changeAssignee(e.currentTarget.value)}
                class="mt-1 w-full rounded border border-border bg-bg-0 px-2 py-1"
              >
                <option value="">(unassigned)</option>
                <For each={assigneeCandidates()}>
                  {(a) => <option value={a.value}>{a.label}</option>}
                </For>
              </select>
            </label>
          </div>
          <div class="text-xs text-fg-3">
            created by <span class="font-mono">{props.task.created_by}</span>
            {" · "}
            {new Date(props.task.created_at_secs * 1000).toLocaleString()}
          </div>
        </section>

        <Show when={props.task.status === "planning"}>
          <section class="rounded border border-border bg-bg-1 px-3 py-2">
            <div class="flex items-center justify-between gap-2">
              <div>
                <div class="text-xs uppercase tracking-wider text-fg-2">
                  Planned assignee
                </div>
                <div class="font-medium">
                  {decorateAssignee(
                    props.task.planned_assignee,
                    props.agents,
                  )}
                </div>
                <div class="text-xs text-fg-3 mt-1">
                  Approve to route this task to{" "}
                  {props.task.planned_assignee ?? "(none)"} (status will
                  flip to <code>approved</code>).
                </div>
              </div>
              <div class="flex gap-2">
                <Button
                  size="sm"
                  variant="primary"
                  disabled={
                    !props.task.planned_assignee ||
                    props.task.assignee !== USER_ASSIGNEE_ID
                  }
                  onClick={approveTask}
                >
                  Approve
                </Button>
                <Button
                  size="sm"
                  variant="danger"
                  disabled={props.task.assignee !== USER_ASSIGNEE_ID}
                  onClick={rejectTask}
                >
                  Reject
                </Button>
              </div>
            </div>
          </section>
        </Show>

        <section>
          <h3 class="text-xs uppercase tracking-wider text-fg-2 mb-1">
            Blocked by
          </h3>
          <Show
            when={props.task.blocked_by.length > 0}
            fallback={
              <div class="text-xs text-fg-3 italic">
                No dependencies.
              </div>
            }
          >
            <ul class="space-y-1">
              <For each={props.task.blocked_by}>
                {(depId) => {
                  const dep = taskById(depId);
                  return (
                    <li class="flex items-center justify-between rounded border border-border-subtle bg-bg-1 px-2 py-1">
                      <span>
                        <Show
                          when={dep}
                          fallback={
                            <span class="font-mono text-xs">
                              {depId.slice(0, 8)} (unknown)
                            </span>
                          }
                        >
                          {(d) => (
                            <>
                              <span class="font-medium">{d().title}</span>
                              <span class="ml-2 text-xs text-fg-2">
                                ({STATUS_LABEL[d().status]})
                              </span>
                            </>
                          )}
                        </Show>
                      </span>
                      <button
                        type="button"
                        onClick={() => removeBlockedBy(depId)}
                        class="text-xs text-danger-fg hover:text-danger-fg"
                      >
                        remove
                      </button>
                    </li>
                  );
                }}
              </For>
            </ul>
          </Show>
          <div class="mt-2 flex gap-2">
            <select
              value={blockedBySelect()}
              onInput={(e) => setBlockedBySelect(e.currentTarget.value)}
              class="flex-1 rounded border border-border bg-bg-0 px-2 py-1 text-sm"
            >
              <option value="">(select task to block on)</option>
              <For
                each={props.allTasks.filter(
                  (t) =>
                    t.id !== props.task.id &&
                    !props.task.blocked_by.includes(t.id),
                )}
              >
                {(t) => (
                  <option value={t.id}>
                    {t.title} ({STATUS_LABEL[t.status]})
                  </option>
                )}
              </For>
            </select>
            <button
              type="button"
              onClick={addBlockedBy}
              disabled={!blockedBySelect()}
              class="rounded border border-border hover:bg-bg-2 px-2 py-1 text-xs disabled:opacity-50"
            >
              + block
            </button>
          </div>
        </section>

        <section>
          <h3 class="text-xs uppercase tracking-wider text-fg-2 mb-1">
            Todos
          </h3>
          <For each={TODO_ORDER}>
            {(group) => {
              const groupTodos = () =>
                props.task.todos.filter((td) => td.status === group);
              return (
                <Show when={groupTodos().length > 0}>
                  <div class="mb-2">
                    <div class="text-xs text-fg-3 mb-1">
                      {TODO_LABEL[group]} ({groupTodos().length})
                    </div>
                    <ul class="space-y-1">
                      <For each={groupTodos()}>
                        {(todo) => (
                          <li class="flex items-center justify-between rounded border border-border-subtle bg-bg-1 px-2 py-1">
                            <button
                              type="button"
                              onClick={() =>
                                toggleTodoStatus(todo.id, todo.status)
                              }
                              class="font-mono text-sm mr-2"
                              title="click to cycle status"
                            >
                              <Switch>
                                <Match when={todo.status === "pending"}>
                                  □
                                </Match>
                                <Match when={todo.status === "in_progress"}>
                                  ◧
                                </Match>
                                <Match when={todo.status === "completed"}>
                                  ■
                                </Match>
                              </Switch>
                            </button>
                            <span class="flex-1">{todo.body}</span>
                            <button
                              type="button"
                              onClick={() => removeTodo(todo.id)}
                              class="text-xs text-danger-fg hover:text-danger-fg ml-2"
                            >
                              remove
                            </button>
                          </li>
                        )}
                      </For>
                    </ul>
                  </div>
                </Show>
              );
            }}
          </For>
          <div class="mt-2 flex gap-2">
            <input
              type="text"
              value={newTodo()}
              onInput={(e) => setNewTodo(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  void addTodo();
                }
              }}
              placeholder="Add a todo…"
              class="flex-1 rounded border border-border bg-bg-0 px-2 py-1 text-sm"
            />
            <button
              type="button"
              onClick={addTodo}
              disabled={newTodo().trim() === ""}
              class="rounded border border-border hover:bg-bg-2 px-2 py-1 text-xs disabled:opacity-50"
            >
              + todo
            </button>
          </div>
        </section>

        <section>
          <h3 class="text-xs uppercase tracking-wider text-fg-2 mb-1">
            Comments
          </h3>
          <Show
            when={props.task.comments.length > 0}
            fallback={
              <div class="text-xs text-fg-3 italic">No comments.</div>
            }
          >
            <ul class="space-y-2">
              <For each={props.task.comments}>
                {(c) => (
                  <li class="rounded border border-border-subtle bg-bg-1 px-2 py-1">
                    <div class="flex items-center justify-between text-xs text-fg-2">
                      <span>
                        <span class="font-mono">{c.author}</span>
                        {" · "}
                        {new Date(
                          c.timestamp_secs * 1000,
                        ).toLocaleString()}
                      </span>
                      <div class="flex gap-2">
                        <button
                          type="button"
                          onClick={() => {
                            setEditingCommentId(c.id);
                            setEditingCommentBody(c.body);
                          }}
                          class="text-accent-fg hover:text-accent-fg"
                        >
                          edit
                        </button>
                        <button
                          type="button"
                          onClick={() => removeComment(c.id)}
                          class="text-danger-fg hover:text-danger-fg"
                        >
                          remove
                        </button>
                      </div>
                    </div>
                    <Show
                      when={editingCommentId() === c.id}
                      fallback={
                        <pre class="mt-1 whitespace-pre-wrap break-words text-sm">
                          {c.body}
                        </pre>
                      }
                    >
                      <textarea
                        rows={2}
                        value={editingCommentBody()}
                        onInput={(e) =>
                          setEditingCommentBody(e.currentTarget.value)
                        }
                        class="mt-1 w-full rounded border border-border bg-bg-0 px-2 py-1 text-sm"
                      />
                      <div class="flex justify-end gap-2 mt-1">
                        <button
                          type="button"
                          onClick={() => {
                            setEditingCommentId(null);
                            setEditingCommentBody("");
                          }}
                          class="text-xs text-fg-2 hover:text-fg-0"
                        >
                          cancel
                        </button>
                        <button
                          type="button"
                          onClick={saveEditedComment}
                          class="text-xs text-success-fg hover:text-success-fg"
                        >
                          save
                        </button>
                      </div>
                    </Show>
                  </li>
                )}
              </For>
            </ul>
          </Show>
          <div class="mt-2 flex gap-2">
            <input
              type="text"
              value={newComment()}
              onInput={(e) => setNewComment(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  void addComment();
                }
              }}
              placeholder="Add a comment…"
              class="flex-1 rounded border border-border bg-bg-0 px-2 py-1 text-sm"
            />
            <button
              type="button"
              onClick={addComment}
              disabled={newComment().trim() === ""}
              class="rounded border border-border hover:bg-bg-2 px-2 py-1 text-xs disabled:opacity-50"
            >
              + comment
            </button>
          </div>
        </section>

        <section>
          <h3 class="text-xs uppercase tracking-wider text-fg-2 mb-1">
            System notes
          </h3>
          <Show
            when={props.task.notes.length > 0}
            fallback={
              <div class="text-xs text-fg-3 italic">No notes.</div>
            }
          >
            <ul class="space-y-1">
              <For each={props.task.notes}>
                {(n) => (
                  <li class="rounded bg-bg-1 px-2 py-1 text-xs">
                    <span class="font-mono text-fg-2">{n.author}</span>
                    {" · "}
                    <span class="text-fg-3">
                      {new Date(n.timestamp_secs * 1000).toLocaleString()}
                    </span>
                    <div class="mt-0.5 text-fg-1">{n.body}</div>
                  </li>
                )}
              </For>
            </ul>
          </Show>
        </section>

        <div class="flex justify-between border-t border-border-subtle pt-3">
          <button
            type="button"
            onClick={deleteTask}
            class="text-sm text-danger-fg hover:text-danger-fg"
          >
            Delete task
          </button>
          <div class="text-xs text-fg-3">
            <Show when={saving()}>saving…</Show>
          </div>
        </div>
      </div>
    </ModalShell>
  );
}

function ModalShell(props: {
  title: string;
  onClose: () => void;
  children: JSX.Element;
}): JSX.Element {
  return (
    <div
      class="fixed inset-0 z-50 bg-black/60 flex items-start justify-center overflow-y-auto"
      onClick={props.onClose}
    >
      <div
        class="my-10 w-full max-w-2xl rounded border border-border bg-bg-1 p-4 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <header class="flex items-center justify-between mb-3">
          <h2 class="text-base font-semibold">{props.title}</h2>
          <button
            type="button"
            onClick={props.onClose}
            class="text-fg-2 hover:text-fg-0"
          >
            ✕
          </button>
        </header>
        {props.children}
      </div>
    </div>
  );
}
