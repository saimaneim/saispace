import {
  BookOpen,
  Brain,
  ChevronDown,
  CircleCheck,
  ExternalLink,
  FileCode,
  Flag,
  FlaskConical,
  GitCommitHorizontal,
  GitMerge,
  History,
  Layers,
  Palette,
  RefreshCw,
  Settings2,
  ShieldCheck,
  Sparkles,
  Tag,
  Target,
  Undo2,
  Wrench,
  Zap,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../lib/api";
import type { Project, TimelineDay, TimelineItem, TimelineKind } from "../lib/types";
import { cn } from "../lib/utils";

type Filter = "all" | "plan" | "code";

const FILTERS: { id: Filter; label: string; icon: typeof Layers }[] = [
  { id: "all", label: "Todo", icon: Layers },
  { id: "plan", label: "Plan", icon: Target },
  { id: "code", label: "Código", icon: GitMerge },
];

const PLAN_KINDS: TimelineKind[] = ["goal_completed", "task_completed", "memory"];
const VISIBLE_FILES = 12;

interface Look {
  icon: typeof Flag;
  tone: string;
  label: string;
}

const COMMIT_LOOKS: Record<string, Omit<Look, "label">> = {
  Nuevo: { icon: Sparkles, tone: "text-signal-teal" },
  Arreglo: { icon: Wrench, tone: "text-status-amber" },
  "Más rápido": { icon: Zap, tone: "text-status-amber" },
  "Mejora interna": { icon: RefreshCw, tone: "text-fog" },
  Documentación: { icon: BookOpen, tone: "text-fog" },
  Pruebas: { icon: FlaskConical, tone: "text-fog" },
  Estilo: { icon: Palette, tone: "text-fog" },
  Mantenimiento: { icon: Settings2, tone: "text-fog" },
  Revertido: { icon: Undo2, tone: "text-coral-red" },
  Seguridad: { icon: ShieldCheck, tone: "text-coral-red" },
};

function lookFor(item: TimelineItem): Look {
  switch (item.kind) {
    case "goal_completed":
      return { icon: Flag, tone: "text-accent", label: "Meta" };
    case "task_completed":
      return { icon: CircleCheck, tone: "text-pulse-green", label: "Tarea" };
    case "release":
      return { icon: Tag, tone: "text-paper", label: item.label ?? "Versión" };
    case "memory":
      return { icon: Brain, tone: "text-iris", label: item.label ?? "Decisión" };
    case "commit": {
      const label = item.label ?? "Cambio";
      return { ...(COMMIT_LOOKS[label] ?? { icon: GitCommitHorizontal, tone: "text-fog" }), label };
    }
  }
}

function localDate(isoDay: string): Date {
  const [year, month, day] = isoDay.split("-").map(Number);
  return new Date(year, month - 1, day);
}

function dayKey(date: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

function dayLabel(isoDay: string, today: Date): string {
  if (isoDay === dayKey(today)) return "Hoy";
  const yesterday = new Date(today.getFullYear(), today.getMonth(), today.getDate() - 1);
  if (isoDay === dayKey(yesterday)) return "Ayer";
  const date = localDate(isoDay);
  const text = new Intl.DateTimeFormat("es-ES", {
    weekday: "long",
    day: "numeric",
    month: "long",
    year: date.getFullYear() === today.getFullYear() ? undefined : "numeric",
  }).format(date);
  return text.charAt(0).toUpperCase() + text.slice(1);
}

const timeOf = (iso: string) =>
  new Intl.DateTimeFormat("es-ES", { hour: "2-digit", minute: "2-digit" }).format(new Date(iso));

function hasDetails(item: TimelineItem): boolean {
  if (item.kind === "goal_completed") return false;
  return Boolean(item.detail || item.hash || item.link || item.files.length > 0);
}

function ItemDetails({ item }: { item: TimelineItem }) {
  const hidden = item.file_count - Math.min(item.files.length, VISIBLE_FILES);
  return (
    <div className="mt-1 mb-1.5 ml-10 min-w-0 space-y-2.5 rounded-lg bg-carbon px-3 py-2.5">
      {item.detail && (
        <p className="max-h-48 max-w-[76ch] overflow-y-auto whitespace-pre-wrap break-words text-xs leading-5 text-mist select-text">
          {item.detail}
        </p>
      )}
      <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-xs text-fog">
        <span className="tabular-nums">{timeOf(item.at)}</span>
        {item.hash && <span className="font-mono text-mist select-text">{item.hash}</span>}
        {item.file_count > 0 && (
          <span className="flex items-center gap-1 tabular-nums">
            <FileCode className="h-3.5 w-3.5" />
            {item.file_count}
          </span>
        )}
        {item.link && (
          <button
            type="button"
            onClick={() => void api.openUrl(item.link as string)}
            title={item.link}
            className="ml-auto flex h-7 items-center gap-1.5 rounded-md px-2 text-fog hover:bg-obsidian hover:text-paper transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40"
          >
            <ExternalLink className="h-3.5 w-3.5" />
            {item.pr ? `PR #${item.pr}` : "Abrir"}
          </button>
        )}
      </div>
      {item.files.length > 0 && (
        <ul aria-label="Ficheros" className="min-w-0 space-y-0.5">
          {item.files.slice(0, VISIBLE_FILES).map((file) => (
            <li key={file} title={file} className="truncate font-mono text-xs text-fog">
              {file}
            </li>
          ))}
          {hidden > 0 && <li className="font-mono text-xs text-ash tabular-nums">+{hidden}</li>}
        </ul>
      )}
    </div>
  );
}

function GoalChip({ title }: { title: string }) {
  return (
    <span
      title={title}
      className="flex min-w-0 max-w-[40%] flex-shrink items-center gap-1 text-xs text-fog"
    >
      <Target className="h-3.5 w-3.5 flex-shrink-0" />
      <span className="truncate">{title}</span>
    </span>
  );
}

function MilestoneRow({ item }: { item: TimelineItem }) {
  return (
    <li aria-label={`Meta completada: ${item.title}`} className="relative flex min-w-0 gap-3 py-1">
      <span className="relative z-10 mt-1.5 flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-md bg-accent text-accent-ink">
        <Flag className="h-4 w-4" />
      </span>
      <div className="min-w-0 flex-1 rounded-lg bg-accent/10 px-3 py-2">
        <p title={item.title} className="truncate text-sm font-semibold text-paper">
          {item.title}
        </p>
        {item.detail && (
          <p className="mt-0.5 line-clamp-2 break-words text-xs leading-5 text-mist">
            {item.detail}
          </p>
        )}
      </div>
    </li>
  );
}

function ItemRow({ item }: { item: TimelineItem }) {
  const [open, setOpen] = useState(false);
  const { icon: Icon, tone, label } = lookFor(item);
  const expandable = hasDetails(item);

  const content = (
    <>
      <span
        className={cn(
          "relative z-10 flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-md bg-obsidian",
          tone,
        )}
      >
        <Icon className="h-4 w-4" />
      </span>
      <span
        className={cn(
          "flex-shrink-0 truncate text-xs font-medium md:w-24",
          tone === "text-paper" ? "text-fog" : tone,
        )}
      >
        {label}
      </span>
      <span
        title={item.title}
        className={cn(
          "min-w-0 flex-1 truncate text-sm",
          item.kind === "release" ? "font-mono font-semibold text-paper" : "text-bone",
        )}
      >
        {item.title}
      </span>
      {item.goal && <GoalChip title={item.goal} />}
      {item.pr !== null && (
        <span className="flex-shrink-0 font-mono text-xs text-ash tabular-nums">#{item.pr}</span>
      )}
      {expandable && (
        <ChevronDown
          aria-hidden="true"
          className={cn(
            "h-3.5 w-3.5 flex-shrink-0 text-ash transition-[transform,opacity] duration-[120ms]",
            open ? "rotate-180 opacity-100" : "opacity-0 group-hover:opacity-100",
          )}
        />
      )}
    </>
  );

  return (
    <li className="relative min-w-0">
      {expandable ? (
        <button
          type="button"
          aria-expanded={open}
          onClick={() => setOpen((value) => !value)}
          className="group flex h-10 w-full min-w-0 items-center gap-3 rounded-md pr-2 text-left hover:bg-carbon/60 transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40"
        >
          {content}
        </button>
      ) : (
        <div className="flex h-10 min-w-0 items-center gap-3 pr-2">{content}</div>
      )}
      {open && <ItemDetails item={item} />}
    </li>
  );
}

interface HistoryTabProps {
  project: Project;
  refreshKey: string | null;
  onError: (error: unknown) => void;
}

export function HistoryTab({ project, refreshKey, onError }: HistoryTabProps) {
  const [days, setDays] = useState<TimelineDay[] | null>(null);
  const [filter, setFilter] = useState<Filter>("all");
  const seq = useRef(0);

  const load = useCallback(async () => {
    const current = ++seq.current;
    try {
      const timeline = await api.projectTimeline(project.id);
      if (current === seq.current) setDays(timeline);
    } catch (error) {
      if (current === seq.current) setDays((previous) => previous ?? []);
      onError(error);
    }
  }, [project.id, onError]);

  useEffect(() => {
    void load();
  }, [load, refreshKey]);

  const visible = useMemo(() => {
    if (!days) return null;
    if (filter === "all") return days;
    const keep = (item: TimelineItem) => PLAN_KINDS.includes(item.kind) === (filter === "plan");
    return days
      .map((day) => ({ ...day, items: day.items.filter(keep) }))
      .filter((day) => day.items.length > 0);
  }, [days, filter]);

  const today = new Date();

  return (
    <div className="flex h-full min-h-0 flex-1 flex-col gap-2 overflow-hidden px-6 pt-2 pb-4">
      <div className="mx-auto flex w-full max-w-4xl flex-shrink-0 justify-end">
        <fieldset
          aria-label="Filtrar historial"
          className="flex items-center gap-0.5 rounded-lg bg-carbon p-1"
        >
          {FILTERS.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              type="button"
              title={label}
              aria-label={label}
              aria-pressed={filter === id}
              onClick={() => setFilter(id)}
              className={cn(
                "flex h-7 w-8 items-center justify-center rounded-md transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40",
                filter === id ? "bg-elevated text-paper" : "text-fog hover:text-mist",
              )}
            >
              <Icon className="h-4 w-4" />
            </button>
          ))}
        </fieldset>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto pr-1">
        <div className="mx-auto w-full max-w-4xl min-w-0 space-y-4">
          {!visible && (
            <div className="space-y-2">
              <div className="h-8 w-32 rounded-md bg-carbon" />
              <div className="h-10 rounded-md bg-carbon/70" />
              <div className="h-10 rounded-md bg-carbon/50" />
            </div>
          )}

          {visible?.length === 0 && (
            <div className="flex flex-col items-center gap-2 px-6 py-16 text-center">
              <div className="mb-1 flex h-10 w-10 items-center justify-center rounded-xl bg-carbon text-ash">
                <History className="h-5 w-5" />
              </div>
              <h2 className="text-sm font-semibold text-bone">Sin historial</h2>
            </div>
          )}

          {visible?.map((day) => {
            const label = dayLabel(day.date, today);
            return (
              <section key={day.date} aria-label={label} className="min-w-0">
                <h3 className="sticky top-0 z-20 flex h-8 items-center bg-void text-xs font-semibold text-fog">
                  <time dateTime={day.date}>{label}</time>
                </h3>
                <ol className="relative min-w-0 before:absolute before:top-3 before:bottom-3 before:left-[13px] before:w-0.5 before:rounded-full before:bg-obsidian">
                  {day.items.map((item, index) =>
                    item.kind === "goal_completed" ? (
                      <MilestoneRow key={`${item.kind}-${item.at}-${index}`} item={item} />
                    ) : (
                      <ItemRow key={`${item.kind}-${item.hash ?? item.at}-${index}`} item={item} />
                    ),
                  )}
                </ol>
              </section>
            );
          })}
        </div>
      </div>
    </div>
  );
}
