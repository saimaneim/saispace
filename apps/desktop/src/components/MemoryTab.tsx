import { Brain, Eye, Pencil, Pin, PinOff, Trash2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { api, formatError } from "../lib/api";
import type { DuplicatePair, Memory, MemoryKind, Project, StaleReport } from "../lib/types";
import { cn, formatTimeAgo } from "../lib/utils";
import { AgentContextDialog } from "./AgentContextDialog";
import { DuplicatesPanel } from "./DuplicatesPanel";
import { Markdown } from "./Markdown";
import { MemoryEditor } from "./MemoryEditor";
import { OrphanMemories } from "./OrphanMemories";
import { StalePanel } from "./StalePanel";
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
  const [duplicates, setDuplicates] = useState<DuplicatePair[]>([]);
  const [dismissedPairKeys, setDismissedPairKeys] = useState<Set<string>>(new Set());
  const [showDuplicates, setShowDuplicates] = useState(false);
  const [staleReport, setStaleReport] = useState<StaleReport>({ stale: [], surplus_sessions: [] });
  const [showStale, setShowStale] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(focusId ?? null);
  const [filter, setFilter] = useState("");
  const [scopeFilter, setScopeFilter] = useState<"all" | "global" | "orphans">("all");
  const [error, setError] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<Memory | null>(null);
  const [editingMemory, setEditingMemory] = useState<Memory | null>(null);
  const [agentDialogOpen, setAgentDialogOpen] = useState(false);

  const load = useCallback(async () => {
    try {
      const [mems, dupes, stale] = await Promise.all([
        api.listMemories(project?.id ?? null, 200),
        api.memoryDuplicates(project?.id ?? null),
        api.memoryStale(project?.id ?? null),
      ]);
      setMemories(mems);
      setDuplicates(dupes);
      setStaleReport(stale);
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

  // Review panels belong to one project: close them when switching.
  useEffect(() => {
    setShowStale(false);
    setShowDuplicates(false);
  }, [project?.id]);

  const visible = useMemo(() => {
    let list = memories;
    if (!project && scopeFilter === "global") {
      list = list.filter(
        (m) => (!m.project_id || m.project_id === "") && (!m.project_name || m.project_name === ""),
      );
    }
    const q = filter.trim().toLowerCase();
    if (q) {
      list = list.filter(
        (m) =>
          m.title.toLowerCase().includes(q) ||
          m.content.toLowerCase().includes(q) ||
          Boolean(m.project_name?.toLowerCase().includes(q)),
      );
    }
    return [...list].sort((a, b) => {
      if (a.pinned !== b.pinned) {
        return a.pinned ? -1 : 1;
      }
      return 0;
    });
  }, [memories, project, scopeFilter, filter]);

  const activeDuplicates = useMemo(() => {
    return duplicates.filter((pair) => !dismissedPairKeys.has(`${pair.a.id}:${pair.b.id}`));
  }, [duplicates, dismissedPairKeys]);

  const staleCount = staleReport.stale.length + staleReport.surplus_sessions.length;

  // Stale memories can be older than the 200 loaded ones: the report carries them whole.
  const selected =
    memories.find((m) => m.id === selectedId) ??
    staleReport.stale.find((m) => m.id === selectedId) ??
    staleReport.surplus_sessions.find((m) => m.id === selectedId) ??
    visible[0] ??
    null;

  const togglePin = async (memory: Memory) => {
    try {
      const nextPinned = !memory.pinned;
      const updated = await api.setMemoryPinned(memory.id, nextPinned);
      setMemories((prev) => prev.map((m) => (m.id === updated.id ? updated : m)));
    } catch (err) {
      setError(formatError(err));
    }
  };

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
        <Button
          variant="secondary"
          size="sm"
          onClick={() => setAgentDialogOpen(true)}
          className="mt-2 flex items-center gap-2 text-xs"
        >
          <Eye className="w-4 h-4" />
          Vista del agente
        </Button>
        {error && <p className="text-sm text-coral-red">{error}</p>}
        <AgentContextDialog
          open={agentDialogOpen}
          onOpenChange={setAgentDialogOpen}
          projectId={project?.id ?? null}
        />
      </div>
    );
  }

  return (
    <div className="flex-1 flex min-h-0">
      <aside className="w-80 flex-shrink-0 bg-carbon flex flex-col min-h-0 p-3 gap-2">
        <Button
          variant="secondary"
          size="sm"
          onClick={() => setAgentDialogOpen(true)}
          className="w-full flex items-center justify-center gap-2 text-xs"
        >
          <Eye className="w-4 h-4" />
          Vista del agente
        </Button>

        {scopeFilter !== "orphans" && (
          <input
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder="Filtrar memorias…"
            aria-label="Filtrar memorias"
            className="h-9 px-3 text-sm bg-obsidian text-paper placeholder:text-fog rounded-md focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
          />
        )}

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
            <button
              type="button"
              onClick={() => setScopeFilter("orphans")}
              className={cn(
                "flex-1 h-7 text-xs rounded transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
                scopeFilter === "orphans"
                  ? "bg-elevated text-paper font-medium"
                  : "text-fog hover:text-mist",
              )}
            >
              Sin proyecto
            </button>
          </div>
        )}

        {scopeFilter !== "orphans" && activeDuplicates.length > 0 && (
          <button
            type="button"
            onClick={() => {
              setShowDuplicates(true);
              setShowStale(false);
            }}
            className="flex items-center justify-between px-3 py-2 text-xs font-medium rounded-md bg-status-amber/10 text-status-amber hover:bg-status-amber/20 transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
          >
            <span>
              {activeDuplicates.length === 1
                ? "1 posible duplicado"
                : `${activeDuplicates.length} posibles duplicados`}
            </span>
          </button>
        )}

        {scopeFilter !== "orphans" && staleCount > 0 && (
          <button
            type="button"
            onClick={() => {
              setShowStale(true);
              setShowDuplicates(false);
            }}
            className="flex items-center justify-between px-3 py-2 text-xs font-medium rounded-md bg-status-amber/10 text-status-amber hover:bg-status-amber/20 transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
          >
            <span>{staleCount === 1 ? "1 para revisar" : `${staleCount} para revisar`}</span>
          </button>
        )}

        {scopeFilter !== "orphans" && (
          <ul className="flex-1 overflow-y-auto space-y-0.5">
            {visible.map((m) => (
              <li key={m.id}>
                <button
                  type="button"
                  onClick={() => {
                    setSelectedId(m.id);
                    setShowDuplicates(false);
                    setShowStale(false);
                  }}
                  className={cn(
                    "w-full text-left px-3 py-2 rounded-md transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
                    selected?.id === m.id ? "bg-elevated" : "hover:bg-obsidian",
                  )}
                >
                  <div className="flex items-center gap-1.5 min-w-0">
                    {m.pinned && <Pin className="w-3.5 h-3.5 text-acid-lime flex-shrink-0" />}
                    <span className="block text-sm text-paper truncate font-medium">{m.title}</span>
                  </div>
                  <span className="block text-xs text-fog">
                    {KIND_LABELS[m.kind]}
                    {!project && ` · ${m.project_name || "Global"}`} · {formatTimeAgo(m.updated_at)}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </aside>

      {scopeFilter === "orphans" ? (
        <OrphanMemories onLinked={load} />
      ) : showStale ? (
        <StalePanel
          report={staleReport}
          onOpenMemory={(id) => {
            setSelectedId(id);
            setShowStale(false);
          }}
          onDeleteMemory={async (id) => {
            await api.deleteMemory(id);
            await load();
          }}
          onClose={() => setShowStale(false)}
        />
      ) : showDuplicates ? (
        <DuplicatesPanel
          pairs={activeDuplicates}
          onDeleteMemory={async (id) => {
            await api.deleteMemory(id);
            await load();
          }}
          onDismissPair={(pair) => {
            setDismissedPairKeys((prev) => {
              const next = new Set(prev);
              next.add(`${pair.a.id}:${pair.b.id}`);
              return next;
            });
          }}
          onClose={() => setShowDuplicates(false)}
        />
      ) : selected ? (
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
            <Button variant="secondary" onClick={() => setEditingMemory(selected)}>
              <Pencil className="w-4 h-4 mr-2" />
              Editar
            </Button>
            <Button variant="secondary" onClick={() => togglePin(selected)}>
              {selected.pinned ? (
                <>
                  <PinOff className="w-4 h-4 mr-2" />
                  Desfijar
                </>
              ) : (
                <>
                  <Pin className="w-4 h-4 mr-2" />
                  Fijar
                </>
              )}
            </Button>
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
      ) : null}

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

      <MemoryEditor
        memory={editingMemory}
        open={Boolean(editingMemory)}
        onOpenChange={(open) => !open && setEditingMemory(null)}
        onSaved={async () => {
          await load();
        }}
      />
      <AgentContextDialog
        open={agentDialogOpen}
        onOpenChange={setAgentDialogOpen}
        projectId={project?.id ?? null}
      />
    </div>
  );
}
