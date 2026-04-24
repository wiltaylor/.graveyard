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

interface Props {
  instanceId: string;
  agents: AgentRow[];
}

const STATUS_ORDER: TaskStatus[] = [
  "open",
  "assigned",
  "in_progress",
  "done",
  "cancelled",
];

const STATUS_LABEL: Record<TaskStatus, string> = {
  open: "Open",
  assigned: "Assigned",
  in_progress: "In progress",
  done: "Done",
  cancelled: "Cancelled",
};

const TODO_ORDER: TodoStatus[] = ["pending", "in_progress", "completed"];

const TODO_LABEL: Record<TodoStatus, string> = {
  pending: "Pending",
  in_progress: "In progress",
  completed: "Completed",
};

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
      open: [],
      assigned: [],
      in_progress: [],
      done: [],
      cancelled: [],
    };
    for (const t of tasks()) {
      out[t.status].push(t);
    }
    return out;
  });

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
      <header class="flex items-center justify-between px-4 py-2 border-b border-slate-800 bg-slate-900/40">
        <h2 class="text-lg font-semibold">Tasks</h2>
        <div class="flex items-center gap-2">
          <Show when={loading()}>
            <span class="text-xs text-slate-400">loading…</span>
          </Show>
          <button
            type="button"
            onClick={() => setShowAddModal(true)}
            class="rounded border border-emerald-700 bg-emerald-900/40 hover:bg-emerald-800 px-3 py-1 text-sm"
          >
            + New task
          </button>
          <button
            onClick={loadTasks}
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

      <div class="flex-1 overflow-x-auto overflow-y-hidden px-4 py-3">
        <div class="grid grid-cols-5 gap-3 h-full min-w-[60rem]">
          <For each={STATUS_ORDER}>
            {(status) => (
              <KanbanColumn
                status={status}
                tasks={tasksByStatus()[status]}
                onDrop={(taskId) => moveTask(taskId, status)}
                onCardClick={(taskId) => setOpenTaskId(taskId)}
              />
            )}
          </For>
        </div>
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
  onDrop: (taskId: string) => void;
  onCardClick: (taskId: string) => void;
}): JSX.Element {
  const [dragOver, setDragOver] = createSignal(false);

  return (
    <section
      class="flex flex-col rounded border bg-slate-950/40 min-w-[14rem]"
      classList={{
        "border-slate-800": !dragOver(),
        "border-sky-600 bg-sky-950/30": dragOver(),
      }}
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
      <header class="px-2 py-1.5 border-b border-slate-800 flex items-center justify-between">
        <h3 class="text-xs uppercase tracking-wider text-slate-300">
          {STATUS_LABEL[props.status]}
        </h3>
        <span class="text-xs text-slate-500">{props.tasks.length}</span>
      </header>
      <div class="flex-1 overflow-y-auto p-2 space-y-2">
        <For each={props.tasks}>
          {(t) => (
            <TaskCard task={t} onClick={() => props.onCardClick(t.id)} />
          )}
        </For>
      </div>
    </section>
  );
}

