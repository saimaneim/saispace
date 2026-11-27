import { Brain, FileText, ListTodo, ScrollText, Search, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api, formatError } from "../lib/api";
import type { SearchHit, SearchKind } from "../lib/types";
import { cn } from "../lib/utils";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "./ui/dialog";

interface SearchDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  activeProjectId: string | null;
  onSelectHit: (hit: SearchHit) => void;
}

const KIND_ICON = {
  project: FileText,
  task: ListTodo,
  tech_rule: ScrollText,
  memory: Brain,
} satisfies Record<SearchKind, typeof FileText>;

const KIND_LABEL: Record<SearchKind, string> = {
  project: "Contexto",
  task: "Tarea",
  tech_rule: "Regla",
  memory: "Memoria",
};

const DEBOUNCE_MS = 150;

function Snippet({ text }: { text: string }) {
  const parts = text.split(/(\[[^\]]*\])/g);
  return (
    <>
      {parts.map((part, i) =>
        part.startsWith("[") && part.endsWith("]") ? (
          <span key={i} className="text-accent font-medium">
            {part.slice(1, -1)}
          </span>
        ) : (
          <span key={i}>{part}</span>
        ),
      )}
    </>
  );
}

export function SearchDialog({
  open,
  onOpenChange,
  activeProjectId,
  onSelectHit,
}: SearchDialogProps) {
  const [query, setQuery] = useState("");
  const [onlyProject, setOnlyProject] = useState(true);
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [selected, setSelected] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const requestId = useRef(0);

  useEffect(() => {
    if (!open) {
      setQuery("");
      setHits([]);
      setError(null);
      setSelected(0);
    }
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const trimmed = query.trim();
    if (!/[\p{L}\p{N}]/u.test(trimmed)) {
      requestId.current++;
      setHits([]);
      setError(null);
      return;
    }
    const current = ++requestId.current;
    const timer = setTimeout(async () => {
      try {
        const scope = onlyProject ? activeProjectId : null;
        const found = await api.searchMemory(trimmed, scope, 30);
        if (current !== requestId.current) return;
        setHits(found);
        setSelected(0);
        setError(null);
      } catch (err) {
        if (current === requestId.current) setError(formatError(err));
      }
    }, DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [query, onlyProject, activeProjectId, open]);

  const choose = (hit: SearchHit) => {
    onOpenChange(false);
    onSelectHit(hit);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelected((i) => Math.min(i + 1, hits.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelected((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter" && hits[selected]) {
      e.preventDefault();
      choose(hits[selected]);
    }
  };

  const hasQuery = /[\p{L}\p{N}]/u.test(query);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-xl p-0 top-[14%] translate-y-0 overflow-hidden bg-carbon">
        <DialogTitle className="sr-only">Buscar</DialogTitle>
        <DialogDescription className="sr-only">
          Busca en notas de contexto, tareas, reglas y memorias.
        </DialogDescription>

        <div className="flex items-center gap-3 px-4 h-12 bg-obsidian" onKeyDown={onKeyDown}>
          <Search className="w-4 h-4 text-fog flex-shrink-0" />
          <input
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Buscar…"
            aria-label="Consulta de búsqueda"
            className="flex-1 min-w-0 h-full bg-transparent text-sm text-paper placeholder:text-fog focus:outline-none"
          />
          {query && (
            <button
              type="button"
              onClick={() => setQuery("")}
              aria-label="Limpiar consulta"
              className="h-6 w-6 rounded flex items-center justify-center text-fog hover:text-paper hover:bg-elevated transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40 flex-shrink-0"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          )}
          {activeProjectId && (
            <button
              type="button"
              onClick={() => setOnlyProject((v) => !v)}
              aria-pressed={onlyProject}
              className={cn(
                "h-7 px-2.5 mr-8 rounded-md text-xs font-medium flex-shrink-0 transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40",
                onlyProject ? "bg-elevated text-paper" : "text-fog hover:text-mist",
              )}
            >
              {onlyProject ? "Este proyecto" : "Todos"}
            </button>
          )}
        </div>

        <div
          className="max-h-[min(24rem,calc(100vh-12rem))] overflow-y-auto overscroll-contain p-2 space-y-0.5"
          onKeyDown={onKeyDown}
        >
          {error && <p className="px-3 py-2 text-sm text-coral-red">{error}</p>}
          {hits.map((hit, i) => {
            const Icon = KIND_ICON[hit.kind];
            const isSelected = i === selected;
            return (
              <button
                key={`${hit.kind}-${hit.id}`}
                type="button"
                onClick={() => choose(hit)}
                onMouseEnter={() => setSelected(i)}
                className={cn(
                  "relative w-full flex items-start gap-3 pl-3.5 pr-3 py-2.5 rounded-md text-left transition-colors duration-[120ms] focus:outline-none",
                  isSelected ? "bg-elevated" : "hover:bg-obsidian",
                )}
              >
                <Icon className="w-4 h-4 mt-0.5 text-fog flex-shrink-0" />
                <span className="flex-1 min-w-0">
                  <span className="block text-sm font-medium text-paper truncate">{hit.title}</span>
                  <span className="block text-xs text-fog line-clamp-2 mt-0.5 break-words">
                    <Snippet text={hit.snippet} />
                  </span>
                </span>
                <span className="text-[11px] text-fog bg-obsidian px-2 py-0.5 rounded-md flex-shrink-0">
                  {KIND_LABEL[hit.kind]}
                </span>
              </button>
            );
          })}
          {!error && hasQuery && hits.length === 0 && (
            <p className="px-3 py-8 text-center text-sm text-fog">Sin resultados</p>
          )}
          {!hasQuery && (
            <p className="px-3 py-8 text-center text-xs text-fog">
              ↑↓ navegar · Enter abrir · Esc cerrar
            </p>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
