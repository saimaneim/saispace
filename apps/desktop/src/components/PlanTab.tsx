import {
  Archive,
  Check,
  ChevronDown,
  ChevronRight,
  ListChecks,
  Plus,
  RotateCcw,
  Target,
  Trash2,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import type { GoalWithTasks, Plan, PlanFilter, Project, Task, UpdateGoalInput } from "../lib/types";
import { cn } from "../lib/utils";

const FILTERS: { id: PlanFilter; label: string }[] = [
  { id: "active", label: "Activas" },
  { id: "completed", label: "Completadas" },
  { id: "all", label: "Todas" },
];

const doneCount = (tasks: Task[]) => tasks.filter((task) => task.status === "done").length;

function replaceTask(plan: Plan, replacement: Task): Plan {
  const replaceIn = (tasks: Task[]) =>
    tasks.map((task) => (task.id === replacement.id ? replacement : task));

  return {
    ...plan,
    goals: plan.goals.map((item) => {
      if (!item.tasks.some((task) => task.id === replacement.id)) return item;
      const tasks = replaceIn(item.tasks);
      return { ...item, tasks, done: doneCount(tasks), total: tasks.length };
    }),
    loose_tasks: replaceIn(plan.loose_tasks),
  };
}

function findTask(plan: Plan, id: string): Task | undefined {
  return (
    plan.loose_tasks.find((task) => task.id === id) ??
    plan.goals.flatMap((item) => item.tasks).find((task) => task.id === id)
  );
}

interface TaskRowProps {
  task: Task;
  pending: boolean;
  onToggle: (task: Task) => Promise<void>;
}

function TaskRow({ task, pending, onToggle }: TaskRowProps) {
  const done = task.status === "done";
  return (
    <li className="group flex h-10 min-w-0 items-center gap-3 rounded-md px-2.5 hover:bg-obsidian transition-colors duration-[120ms]">
      <span className="relative flex h-4 w-4 flex-shrink-0">
        <input
          type="checkbox"
          checked={done}
          aria-label={task.title}
          title={done ? "Marcar como pendiente" : "Marcar como hecha"}
          disabled={pending}
          onChange={() => void onToggle(task)}
          className="peer h-4 w-4 cursor-pointer appearance-none rounded-[5px] border border-smoke bg-obsidian transition-colors duration-[120ms] checked:border-acid-lime checked:bg-acid-lime hover:border-fog focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40 disabled:cursor-wait"
        />
        <Check
          aria-hidden="true"
          strokeWidth={3}
          className="pointer-events-none absolute inset-0 m-auto h-3 w-3 text-void opacity-0 transition-opacity duration-[120ms] peer-checked:opacity-100"
        />
      </span>
      <span
        title={task.title}
        className={cn(
          "min-w-0 flex-1 truncate text-sm",
          done || task.status === "cancelled" ? "text-ash line-through" : "text-bone",
        )}
      >
        {task.title}
      </span>
      {task.status === "in_progress" && (
        <span className="flex-shrink-0 rounded px-1.5 py-0.5 bg-status-amber/12 text-xs font-medium text-status-amber">
          En curso
        </span>
      )}
      {task.status === "blocked" && (
        <span className="flex-shrink-0 rounded px-1.5 py-0.5 bg-coral-red/12 text-xs font-medium text-coral-red">
          Bloqueada
        </span>
      )}
    </li>
  );
}

interface GoalSectionProps {
  item: GoalWithTasks;
  collapsed: boolean;
  busy: boolean;
  addingTask: boolean;
  confirmingDelete: boolean;
  pendingTaskIds: Set<string>;
  onToggleCollapsed: () => void;
  onToggleTask: (task: Task) => Promise<void>;
  onUpdate: (input: UpdateGoalInput) => Promise<void>;
  onDelete: () => Promise<void>;
  onConfirmDeleteChange: (confirming: boolean) => void;
  onAddTask: (title: string) => Promise<boolean>;
}

function GoalSection({
  item,
  collapsed,
  busy,
  addingTask,
  confirmingDelete,
  pendingTaskIds,
  onToggleCollapsed,
  onToggleTask,
  onUpdate,
  onDelete,
  onConfirmDeleteChange,
  onAddTask,
}: GoalSectionProps) {
  const { goal } = item;
  const [newTask, setNewTask] = useState("");
  const progress = item.total === 0 ? 0 : Math.round((item.done / item.total) * 100);
  const allDone = goal.status === "active" && item.total > 0 && item.done === item.total;

  const submitTask = async () => {
    const title = newTask.trim();
    if (!title || addingTask || busy) return;
    if (await onAddTask(title)) setNewTask("");
  };

  return (
    <article aria-label={goal.title} className="group min-w-0 overflow-hidden rounded-xl bg-carbon">
      <div className="flex min-w-0 items-start gap-1.5 px-2.5 py-2.5">
        <button
          type="button"
          onClick={onToggleCollapsed}
          aria-label={`${collapsed ? "Expandir" : "Contraer"} ${goal.title}`}
          aria-expanded={!collapsed}
          aria-controls={`goal-${goal.id}-tasks`}
          className="mt-0.5 flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-md text-fog hover:bg-obsidian hover:text-paper transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
        >
          {collapsed ? <ChevronRight className="h-4 w-4" /> : <ChevronDown className="h-4 w-4" />}
        </button>

        <button
          type="button"
          onClick={onToggleCollapsed}
          className="min-w-0 flex-1 rounded-md py-1 text-left focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
        >
          <span
            title={goal.title}
            className={cn(
              "block truncate text-sm font-semibold",
              goal.status === "active" ? "text-paper" : "text-fog",
            )}
          >
            {goal.title}
          </span>
          {goal.criteria && (
            <span className="mt-0.5 block min-w-0 break-words text-xs leading-5 text-fog">
              Hecho cuando: {goal.criteria}
            </span>
          )}
        </button>

        <div
          className={cn(
            "mt-0.5 flex flex-shrink-0 items-center gap-0.5 transition-opacity duration-[120ms]",
            confirmingDelete
              ? "opacity-100"
              : "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100",
          )}
        >
          <button
            type="button"
            disabled={busy}
            onClick={() =>
              void onUpdate({ status: goal.status === "active" ? "completed" : "active" })
            }
            aria-label={`${goal.status === "active" ? "Completar" : "Reactivar"} ${goal.title}`}
            title={goal.status === "active" ? "Completar meta" : "Reactivar meta"}
            className="flex h-7 w-7 items-center justify-center rounded-md text-fog hover:bg-obsidian hover:text-pulse-green transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40 disabled:cursor-wait"
          >
            {goal.status === "active" ? (
              <Check className="h-4 w-4" />
            ) : (
              <RotateCcw className="h-3.5 w-3.5" />
            )}
          </button>
          {goal.status !== "archived" && (
            <button
              type="button"
              disabled={busy}
              onClick={() => void onUpdate({ status: "archived" })}
              aria-label={`Archivar ${goal.title}`}
              title="Archivar meta"
              className="flex h-7 w-7 items-center justify-center rounded-md text-fog hover:bg-obsidian hover:text-paper transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40 disabled:cursor-wait"
            >
              <Archive className="h-3.5 w-3.5" />
            </button>
          )}
          <button
            type="button"
            disabled={busy}
            onClick={() => onConfirmDeleteChange(true)}
            aria-label={`Eliminar ${goal.title}`}
            title="Eliminar meta"
            className="flex h-7 w-7 items-center justify-center rounded-md text-fog hover:bg-coral-red/10 hover:text-coral-red transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40 disabled:cursor-wait"
          >
            <Trash2 className="h-3.5 w-3.5" />
          </button>
        </div>

        <span className="mt-1.5 flex-shrink-0 pl-1 pr-1.5 text-xs tabular-nums text-fog">
          {item.done}/{item.total}
        </span>
      </div>

      <div
        role="progressbar"
        aria-label={`Progreso de ${goal.title}`}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={progress}
        className="mx-3.5 h-1 overflow-hidden rounded-full bg-obsidian"
      >
        <div
          className={cn(
            "h-full rounded-full transition-[width] duration-[120ms]",
            allDone || goal.status === "completed" ? "bg-pulse-green" : "bg-acid-lime",
          )}
          style={{ width: `${progress}%` }}
        />
      </div>

      {confirmingDelete && (
        <div className="flex min-w-0 flex-wrap items-center gap-2 px-3.5 py-2.5 text-xs text-fog">
          <span className="min-w-0 flex-1 break-words">
            ¿Eliminar la meta? Sus tareas quedarán en «Sin meta».
          </span>
          <button
            type="button"
            onClick={() => onConfirmDeleteChange(false)}
            className="h-7 rounded-md px-2.5 text-mist hover:bg-obsidian transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
          >
            Cancelar
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() => void onDelete()}
            className="h-7 rounded-md bg-coral-red/12 px-2.5 text-coral-red hover:bg-coral-red/20 transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-coral-red/40 disabled:cursor-wait"
          >
            Eliminar
          </button>
        </div>
      )}

      {!collapsed && (
        <div id={`goal-${goal.id}-tasks`} className="min-w-0 px-2 pb-2.5 pt-1.5">
          {item.tasks.length > 0 && (
            <ul aria-label={`Tareas de ${goal.title}`} className="min-w-0 space-y-0.5">
              {item.tasks.map((task) => (
                <TaskRow
                  key={task.id}
                  task={task}
                  pending={pendingTaskIds.has(task.id)}
                  onToggle={onToggleTask}
                />
              ))}
            </ul>
          )}

          {allDone && (
            <div className="flex min-w-0 flex-wrap items-center gap-2 px-2.5 py-2 text-xs text-fog">
              <span className="min-w-0 break-words">Todos los pasos están listos.</span>
              <button
                type="button"
                disabled={busy}
                onClick={() => void onUpdate({ status: "completed" })}
                className="flex-shrink-0 rounded-md px-2 py-1 text-pulse-green hover:bg-obsidian transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
              >
                Completar meta
              </button>
            </div>
          )}

          {goal.status === "active" && (
            <div className="mt-0.5 flex h-9 min-w-0 items-center gap-2 rounded-md px-2.5 text-fog focus-within:bg-obsidian transition-colors duration-[120ms]">
              <Plus className="h-3.5 w-3.5 flex-shrink-0" />
              <input
                value={newTask}
                disabled={addingTask || busy}
                onChange={(event) => setNewTask(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    void submitTask();
                  } else if (event.key === "Escape") {
                    event.preventDefault();
                    setNewTask("");
                    event.currentTarget.blur();
                  }
                }}
                aria-label={`Nueva tarea para ${goal.title}`}
                placeholder="Agregar tarea…"
                className="h-8 min-w-0 flex-1 bg-transparent text-sm text-bone outline-none placeholder:text-ash disabled:cursor-wait"
              />
              {addingTask && <span className="flex-shrink-0 text-xs text-ash">Guardando…</span>}
            </div>
          )}
        </div>
      )}
    </article>
  );
}

