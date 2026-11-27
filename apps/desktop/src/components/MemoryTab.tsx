import {
  Brain,
  ChevronRight,
  Copy,
  Eye,
  History,
  Pencil,
  Pin,
  PinOff,
  Search,
  Trash2,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
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

export function MemoryTab({ project, refreshKey, focusId }: MemoryTabProps) {
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
    if (focusId) setSelectedId(focusId);
  }, [focusId]);

  useEffect(() => {
    if (!focusId || memories.length === 0 || memories.some((m) => m.id === focusId)) return;
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
  }, [focusId, memories]);

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

  const staleCount = staleReport.stale.length;

  const selected =
    memories.find((m) => m.id === selectedId) ??
    staleReport.stale.find((m) => m.id === selectedId) ??
    staleReport.surplus_sessions.find((m) => m.id === selectedId) ??
    visible[0] ??
    null;

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
      <div className="flex-1 flex min-h-0">
        <aside className="w-80 flex-shrink-0 bg-carbon flex flex-col min-h-0 p-3 gap-2">
          <div className="h-7 flex items-center justify-between px-1">
            <div className="h-3 w-16 rounded bg-obsidian" />
            <div className="h-6 w-24 rounded-md bg-obsidian" />
          </div>
          <div className="h-9 rounded-md bg-obsidian" />
          <div className="space-y-1.5 pt-1">
            <div className="h-14 rounded-md bg-obsidian/70" />
            <div className="h-14 rounded-md bg-obsidian/50" />
            <div className="h-14 rounded-md bg-obsidian/40" />
          </div>
        </aside>
        <section className="flex-1 min-w-0 px-7 pt-5 pb-7 space-y-4">
          <div className="h-6 w-28 rounded-md bg-carbon" />
          <div className="h-7 w-72 rounded-md bg-carbon" />
          <div className="h-44 max-w-[76ch] rounded-xl bg-carbon" />
        </section>
      </div>
    );
  }

  if (memories.length === 0) {
    return (
      <div className="flex-1 flex flex-col min-h-0">
        <div className="flex-1 flex flex-col items-center justify-center gap-2.5 text-center px-6">
          <div className="w-10 h-10 rounded-xl bg-carbon flex items-center justify-center text-ash">
            <Brain className="w-5 h-5" />
          </div>
          <p className="text-sm font-medium text-paper">Sin memorias</p>
          <p className="text-xs text-fog max-w-xs leading-relaxed">
            Se guardan con <code className="font-mono text-mist">save_memory</code> o{" "}
            <code className="font-mono text-mist">sai memory save</code>.
          </p>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => setAgentDialogOpen(true)}
            className="mt-1 gap-1.5 text-xs"
          >
            <Eye className="w-3.5 h-3.5" />
            Vista del agente
          </Button>
          {error && <p className="text-xs text-coral-red">{error}</p>}
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
    <div className="flex-1 flex flex-col min-h-0">
      <div className="flex-1 flex min-h-0">
        <aside className="w-80 flex-shrink-0 bg-carbon flex flex-col min-h-0">
          <div className="px-3 pt-3 pb-2 space-y-2 flex-shrink-0">
            <div className="flex items-center justify-between gap-2 px-1 h-7">
              <span className="text-xs font-medium text-fog tabular-nums">
                {visible.length} {visible.length === 1 ? "memoria" : "memorias"}
              </span>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setAgentDialogOpen(true)}
                className="h-7 px-2 text-xs gap-1.5 text-fog hover:text-paper hover:bg-obsidian"
              >
                <Eye className="w-3.5 h-3.5" />
                Vista del agente
              </Button>
            </div>

            {scopeFilter !== "orphans" && (
              <div className="relative">
                <Search className="w-3.5 h-3.5 absolute left-2.5 top-1/2 -translate-y-1/2 text-fog pointer-events-none" />
                <input
                  value={filter}
                  onChange={(e) => setFilter(e.target.value)}
                  placeholder="Filtrar memorias…"
                  aria-label="Filtrar memorias"
                  className="w-full h-9 pl-8 pr-8 text-sm bg-obsidian text-paper placeholder:text-fog rounded-md focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40 transition-colors duration-[120ms]"
                />
                {filter && (
                  <button
                    type="button"
                    onClick={() => setFilter("")}
                    aria-label="Limpiar filtro"
                    className="absolute right-1.5 top-1/2 -translate-y-1/2 h-6 w-6 rounded flex items-center justify-center text-fog hover:text-paper hover:bg-elevated transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
                  >
                    <X className="w-3.5 h-3.5" />
                  </button>
                )}
              </div>
            )}

            {!project && (
              <div className="flex items-center gap-1 bg-obsidian p-1 rounded-lg flex-shrink-0 select-none">
                <button
                  type="button"
                  onClick={() => setScopeFilter("all")}
                  className={cn(
                    "flex-1 h-7 text-xs rounded-md transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
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
                    "flex-1 h-7 text-xs rounded-md transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
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
                    "flex-1 h-7 text-xs rounded-md transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
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
              <div className="space-y-1">
                {activeDuplicates.length > 0 && (
                  <button
                    type="button"
                    onClick={() => {
                      setShowDuplicates(true);
                      setShowStale(false);
                    }}
                    className={cn(
                      "w-full flex items-center gap-2 px-2.5 h-8 text-xs font-medium rounded-md transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
                      showDuplicates
                        ? "bg-status-amber/20 text-status-amber"
                        : "bg-status-amber/10 text-status-amber hover:bg-status-amber/15",
                    )}
                  >
                    <Copy className="w-3.5 h-3.5 flex-shrink-0" />
                    <span className="truncate flex-1 text-left">
                      {activeDuplicates.length === 1
                        ? "1 posible duplicado"
                        : `${activeDuplicates.length} posibles duplicados`}
                    </span>
                    <ChevronRight className="w-3.5 h-3.5 flex-shrink-0 opacity-75" />
                  </button>
                )}

                {staleCount > 0 && (
                  <button
                    type="button"
                    onClick={() => {
                      setShowStale(true);
                      setShowDuplicates(false);
                    }}
                    className={cn(
                      "w-full flex items-center gap-2 px-2.5 h-8 text-xs font-medium rounded-md transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
                      showStale
                        ? "bg-status-amber/20 text-status-amber"
                        : "bg-status-amber/10 text-status-amber hover:bg-status-amber/15",
                    )}
                  >
                    <History className="w-3.5 h-3.5 flex-shrink-0" />
                    <span className="truncate flex-1 text-left">
                      {staleCount === 1 ? "1 para revisar" : `${staleCount} para revisar`}
                    </span>
                    <ChevronRight className="w-3.5 h-3.5 flex-shrink-0 opacity-75" />
                  </button>
                )}
              </div>
            )}
          </div>

          {scopeFilter !== "orphans" && (
            <ul className="flex-1 overflow-y-auto px-2 pb-3 space-y-0.5">
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
                        "group relative w-full text-left pl-3 pr-2.5 py-2.5 rounded-md transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
                        isSelected ? "bg-elevated" : "hover:bg-obsidian",
                      )}
                    >
                      {isSelected && (
                        <span
                          className="absolute left-1 top-2.5 bottom-2.5 w-0.5 rounded-full bg-acid-lime"
                          aria-hidden="true"
                        />
                      )}
                      <div className="flex items-center gap-1.5 min-w-0">
                        {m.pinned && <Pin className="w-3.5 h-3.5 text-acid-lime flex-shrink-0" />}
                        <span
                          className={cn(
                            "block text-sm truncate font-medium flex-1 min-w-0",
                            isSelected ? "text-paper" : "text-bone group-hover:text-paper",
                          )}
                        >
                          {m.title}
                        </span>
                      </div>
                      <div className="mt-1 flex items-center justify-between gap-2 min-w-0">
                        <div className="flex items-center gap-1.5 min-w-0 flex-1">
                          <span
                            className={cn(
                              "w-1.5 h-1.5 rounded-full flex-shrink-0",
                              (MEMORY_KIND_STYLE[m.kind] ?? MEMORY_KIND_STYLE.note).dot,
                            )}
                            aria-hidden="true"
                          />
                          <span className="truncate text-xs text-fog tabular-nums">
                            {KIND_LABELS[m.kind]}
                            {!project ? ` · ${m.project_name || "Global"}` : ""}
                            {` · ${formatTimeAgo(m.updated_at)}`}
                          </span>
                        </div>
                        {project && m.topic_key && (
                          <span className="truncate font-mono text-[11px] text-ash max-w-[110px] flex-shrink-0">
                            {m.topic_key}
                          </span>
                        )}
                      </div>
                    </button>
                  </li>
                );
              })}

              {visible.length === 0 && (
                <li className="px-3 py-8 text-center text-xs text-fog space-y-2">
                  <p>Sin coincidencias</p>
                  {filter && (
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => setFilter("")}
                      className="h-7 text-xs"
                    >
                      Limpiar filtro
                    </Button>
                  )}
                </li>
              )}
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
          <section className="flex-1 min-w-0 flex flex-col bg-void">
            <header className="px-7 pt-5 pb-4 flex-shrink-0 space-y-2.5 max-w-[82ch]">
              <div className="flex flex-wrap items-center justify-between gap-3 min-w-0">
                <div className="flex flex-wrap items-center gap-1.5 text-xs text-fog min-w-0">
                  <MemoryKindBadge kind={selected.kind} />
                  {selected.topic_key && (
                    <>
                      <span aria-hidden="true" className="text-ash">
                        {" · "}
                      </span>
                      <span
                        className="font-mono text-[11.5px] text-mist bg-obsidian px-2 py-0.5 rounded-md truncate max-w-[240px]"
                        title={selected.topic_key}
                      >
                        {selected.topic_key}
                      </span>
                    </>
                  )}
                  {!project && selected.project_name && (
                    <>
                      <span aria-hidden="true" className="text-ash">
                        {" · "}
                      </span>
                      <span className="text-mist truncate max-w-[180px]">
                        {selected.project_name}
                      </span>
                    </>
                  )}
                  <span aria-hidden="true" className="text-ash">
                    {" · "}
                  </span>
                  <span
                    className="tabular-nums text-fog"
                    title={formatFullDate(selected.updated_at)}
                  >
                    {formatTimeAgo(selected.updated_at)}
                  </span>
                  {selected.use_count > 0 && (
                    <>
                      <span aria-hidden="true" className="text-ash">
                        {" · "}
                      </span>
                      <span className="tabular-nums text-fog">
                        {selected.use_count} {selected.use_count === 1 ? "consulta" : "consultas"}
                      </span>
                    </>
                  )}
                </div>

                <div className="flex items-center gap-1.5 flex-shrink-0">
                  <Button variant="secondary" size="sm" onClick={() => setEditingMemory(selected)}>
                    <Pencil className="w-3.5 h-3.5 mr-1.5" />
                    Editar
                  </Button>
                  <Button variant="secondary" size="sm" onClick={() => togglePin(selected)}>
                    {selected.pinned ? (
                      <>
                        <PinOff className="w-3.5 h-3.5 mr-1.5 text-acid-lime" />
                        Desfijar
                      </>
                    ) : (
                      <>
                        <Pin className="w-3.5 h-3.5 mr-1.5" />
                        Fijar
                      </>
                    )}
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => setPendingDelete(selected)}
                    className="text-fog hover:text-coral-red hover:bg-coral-red/10"
                  >
                    <Trash2 className="w-3.5 h-3.5 mr-1.5" />
                    Eliminar
                  </Button>
                </div>
              </div>

              <h2 className="text-lg font-semibold text-paper tracking-tight leading-snug break-words text-pretty">
                {selected.title}
              </h2>
            </header>

            {error && (
              <div
                className="mx-7 mb-3 max-w-[76ch] px-3 py-2 rounded-md bg-coral-red/10 text-coral-red text-sm"
                role="alert"
              >
                {error}
              </div>
            )}

            <div className="flex-1 min-h-0 overflow-y-auto px-7 pb-7">
              <div className="max-w-[76ch] space-y-3">
                <div className="bg-carbon rounded-xl px-6 py-5 min-w-0">
                  {selected.content.trim() ? (
                    <Markdown content={selected.content} />
                  ) : (
                    <p className="text-xs text-ash">Sin contenido</p>
                  )}
                </div>
                <MemoryLinks memory={selected} onOpen={openMemory} />
              </div>
            </div>
          </section>
        ) : (
          <div className="flex-1 flex flex-col items-center justify-center gap-2 text-center px-6">
            <p className="text-sm text-fog">Ninguna memoria coincide con el filtro</p>
            {filter && (
              <Button variant="secondary" size="sm" onClick={() => setFilter("")}>
                Limpiar filtro
              </Button>
            )}
          </div>
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
