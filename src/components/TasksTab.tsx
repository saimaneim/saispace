import { AlertCircle, CheckCircle2, Circle, Clock, ListTodo, Trash2, X } from "lucide-react";
import { useState } from "react";
import type { Task, TaskPriority, TaskStatus } from "../lib/types";
import { cn, formatTimeAgo } from "../lib/utils";

const PRIORITY_LABELS: Record<TaskPriority, string> = {
  low: "Baja",
  medium: "Media",
  high: "Alta",
  urgent: "Urgente",
};

export interface TasksTabProps {
  tasks: Task[];
  onUpdateTask: (id: string, status?: TaskStatus, priority?: TaskPriority) => Promise<void>;
  onDeleteTask: (id: string) => Promise<void>;
}

export function TasksTab({ tasks, onUpdateTask, onDeleteTask }: TasksTabProps) {
  const [filter, setFilter] = useState<"open" | "done" | "all">("open");

  const cycleStatus = (current: TaskStatus): TaskStatus => {
    switch (current) {
      case "todo":
        return "in_progress";
      case "in_progress":
        return "done";
      case "done":
        return "todo";
      case "blocked":
        return "todo";
      case "cancelled":
        return "todo";
    }
  };

  const cyclePriority = (current: TaskPriority): TaskPriority => {
    switch (current) {
      case "low":
        return "medium";
      case "medium":
        return "high";
      case "high":
        return "urgent";
      case "urgent":
        return "low";
    }
  };

  const filteredTasks = tasks.filter((t) => {
    if (filter === "open") return t.status !== "done" && t.status !== "cancelled";
    if (filter === "done") return t.status === "done";
    return true;
  });

  const getPriorityTextColor = (priority: TaskPriority) => {
    switch (priority) {
      case "urgent":
        return "text-acid-lime";
      case "high":
        return "text-status-amber";
      case "medium":
        return "text-mist";
      case "low":
        return "text-ash";
    }
  };

  const openCount = tasks.filter((t) => t.status !== "done" && t.status !== "cancelled").length;
  const doneCount = tasks.filter((t) => t.status === "done").length;

  return (
    <div className="flex-1 flex flex-col h-full overflow-hidden px-6 pb-4 gap-3">
      <div className="flex items-center justify-between">
        <div
          role="tablist"
          aria-label="Filtro de tareas"
          className="flex items-center gap-1 bg-obsidian p-1 rounded-lg select-none"
        >
          <button
            type="button"
            role="tab"
            aria-selected={filter === "open"}
            aria-label="Ver tareas abiertas"
            onClick={() => setFilter("open")}
            className={cn(
              "h-8 px-3 rounded-md text-sm font-medium transition-colors duration-[120ms] flex items-center gap-1.5 focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
              filter === "open" ? "bg-elevated text-paper" : "text-fog hover:text-mist",
            )}
          >
            <span>Abiertas</span>
            <span className="tabular-nums font-sans text-xs text-fog">{openCount}</span>
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={filter === "done"}
            aria-label="Ver tareas hechas"
            onClick={() => setFilter("done")}
            className={cn(
              "h-8 px-3 rounded-md text-sm font-medium transition-colors duration-[120ms] flex items-center gap-1.5 focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
              filter === "done" ? "bg-elevated text-paper" : "text-fog hover:text-mist",
            )}
          >
            <span>Hechas</span>
            <span className="tabular-nums font-sans text-xs text-fog">{doneCount}</span>
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={filter === "all"}
            aria-label="Ver todas las tareas"
            onClick={() => setFilter("all")}
            className={cn(
              "h-8 px-3 rounded-md text-sm font-medium transition-colors duration-[120ms] flex items-center gap-1.5 focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
              filter === "all" ? "bg-elevated text-paper" : "text-fog hover:text-mist",
            )}
          >
            <span>Todas</span>
            <span className="tabular-nums font-sans text-xs text-fog">{tasks.length}</span>
          </button>
        </div>
      </div>

      <ul
        aria-label="Lista de tareas del proyecto"
        className="flex-1 overflow-y-auto space-y-1 pr-1"
      >
        {filteredTasks.map((t) => {
          const isDone = t.status === "done";
          const isInProgress = t.status === "in_progress";
          const isBlocked = t.status === "blocked";
          const isCancelled = t.status === "cancelled";

          return (
            <li
              key={t.id}
              className="group flex items-center justify-between px-3 h-10 rounded-md bg-obsidian/40 hover:bg-obsidian/80 transition-colors duration-[120ms]"
            >
              <div className="flex items-center gap-3 min-w-0 flex-1">
                <button
                  type="button"
                  onClick={() => onUpdateTask(t.id, cycleStatus(t.status))}
                  aria-label={`Cambiar estado de ${t.title}`}
                  className="flex-shrink-0 text-fog hover:text-paper transition-colors duration-[120ms] p-1 rounded focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
                  title="Cambiar estado"
                >
                  {isDone ? (
                    <CheckCircle2 className="w-4 h-4 text-pulse-green" />
                  ) : isInProgress ? (
                    <Clock className="w-4 h-4 text-status-amber" />
                  ) : isBlocked ? (
                    <AlertCircle className="w-4 h-4 text-coral-red" />
                  ) : isCancelled ? (
                    <X className="w-4 h-4 text-ash" />
                  ) : (
                    <Circle className="w-4 h-4 text-ash group-hover:text-fog transition-colors duration-[120ms]" />
                  )}
                </button>

                <span
                  className={cn(
                    "text-sm truncate tracking-tight font-sans",
                    isDone || isCancelled ? "text-ash line-through" : "text-paper",
                  )}
                >
                  {t.title}
                </span>
              </div>

              <div className="flex items-center gap-2 flex-shrink-0 ml-3">
                <div className="flex items-center gap-2 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity duration-[120ms]">
                  <button
                    type="button"
                    onClick={() => onUpdateTask(t.id, undefined, cyclePriority(t.priority))}
                    aria-label={`Cambiar prioridad de ${t.title}`}
                    className={cn(
                      "text-xs font-sans px-2 py-1 rounded hover:bg-elevated transition-colors duration-[120ms] cursor-pointer focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
                      getPriorityTextColor(t.priority),
                    )}
                    title="Alternar prioridad"
                  >
                    {PRIORITY_LABELS[t.priority]}
                  </button>

                  <button
                    type="button"
                    onClick={() => onDeleteTask(t.id)}
                    aria-label={`Eliminar tarea ${t.title}`}
                    className="p-1 rounded text-ash hover:text-coral-red hover:bg-coral-red/10 transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
                    title="Eliminar tarea"
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                </div>

                <span className="font-sans text-xs text-ash tabular-nums min-w-14 text-right">
                  {formatTimeAgo(t.created_at)}
                </span>
              </div>
            </li>
          );
        })}

        {filteredTasks.length === 0 && (
          <div className="py-16 text-center text-sm text-fog space-y-2">
            <ListTodo className="w-8 h-8 mx-auto text-ash mb-2" />
            <p>Las tareas las crea tu IA con create_task</p>
          </div>
        )}
      </ul>
    </div>
  );
}