function TaskCard(props: { task: Task; onClick: () => void }): JSX.Element {
  return (
    <article
      draggable={true}
      onDragStart={(e) => {
        e.dataTransfer?.setData("text/plain", props.task.id);
        if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
      }}
      onClick={props.onClick}
      class="rounded border border-slate-700 bg-slate-900 p-2 text-sm cursor-pointer hover:border-slate-500"
    >
      <div class="font-medium line-clamp-2">{props.task.title}</div>
      <div class="mt-1 flex items-center justify-between text-xs text-slate-400">
        <span>{props.task.assignee ?? "unassigned"}</span>
        <span>
          <Show when={props.task.todos.length > 0}>
            <span>
              {props.task.todos.filter((t) => t.status === "completed").length}
              /{props.task.todos.length} todos
            </span>
          </Show>
          <Show when={props.task.blocked_by.length > 0}>
            <span class="ml-2 text-amber-400">
              {props.task.blocked_by.length} blocking
            </span>
          </Show>
        </span>
      </div>
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

  const assigneeCandidates = () =>
    props.agents.filter((a) => a.kind === "agent" || a.kind === "automaton");

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
          <span class="text-xs text-slate-400">Title</span>
          <input
            type="text"
            required
            value={title()}
            onInput={(e) => setTitle(e.currentTarget.value)}
            class="mt-1 w-full rounded border border-slate-700 bg-slate-950 px-2 py-1 text-sm"
          />
        </label>
        <label class="block">
          <span class="text-xs text-slate-400">Description</span>
          <textarea
            rows={3}
            value={description()}
            onInput={(e) => setDescription(e.currentTarget.value)}
            class="mt-1 w-full rounded border border-slate-700 bg-slate-950 px-2 py-1 text-sm"
          />
        </label>
        <label class="block">
          <span class="text-xs text-slate-400">Assignee</span>
          <select
            value={assignee()}
            onInput={(e) => setAssignee(e.currentTarget.value)}
            class="mt-1 w-full rounded border border-slate-700 bg-slate-950 px-2 py-1 text-sm"
          >
            <option value="">(unassigned)</option>
            <For each={assigneeCandidates()}>
              {(a) => (
                <option value={a.id}>
                  {a.id} ({a.kind})
                </option>
              )}
            </For>
          </select>
        </label>
        <div class="flex justify-end gap-2">
          <button
            type="button"
            onClick={props.onClose}
            class="text-sm text-slate-400 hover:text-slate-100"
          >
            cancel
          </button>
          <button
            type="submit"
            disabled={submitting() || title().trim() === ""}
            class="rounded border border-emerald-700 bg-emerald-900/40 hover:bg-emerald-800 disabled:opacity-50 px-3 py-1 text-sm"
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

  const taskById = (id: string): Task | undefined =>
    props.allTasks.find((t) => t.id === id);

  const assigneeCandidates = () =>
    props.agents.filter((a) => a.kind === "agent" || a.kind === "automaton");

  return (
    <ModalShell onClose={props.onClose} title={`Task · ${props.task.id.slice(0, 8)}`}>
      <div class="space-y-4 text-sm">
        <section class="space-y-2">
          <label class="block">
            <span class="text-xs text-slate-400">Title</span>
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
              class="mt-1 w-full rounded border border-slate-700 bg-slate-950 px-2 py-1"
            />
          </label>
          <label class="block">
            <span class="text-xs text-slate-400">Description</span>
            <textarea
              rows={4}
              value={description()}
              onInput={(e) => setDescription(e.currentTarget.value)}
              onBlur={saveDescription}
              class="mt-1 w-full rounded border border-slate-700 bg-slate-950 px-2 py-1"
            />
          </label>
          <div class="grid grid-cols-2 gap-2">
            <label class="block">
              <span class="text-xs text-slate-400">Status</span>
              <select
                value={props.task.status}
                onInput={(e) =>
                  changeStatus(e.currentTarget.value as TaskStatus)
                }
                class="mt-1 w-full rounded border border-slate-700 bg-slate-950 px-2 py-1"
              >
                <For each={STATUS_ORDER}>
                  {(s) => <option value={s}>{STATUS_LABEL[s]}</option>}
                </For>
              </select>
            </label>
            <label class="block">
              <span class="text-xs text-slate-400">Assignee</span>
              <select
                value={props.task.assignee ?? ""}
                onInput={(e) => changeAssignee(e.currentTarget.value)}
                class="mt-1 w-full rounded border border-slate-700 bg-slate-950 px-2 py-1"
              >
                <option value="">(unassigned)</option>
                <For each={assigneeCandidates()}>
                  {(a) => (
                    <option value={a.id}>
                      {a.id} ({a.kind})
                    </option>
                  )}
                </For>
              </select>
            </label>
          </div>
          <div class="text-xs text-slate-500">
            created by <span class="font-mono">{props.task.created_by}</span>
            {" · "}
            {new Date(props.task.created_at_secs * 1000).toLocaleString()}
          </div>
        </section>

        <section>
          <h3 class="text-xs uppercase tracking-wider text-slate-400 mb-1">
            Blocked by
          </h3>
          <Show
            when={props.task.blocked_by.length > 0}
            fallback={
              <div class="text-xs text-slate-500 italic">
                No dependencies.
              </div>
            }
          >
            <ul class="space-y-1">
              <For each={props.task.blocked_by}>
                {(depId) => {
                  const dep = taskById(depId);
                  return (
                    <li class="flex items-center justify-between rounded border border-slate-800 bg-slate-900/60 px-2 py-1">
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
                              <span class="ml-2 text-xs text-slate-400">
                                ({STATUS_LABEL[d().status]})
                              </span>
                            </>
                          )}
                        </Show>
                      </span>
                      <button
                        type="button"
                        onClick={() => removeBlockedBy(depId)}
                        class="text-xs text-red-300 hover:text-red-100"
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
              class="flex-1 rounded border border-slate-700 bg-slate-950 px-2 py-1 text-sm"
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
              class="rounded border border-slate-700 hover:bg-slate-800 px-2 py-1 text-xs disabled:opacity-50"
            >
              + block
            </button>
          </div>
        </section>

        <section>
          <h3 class="text-xs uppercase tracking-wider text-slate-400 mb-1">
            Todos
          </h3>
          <For each={TODO_ORDER}>
            {(group) => {
              const groupTodos = () =>
                props.task.todos.filter((td) => td.status === group);
              return (
                <Show when={groupTodos().length > 0}>
                  <div class="mb-2">
                    <div class="text-xs text-slate-500 mb-1">
                      {TODO_LABEL[group]} ({groupTodos().length})
                    </div>
                    <ul class="space-y-1">
                      <For each={groupTodos()}>
                        {(todo) => (
                          <li class="flex items-center justify-between rounded border border-slate-800 bg-slate-900/60 px-2 py-1">
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
                              class="text-xs text-red-300 hover:text-red-100 ml-2"
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
              class="flex-1 rounded border border-slate-700 bg-slate-950 px-2 py-1 text-sm"
            />
            <button
              type="button"
              onClick={addTodo}
              disabled={newTodo().trim() === ""}
              class="rounded border border-slate-700 hover:bg-slate-800 px-2 py-1 text-xs disabled:opacity-50"
            >
              + todo
            </button>
          </div>
        </section>

        <section>
          <h3 class="text-xs uppercase tracking-wider text-slate-400 mb-1">
            Comments
          </h3>
          <Show
            when={props.task.comments.length > 0}
            fallback={
              <div class="text-xs text-slate-500 italic">No comments.</div>
            }
          >
            <ul class="space-y-2">
              <For each={props.task.comments}>
                {(c) => (
                  <li class="rounded border border-slate-800 bg-slate-900/60 px-2 py-1">
                    <div class="flex items-center justify-between text-xs text-slate-400">
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
                          class="text-sky-300 hover:text-sky-100"
                        >
                          edit
                        </button>
                        <button
                          type="button"
                          onClick={() => removeComment(c.id)}
                          class="text-red-300 hover:text-red-100"
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
                        class="mt-1 w-full rounded border border-slate-700 bg-slate-950 px-2 py-1 text-sm"
                      />
                      <div class="flex justify-end gap-2 mt-1">
                        <button
                          type="button"
                          onClick={() => {
                            setEditingCommentId(null);
                            setEditingCommentBody("");
                          }}
                          class="text-xs text-slate-400 hover:text-slate-100"
                        >
                          cancel
                        </button>
                        <button
                          type="button"
                          onClick={saveEditedComment}
                          class="text-xs text-emerald-300 hover:text-emerald-100"
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
              class="flex-1 rounded border border-slate-700 bg-slate-950 px-2 py-1 text-sm"
            />
            <button
              type="button"
              onClick={addComment}
              disabled={newComment().trim() === ""}
              class="rounded border border-slate-700 hover:bg-slate-800 px-2 py-1 text-xs disabled:opacity-50"
            >
              + comment
            </button>
          </div>
        </section>

        <section>
          <h3 class="text-xs uppercase tracking-wider text-slate-400 mb-1">
            System notes
          </h3>
          <Show
            when={props.task.notes.length > 0}
            fallback={
              <div class="text-xs text-slate-500 italic">No notes.</div>
            }
          >
            <ul class="space-y-1">
              <For each={props.task.notes}>
                {(n) => (
                  <li class="rounded bg-slate-900/40 px-2 py-1 text-xs">
                    <span class="font-mono text-slate-400">{n.author}</span>
                    {" · "}
                    <span class="text-slate-500">
                      {new Date(n.timestamp_secs * 1000).toLocaleString()}
                    </span>
                    <div class="mt-0.5 text-slate-300">{n.body}</div>
                  </li>
                )}
              </For>
            </ul>
          </Show>
        </section>

        <div class="flex justify-between border-t border-slate-800 pt-3">
          <button
            type="button"
            onClick={deleteTask}
            class="text-sm text-red-300 hover:text-red-100"
          >
            Delete task
          </button>
          <div class="text-xs text-slate-500">
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
        class="my-10 w-full max-w-2xl rounded border border-slate-700 bg-slate-900 p-4 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <header class="flex items-center justify-between mb-3">
          <h2 class="text-base font-semibold">{props.title}</h2>
          <button
            type="button"
            onClick={props.onClose}
            class="text-slate-400 hover:text-slate-100"
          >
            ✕
          </button>
        </header>
        {props.children}
      </div>
    </div>
  );
}
