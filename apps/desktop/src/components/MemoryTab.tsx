import {
  Brain,
  ChevronRight,
  Clock,
  Copy,
  Eye,
  Folder,
  History,
  Pencil,
  Pin,
  PinOff,
  Search,
  Trash2,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, formatError } from "../lib/api";
import type { DuplicatePair, Memory, Project, StaleReport } from "../lib/types";
import { cn, formatTimeAgo } from "../lib/utils";
import { AgentContextDialog } from "./AgentContextDialog";
import { DuplicatesPanel } from "./DuplicatesPanel";
import { Markdown } from "./Markdown";
import { MemoryEditor } from "./MemoryEditor";
import { MEMORY_KIND_LABELS, MEMORY_KIND_STYLE, MemoryKindBadge } from "./MemoryKindBadge";
import { MemoryLinks } from "./MemoryLinks";
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
  focusSeq?: number;
}

export const KIND_LABELS = MEMORY_KIND_LABELS;

function formatFullDate(isoString: string): string {
  try {
    return new Intl.DateTimeFormat("es", {
      dateStyle: "medium",
      timeStyle: "short",
    }).format(new Date(isoString));
  } catch {
    return isoString;
  }
}

export function MemoryTab({ project, refreshKey, focusId, focusSeq }: MemoryTabProps) {
  const [memories, setMemories] = useState<Memory[]>([]);
  const [loading, setLoading] = useState(refreshKey !== undefined);
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
  const memoriesRef = useRef(memories);
  memoriesRef.current = memories;

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
    } finally {
      setLoading(false);
    }
  }, [project?.id]);

  useEffect(() => {
    if (refreshKey !== undefined) load();
  }, [load, refreshKey]);

  useEffect(() => {
    if (!focusId) return;
    void focusSeq;
    setSelectedId(focusId);
    setShowDuplicates(false);
    setShowStale(false);
    setScopeFilter("all");
    setFilter("");

    if (memoriesRef.current.some((m) => m.id === focusId)) return;
    let cancelled = false;
    api
      .getMemory(focusId)
      .then((memory) => {
        if (!cancelled && memory) {
          setMemories((prev) => (prev.some((m) => m.id === memory.id) ? prev : [memory, ...prev]));
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [focusId, focusSeq]);

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
          Boolean(m.topic_key?.toLowerCase().includes(q)) ||
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
  const isFiltering = filter.trim().length > 0;

  const selected = isFiltering
    ? (visible.find((m) => m.id === selectedId) ?? visible[0] ?? null)
    : (memories.find((m) => m.id === selectedId) ??
      staleReport.stale.find((m) => m.id === selectedId) ??
      staleReport.surplus_sessions.find((m) => m.id === selectedId) ??
      visible[0] ??
      null);

  const openMemory = async (id: string) => {
    if (!memories.some((m) => m.id === id)) {
      try {
        const memory = await api.getMemory(id);
        setMemories((prev) => (prev.some((m) => m.id === id) ? prev : [memory, ...prev]));
      } catch (err) {
        setError(formatError(err));
        return;
      }
    }
    setShowStale(false);
    setShowDuplicates(false);
    setScopeFilter("all");
    setSelectedId(id);
  };

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

  if (loading && memories.length === 0) {
    return (
      <div className="flex min-h-0 flex-1">
        <aside className="bg-carbon flex min-h-0 w-80 flex-shrink-0 flex-col gap-2 p-3">
          <div className="flex h-7 items-center justify-between px-1">
            <div className="bg-obsidian h-3 w-16 rounded" />
            <div className="bg-obsidian h-7 w-7 rounded-md" />
          </div>
          <div className="bg-obsidian h-9 rounded-md" />
          <div className="space-y-1.5 pt-1">
            <div className="bg-obsidian/70 h-14 rounded-md" />
            <div className="bg-obsidian/50 h-14 rounded-md" />
            <div className="bg-obsidian/40 h-14 rounded-md" />
          </div>
        </aside>
        <section className="min-w-0 flex-1 space-y-4 px-7 pt-5 pb-7">
          <div className="bg-carbon h-7 w-72 rounded-md" />
          <div className="bg-carbon h-5 w-44 rounded-md" />
          <div className="bg-carbon h-44 max-w-[76ch] rounded-xl" />
        </section>
      </div>
    );
  }

  if (memories.length === 0 && scopeFilter !== "orphans") {
    return (
      <div className="flex min-h-0 flex-1 flex-col">
        <div className="flex flex-1 flex-col items-center justify-center gap-2.5 px-6 text-center">
          <div className="bg-carbon text-ash flex h-10 w-10 items-center justify-center rounded-xl">
            <Brain className="h-5 w-5" />
          </div>
          <p className="text-paper text-sm font-medium">Sin memorias</p>
          <p className="text-fog max-w-xs text-xs leading-relaxed">
            <code className="text-mist font-mono">save_memory</code> /{" "}
            <code className="text-mist font-mono">sai memory save</code>
          </p>
          <Button
            variant="secondary"
            size="icon"
            onClick={() => setAgentDialogOpen(true)}
            title="Vista del agente"
            aria-label="Vista del agente"
            className="mt-1"
          >
            <Eye className="h-4 w-4" />
          </Button>
          {error && (
            <p className="text-coral-red text-xs" role="alert">
              {error}
            </p>
          )}
          <AgentContextDialog
            open={agentDialogOpen}
            onOpenChange={setAgentDialogOpen}
            projectId={project?.id ?? null}
          />
        </div>
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex min-h-0 flex-1">
        <aside className="bg-carbon flex min-h-0 w-80 flex-shrink-0 flex-col">
          <div className="flex-shrink-0 space-y-2 px-3 pt-3 pb-2">
            <div className="flex h-7 items-center justify-between gap-2 px-1">
              <span className="text-fog text-xs font-medium tabular-nums">
                {visible.length} {visible.length === 1 ? "memoria" : "memorias"}
              </span>
              <button
                type="button"
                onClick={() => setAgentDialogOpen(true)}
                title="Vista del agente"
                aria-label="Vista del agente"
                className="text-fog hover:text-paper hover:bg-obsidian focus-visible:ring-accent/40 flex h-7 w-7 items-center justify-center rounded-md transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2"
              >
                <Eye className="h-4 w-4" />
              </button>
            </div>

            {scopeFilter !== "orphans" && (
              <div className="relative">
                <Search className="text-fog pointer-events-none absolute top-1/2 left-2.5 h-3.5 w-3.5 -translate-y-1/2" />
                <input
                  value={filter}
                  onChange={(e) => setFilter(e.target.value)}
                  placeholder="Filtrar…"
                  aria-label="Filtrar memorias"
                  className="bg-obsidian text-paper placeholder:text-fog focus-visible:ring-accent/40 h-9 w-full rounded-md pr-8 pl-8 text-sm transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2"
                />
                {filter && (
                  <button
                    type="button"
                    onClick={() => setFilter("")}
                    aria-label="Limpiar filtro"
                    title="Limpiar filtro"
                    className="text-fog hover:text-paper hover:bg-elevated focus-visible:ring-accent/40 absolute top-1/2 right-1.5 flex h-6 w-6 -translate-y-1/2 items-center justify-center rounded transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2"
                  >
                    <X className="h-3.5 w-3.5" />
                  </button>
                )}
              </div>
            )}

            {!project && (
              <div className="bg-obsidian flex flex-shrink-0 items-center gap-1 rounded-lg p-1 select-none">
                <button
                  type="button"
                  onClick={() => setScopeFilter("all")}
                  className={cn(
                    "focus-visible:ring-accent/40 h-7 flex-1 rounded-md text-xs transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2",
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
                    "focus-visible:ring-accent/40 h-7 flex-1 rounded-md text-xs transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2",
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
                    "focus-visible:ring-accent/40 h-7 flex-1 rounded-md text-xs transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2",
                    scopeFilter === "orphans"
                      ? "bg-elevated text-paper font-medium"
                      : "text-fog hover:text-mist",
                  )}
                >
                  Sin proyecto
                </button>
              </div>
            )}

            {scopeFilter !== "orphans" && (activeDuplicates.length > 0 || staleCount > 0) && (
              <div className="flex items-center gap-1.5">
                {activeDuplicates.length > 0 && (
                  <button
                    type="button"
                    onClick={() => {
                      setShowDuplicates(true);
                      setShowStale(false);
                    }}
                    title="Posibles duplicados"
                    className={cn(
                      "focus-visible:ring-accent/40 flex h-8 min-w-0 flex-1 items-center gap-1.5 rounded-md px-2.5 text-xs font-medium transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2",
                      showDuplicates
                        ? "bg-status-amber/20 text-status-amber"
                        : "bg-status-amber/10 text-status-amber hover:bg-status-amber/15",
                    )}
                  >
                    <Copy className="h-3.5 w-3.5 flex-shrink-0" />
                    <span className="flex-1 truncate text-left tabular-nums">
                      {activeDuplicates.length} dup.
                    </span>
                    <ChevronRight className="h-3.5 w-3.5 flex-shrink-0 opacity-75" />
                  </button>
                )}

                {staleCount > 0 && (
                  <button
                    type="button"
                    onClick={() => {
                      setShowStale(true);
                      setShowDuplicates(false);
                    }}
                    title="Memorias para revisar"
                    className={cn(
                      "focus-visible:ring-accent/40 flex h-8 min-w-0 flex-1 items-center gap-1.5 rounded-md px-2.5 text-xs font-medium transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2",
                      showStale
                        ? "bg-status-amber/20 text-status-amber"
                        : "bg-status-amber/10 text-status-amber hover:bg-status-amber/15",
                    )}
                  >
                    <History className="h-3.5 w-3.5 flex-shrink-0" />
                    <span className="flex-1 truncate text-left tabular-nums">
                      {staleCount} rev.
                    </span>
                    <ChevronRight className="h-3.5 w-3.5 flex-shrink-0 opacity-75" />
                  </button>
                )}
              </div>
            )}
          </div>

          {scopeFilter !== "orphans" && (
            <ul className="min-h-0 flex-1 space-y-0.5 overflow-y-auto px-2 pb-3">
              {visible.map((m) => {
                const isSelected = !showDuplicates && !showStale && selected?.id === m.id;
                return (
                  <li key={m.id}>
                    <button
                      type="button"
                      onClick={() => {
                        setSelectedId(m.id);
                        setShowDuplicates(false);
                        setShowStale(false);
                      }}
                      aria-current={isSelected ? "true" : undefined}
                      className={cn(
                        "group focus-visible:ring-accent/40 relative w-full rounded-md py-2.5 pr-2.5 pl-3 text-left transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2",
                        isSelected ? "bg-elevated" : "hover:bg-obsidian",
                      )}
                    >
                      <div className="flex min-w-0 items-center gap-1.5">
                        {m.pinned && <Pin className="text-accent h-3.5 w-3.5 flex-shrink-0" />}
                        <span
                          className={cn(
                            "block min-w-0 flex-1 truncate text-sm font-medium",
                            isSelected ? "text-paper" : "text-bone group-hover:text-paper",
                          )}
                        >
                          {m.title}
                        </span>
                      </div>
                      <div className="mt-1 flex min-w-0 items-center justify-between gap-2">
                        <div className="flex min-w-0 flex-1 items-center gap-1.5">
                          <span
                            className={cn(
                              "h-1.5 w-1.5 flex-shrink-0 rounded-full",
                              (MEMORY_KIND_STYLE[m.kind] ?? MEMORY_KIND_STYLE.note).dot,
                            )}
                            aria-hidden="true"
                          />
                          <span className="text-fog truncate text-xs">
                            {KIND_LABELS[m.kind]}
                            {!project ? ` · ${m.project_name || "Global"}` : ""}
                          </span>
                        </div>
                        <span className="text-ash flex-shrink-0 text-[11px] tabular-nums">
                          {formatTimeAgo(m.updated_at)}
                        </span>
                      </div>
                    </button>
                  </li>
                );
              })}

              {visible.length === 0 && (
                <li className="text-fog space-y-2 px-3 py-8 text-center text-xs">
                  <p>Sin coincidencias</p>
                  {filter && (
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => setFilter("")}
                      className="h-7 text-xs"
                    >
                      Limpiar
                    </Button>
                  )}
                </li>
              )}
            </ul>
          )}
        </aside>

        <div className="bg-void flex min-h-0 min-w-0 flex-1 flex-col">
          {error && (
            <div
              className="bg-coral-red/10 text-coral-red mx-7 mt-4 max-w-[76ch] flex-shrink-0 rounded-md px-3 py-2 text-sm"
              role="alert"
            >
              {error}
            </div>
          )}

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
            <section className="flex min-h-0 min-w-0 flex-1 flex-col">
              <header className="max-w-[82ch] flex-shrink-0 space-y-2.5 px-7 pt-5 pb-4">
                <div className="flex min-w-0 items-start justify-between gap-4">
                  <h2 className="text-paper min-w-0 flex-1 text-lg leading-snug font-semibold tracking-tight text-pretty break-words">
                    {selected.title}
                  </h2>

                  <div className="flex flex-shrink-0 items-center gap-1">
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => setEditingMemory(selected)}
                      aria-label="Editar"
                      title="Editar"
                      className="text-fog hover:text-paper hover:bg-obsidian h-8 w-8"
                    >
                      <Pencil className="h-4 w-4" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => togglePin(selected)}
                      aria-label={selected.pinned ? "Desfijar" : "Fijar"}
                      title={selected.pinned ? "Desfijar" : "Fijar"}
                      className="text-fog hover:text-paper hover:bg-obsidian h-8 w-8"
                    >
                      {selected.pinned ? (
                        <PinOff className="text-accent h-4 w-4" />
                      ) : (
                        <Pin className="h-4 w-4" />
                      )}
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => setPendingDelete(selected)}
                      aria-label="Eliminar"
                      title="Eliminar"
                      className="text-fog hover:text-coral-red hover:bg-coral-red/10 h-8 w-8"
                    >
                      <Trash2 className="h-4 w-4" />
                    </Button>
                  </div>
                </div>

                <div className="text-fog flex min-w-0 flex-wrap items-center gap-2 text-xs">
                  <MemoryKindBadge kind={selected.kind} />
                  {!project && selected.project_name && (
                    <span className="text-mist bg-obsidian inline-flex max-w-[180px] items-center gap-1 truncate rounded-md px-2 py-0.5">
                      <Folder className="text-ash h-3 w-3 flex-shrink-0" />
                      <span className="truncate">{selected.project_name}</span>
                    </span>
                  )}
                  <span
                    className="text-fog inline-flex items-center gap-1 tabular-nums"
                    title={formatFullDate(selected.updated_at)}
                  >
                    <Clock className="text-ash h-3 w-3 flex-shrink-0" />
                    <span>{formatTimeAgo(selected.updated_at)}</span>
                  </span>
                  {selected.use_count > 0 && (
                    <span
                      className="text-fog inline-flex items-center gap-1 tabular-nums"
                      title={`${selected.use_count} ${selected.use_count === 1 ? "consulta" : "consultas"}`}
                    >
                      <Eye className="text-ash h-3 w-3 flex-shrink-0" />
                      <span>{selected.use_count}</span>
                    </span>
                  )}
                </div>
              </header>

              <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-7">
                <div className="min-w-0 space-y-5">
                  <div className="min-w-0">
                    {selected.content.trim() ? (
                      <Markdown content={selected.content} />
                    ) : (
                      <p className="text-ash text-xs">Sin contenido</p>
                    )}
                  </div>
                  <MemoryLinks memory={selected} onOpen={openMemory} />
                </div>
              </div>
            </section>
          ) : (
            <div className="flex flex-1 flex-col items-center justify-center gap-2 px-6 text-center">
              <p className="text-fog text-sm">Sin coincidencias</p>
              {filter && (
                <Button variant="secondary" size="sm" onClick={() => setFilter("")}>
                  Limpiar
                </Button>
              )}
            </div>
          )}
        </div>

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
    </div>
  );
}
