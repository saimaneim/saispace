import { Brain, Trash2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { api, formatError } from "../lib/api";
import type { Memory, MemoryKind, Project } from "../lib/types";
import { cn, formatTimeAgo } from "../lib/utils";
import { Markdown } from "./Markdown";
import { Button } from "./ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./ui/dialog";

export interface MemoryTabProps {
  project: Project | null;
  refreshKey?: string | null;
  focusId?: string | null;
}

export const KIND_LABELS: Record<MemoryKind, string> = {
  decision: "Decisión",
  architecture: "Arquitectura",
  bugfix: "Corrección",
  pattern: "Patrón",
  config: "Configuración",
  discovery: "Hallazgo",
  preference: "Preferencia",
  session_summary: "Resumen de sesión",
  note: "Nota",
};

export function MemoryTab({ project, refreshKey, focusId }: MemoryTabProps) {
  const [memories, setMemories] = useState<Memory[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(focusId ?? null);
  const [filter, setFilter] = useState("");
  const [scopeFilter, setScopeFilter] = useState<"all" | "global">("all");
  const [error, setError] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<Memory | null>(null);

  const load = useCallback(async () => {
    try {
      setMemories(await api.listMemories(project?.id ?? null, 200));
    } catch (err) {
      setError(formatError(err));
    }
  }, [project?.id]);

  useEffect(() => {
    if (refreshKey !== undefined) load();
  }, [load, refreshKey]);

  useEffect(() => {
    if (focusId) setSelectedId(focusId);
  }, [focusId]);

  const visible = useMemo(() => {
    let list = memories;
    if (!project && scopeFilter === "global") {
      list = list.filter(
        (m) => (!m.project_id || m.project_id === "") && (!m.project_name || m.project_name === ""),
      );
    }
    const q = filter.trim().toLowerCase();
    if (!q) return list;
    return list.filter(
      (m) =>
        m.title.toLowerCase().includes(q) ||
        m.content.toLowerCase().includes(q) ||
        Boolean(m.project_name?.toLowerCase().includes(q)),
    );
  }, [memories, project, scopeFilter, filter]);

  const selected = memories.find((m) => m.id === selectedId) ?? visible[0] ?? null;

  const confirmDelete = async () => {
    if (!pendingDelete) return;
    try {
      await api.deleteMemory(pendingDelete.id);
      setPendingDelete(null);
      await load();
    } catch (err) {
      setError(formatError(err));
    }
  };

  if (memories.length === 0) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center gap-2 text-center px-6">
        <Brain className="w-8 h-8 text-ash" />
        <p className="text-sm text-paper">Sin memorias todavía</p>
        <p className="text-sm text-fog max-w-sm">
          Los agentes guardan aquí decisiones y hallazgos con la tool save_memory o con sai memory
          save.
        </p>
        {error && <p className="text-sm text-coral-red">{error}</p>}
      </div>
    );
  }

  return (
    <div className="flex-1 flex min-h-0">
      <aside className="w-80 flex-shrink-0 bg-carbon flex flex-col min-h-0 p-3 gap-2">
        <input
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="Filtrar memorias…"
          aria-label="Filtrar memorias"
          className="h-9 px-3 text-sm bg-obsidian text-paper placeholder:text-fog rounded-md focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
        />

        {!project && (
          <div className="flex items-center gap-1 bg-obsidian p-1 rounded-md flex-shrink-0 select-none">
            <button
              type="button"
              onClick={() => setScopeFilter("all")}
              className={cn(
                "flex-1 h-7 text-xs rounded transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
                scopeFilter === "all"
                  ? "bg-elevated text-paper font-medium"
                  : "text-fog hover:text-mist",
              )}
            >
              Todas
            </button>
            <button
              type="button"
              onClick={() => setScopeFilter("global")}
              className={cn(
                "flex-1 h-7 text-xs rounded transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
                scopeFilter === "global"
                  ? "bg-elevated text-paper font-medium"
                  : "text-fog hover:text-mist",
              )}
            >
              Globales
            </button>
          </div>
        )}

        <ul className="flex-1 overflow-y-auto space-y-0.5">
          {visible.map((m) => (
            <li key={m.id}>
              <button
                type="button"
                onClick={() => setSelectedId(m.id)}
                className={cn(
                  "w-full text-left px-3 py-2 rounded-md transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
                  selected?.id === m.id ? "bg-elevated" : "hover:bg-obsidian",
                )}
              >
                <span className="block text-sm text-paper truncate font-medium">{m.title}</span>
                <span className="block text-xs text-fog">
                  {KIND_LABELS[m.kind]}
                  {!project && ` · ${m.project_name || "Global"}`} · {formatTimeAgo(m.updated_at)}
                </span>
              </button>
            </li>
          ))}
        </ul>
      </aside>

      {selected && (
        <section className="flex-1 min-w-0 flex flex-col px-6 pb-6">
          <div className="h-14 flex items-center gap-3 flex-shrink-0">
            <div className="flex-1 min-w-0">
              <h2 className="text-base font-semibold text-paper truncate">{selected.title}</h2>
              <p className="text-xs text-fog truncate">
                {KIND_LABELS[selected.kind]}
                {selected.topic_key ? ` · ${selected.topic_key}` : ""}
                {!project && selected.project_name ? ` · ${selected.project_name}` : ""}
                {` · ${formatTimeAgo(selected.updated_at)}`}
              </p>
            </div>
            <Button variant="secondary" onClick={() => setPendingDelete(selected)}>
              <Trash2 className="w-4 h-4 mr-2" />
              Eliminar
            </Button>
          </div>
          {error && (
            <div
              className="mb-2 px-3 py-2 rounded-md bg-coral-red/10 text-coral-red text-sm"
              role="alert"
            >
              {error}
            </div>
          )}
          <div className="flex-1 overflow-y-auto bg-carbon rounded-lg p-6">
            <Markdown content={selected.content} className="max-w-[70ch]" />
          </div>
        </section>
      )}

      <Dialog
        open={Boolean(pendingDelete)}
        onOpenChange={(open) => !open && setPendingDelete(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Eliminar memoria</DialogTitle>
            <DialogDescription>
              Los agentes dejarán de recibir «{pendingDelete?.title}». No se puede deshacer.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="secondary" onClick={() => setPendingDelete(null)}>
              Cancelar
            </Button>
            <Button variant="destructive" onClick={confirmDelete}>
              Eliminar
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