export interface PlanTabProps {
  project: Project;
  refreshKey: string | null;
  onError: (error: unknown) => void;
  onChanged: () => Promise<void>;
}

export function PlanTab({ project, refreshKey, onError, onChanged }: PlanTabProps) {
  const [filter, setFilter] = useState<PlanFilter>("active");
  const [plan, setPlan] = useState<Plan | null>(null);
  const [loading, setLoading] = useState(true);
  const [goalTitle, setGoalTitle] = useState("");
  const [criteria, setCriteria] = useState("");
  const [showCriteria, setShowCriteria] = useState(false);
  const [creatingGoal, setCreatingGoal] = useState(false);
  const [collapsedGoalIds, setCollapsedGoalIds] = useState<Set<string>>(new Set());
  const [looseCollapsed, setLooseCollapsed] = useState(true);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
  const [busyGoalId, setBusyGoalId] = useState<string | null>(null);
  const [addingTaskGoalId, setAddingTaskGoalId] = useState<string | null>(null);
  const [pendingTaskIds, setPendingTaskIds] = useState<Set<string>>(new Set());
  const requestSeq = useRef(0);

  const loadPlan = useCallback(async () => {
    const seq = ++requestSeq.current;
    setLoading(true);
    try {
      const next = await api.getPlan(project.id, filter);
      if (seq === requestSeq.current) setPlan(next);
    } catch (error) {
      if (seq === requestSeq.current) onError(error);
    } finally {
      if (seq === requestSeq.current) setLoading(false);
    }
  }, [filter, onError, project.id]);

  useEffect(() => {
    void loadPlan();
  }, [loadPlan, refreshKey]);

  const changeFilter = (next: PlanFilter) => {
    if (next === filter) return;
    setPlan(null);
    setFilter(next);
  };

  const createGoal = async () => {
    const title = goalTitle.trim();
    if (!title || creatingGoal) return;
    setCreatingGoal(true);
    try {
      await api.createGoal(project.id, title, showCriteria ? criteria.trim() || null : null, []);
      setGoalTitle("");
      setCriteria("");
      setShowCriteria(false);
      await loadPlan();
      void onChanged();
    } catch (error) {
      onError(error);
    } finally {
      setCreatingGoal(false);
    }
  };

  const cancelGoal = () => {
    setGoalTitle("");
    setCriteria("");
    setShowCriteria(false);
  };

  const toggleCollapsed = (id: string) => {
    setCollapsedGoalIds((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const toggleTask = async (task: Task) => {
    if (!plan || pendingTaskIds.has(task.id)) return;
    const previous = findTask(plan, task.id);
    if (!previous) return;
    const status: Task["status"] = previous.status === "done" ? "todo" : "done";
    const optimistic: Task = {
      ...previous,
      status,
      completed_at: status === "done" ? new Date().toISOString() : null,
    };

    setPlan((current) => (current ? replaceTask(current, optimistic) : current));
    setPendingTaskIds((current) => new Set(current).add(task.id));
    try {
      const updated = await api.updateTask(task.id, { status });
      setPlan((current) => (current ? replaceTask(current, updated) : current));
      void onChanged();
    } catch (error) {
      setPlan((current) => (current ? replaceTask(current, previous) : current));
      onError(error);
    } finally {
      setPendingTaskIds((current) => {
        const next = new Set(current);
        next.delete(task.id);
        return next;
      });
    }
  };

  const addTask = async (goalId: string, title: string): Promise<boolean> => {
    setAddingTaskGoalId(goalId);
    try {
      const task = await api.createTask({ project_id: project.id, title, goal_id: goalId });
      setPlan((current) => {
        if (!current) return current;
        return {
          ...current,
          goals: current.goals.map((item) => {
            if (item.goal.id !== goalId || item.tasks.some((existing) => existing.id === task.id)) {
              return item;
            }
            const tasks = [...item.tasks, task];
            return { ...item, tasks, done: doneCount(tasks), total: tasks.length };
          }),
        };
      });
      void onChanged();
      return true;
    } catch (error) {
      onError(error);
      return false;
    } finally {
      setAddingTaskGoalId(null);
    }
  };

  const updateGoal = async (id: string, input: UpdateGoalInput) => {
    setBusyGoalId(id);
    try {
      await api.updateGoal(id, input);
      setConfirmDeleteId(null);
      await loadPlan();
      void onChanged();
    } catch (error) {
      onError(error);
    } finally {
      setBusyGoalId(null);
    }
  };

  const deleteGoal = async (id: string) => {
    setBusyGoalId(id);
    try {
      await api.deleteGoal(id);
      setConfirmDeleteId(null);
      await loadPlan();
      void onChanged();
    } catch (error) {
      onError(error);
    } finally {
      setBusyGoalId(null);
    }
  };

  return (
    <div className="flex h-full min-h-0 flex-1 flex-col gap-4 overflow-hidden px-6 pb-4">
      <div className="mx-auto flex w-full max-w-4xl flex-shrink-0 flex-col gap-3">
        <div className="min-w-0 rounded-xl bg-carbon p-1.5">
          <div className="flex h-9 min-w-0 items-center gap-2">
            <Target className="ml-2 h-4 w-4 flex-shrink-0 text-fog" />
            <input
              value={goalTitle}
              disabled={creatingGoal}
              onChange={(event) => setGoalTitle(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  void createGoal();
                } else if (event.key === "Escape") {
                  event.preventDefault();
                  cancelGoal();
                  event.currentTarget.blur();
                }
              }}
              aria-label="Nueva meta"
              placeholder="Nueva meta…"
              className="h-8 min-w-0 flex-1 bg-transparent text-sm text-paper outline-none placeholder:text-ash disabled:cursor-wait"
            />
            <button
              type="button"
              onClick={() => setShowCriteria((visible) => !visible)}
              aria-label="Añadir criterio de finalización"
              aria-expanded={showCriteria}
              title="Añadir criterio «Hecho cuando»"
              className={cn(
                "mr-1 flex h-7 flex-shrink-0 items-center gap-1.5 rounded-md px-2 text-xs transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
                showCriteria
                  ? "bg-elevated text-mist"
                  : "text-fog hover:bg-obsidian hover:text-mist",
              )}
            >
              {showCriteria ? (
                <ListChecks className="h-3.5 w-3.5" />
              ) : (
                <Plus className="h-3.5 w-3.5" />
              )}
              Hecho cuando
            </button>
          </div>
          {showCriteria && (
            <div className="flex h-9 min-w-0 items-center gap-2 px-2">
              <ListChecks className="h-4 w-4 flex-shrink-0 text-ash" />
              <input
                value={criteria}
                disabled={creatingGoal}
                onChange={(event) => setCriteria(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    void createGoal();
                  } else if (event.key === "Escape") {
                    event.preventDefault();
                    cancelGoal();
                    event.currentTarget.blur();
                  }
                }}
                aria-label="Criterio de finalización"
                placeholder="Resultado observable…"
                className="h-8 min-w-0 flex-1 bg-transparent text-sm text-mist outline-none placeholder:text-ash disabled:cursor-wait"
              />
            </div>
          )}
        </div>

        <div
          role="tablist"
          aria-label="Filtro de metas"
          className="flex items-center gap-1 self-start rounded-lg bg-obsidian p-1"
        >
          {FILTERS.map(({ id, label }) => (
            <button
              key={id}
              type="button"
              role="tab"
              aria-selected={filter === id}
              onClick={() => changeFilter(id)}
              className={cn(
                "flex h-7 items-center gap-1.5 rounded-md px-3 text-xs font-medium transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
                filter === id ? "bg-elevated text-paper" : "text-fog hover:text-mist",
              )}
            >
              <span>{label}</span>
              <span className="text-xs tabular-nums text-fog">{plan?.counts[id] ?? 0}</span>
            </button>
          ))}
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto pr-1">
        <div className="mx-auto w-full max-w-4xl min-w-0 space-y-2.5">
          {plan?.goals.map((item) => (
            <GoalSection
              key={item.goal.id}
              item={item}
              collapsed={collapsedGoalIds.has(item.goal.id)}
              busy={busyGoalId === item.goal.id}
              addingTask={addingTaskGoalId === item.goal.id}
              confirmingDelete={confirmDeleteId === item.goal.id}
              pendingTaskIds={pendingTaskIds}
              onToggleCollapsed={() => toggleCollapsed(item.goal.id)}
              onToggleTask={toggleTask}
              onUpdate={(input) => updateGoal(item.goal.id, input)}
              onDelete={() => deleteGoal(item.goal.id)}
              onConfirmDeleteChange={(confirming) =>
                setConfirmDeleteId(confirming ? item.goal.id : null)
              }
              onAddTask={(title) => addTask(item.goal.id, title)}
            />
          ))}

          {plan && plan.counts.all === 0 && (
            <div className="flex flex-col items-center gap-2 px-6 py-16 text-center">
              <div className="w-10 h-10 rounded-xl bg-carbon flex items-center justify-center text-ash mb-1">
                <Target className="h-5 w-5" />
              </div>
              <h2 className="text-sm font-semibold text-bone">Sin metas</h2>
              <p className="max-w-xs break-words text-xs leading-5 text-fog">
                Escribe una meta arriba y pulsa Enter.
              </p>
            </div>
          )}

          {plan && plan.goals.length === 0 && plan.counts.all > 0 && (
            <p className="px-3 py-10 text-center text-sm text-fog">No hay metas en este filtro.</p>
          )}

          {!plan && loading && (
            <div className="space-y-2.5">
              <div className="h-24 rounded-xl bg-carbon" />
              <div className="h-24 rounded-xl bg-carbon/70" />
            </div>
          )}

          {plan && plan.loose_tasks.length > 0 && (
            <section aria-label="Tareas sin meta" className="min-w-0 rounded-xl bg-carbon/60">
              <button
                type="button"
                onClick={() => setLooseCollapsed((collapsed) => !collapsed)}
                aria-expanded={!looseCollapsed}
                className="flex h-10 w-full min-w-0 items-center gap-2 rounded-xl px-3 text-left text-fog hover:bg-carbon hover:text-mist transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
              >
                {looseCollapsed ? (
                  <ChevronRight className="h-4 w-4 flex-shrink-0" />
                ) : (
                  <ChevronDown className="h-4 w-4 flex-shrink-0" />
                )}
                <span className="min-w-0 flex-1 truncate text-sm font-medium">Sin meta</span>
                <span className="flex-shrink-0 text-xs tabular-nums">
                  {plan.loose_tasks.length}
                </span>
              </button>
              {!looseCollapsed && (
                <ul aria-label="Lista de tareas sin meta" className="min-w-0 space-y-0.5 px-2 pb-2">
                  {plan.loose_tasks.map((task) => (
                    <TaskRow
                      key={task.id}
                      task={task}
                      pending={pendingTaskIds.has(task.id)}
                      onToggle={toggleTask}
                    />
                  ))}
                </ul>
              )}
            </section>
          )}
        </div>
      </div>
    </div>
  );
}
