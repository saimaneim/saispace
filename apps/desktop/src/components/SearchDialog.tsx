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
      <DialogContent className="bg-carbon top-[14%] max-w-xl translate-y-0 overflow-hidden p-0">
        <DialogTitle className="sr-only">Buscar</DialogTitle>
        <DialogDescription className="sr-only">
          Busca en notas de contexto, tareas, reglas y memorias.
        </DialogDescription>

        <div className="bg-obsidian flex h-12 items-center gap-3 px-4" onKeyDown={onKeyDown}>
          <Search className="text-fog h-4 w-4 flex-shrink-0" />
          <input
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Buscar…"
            aria-label="Consulta de búsqueda"
            className="text-paper placeholder:text-fog h-full min-w-0 flex-1 bg-transparent text-sm focus:outline-none"
          />
          {query && (
            <button
              type="button"
              onClick={() => setQuery("")}
              aria-label="Limpiar consulta"
              className="text-fog hover:text-paper hover:bg-elevated focus-visible:ring-accent/40 flex h-6 w-6 flex-shrink-0 items-center justify-center rounded transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          )}
          {activeProjectId && (
            <button
              type="button"
              onClick={() => setOnlyProject((v) => !v)}
              aria-pressed={onlyProject}
              className={cn(
                "focus-visible:ring-accent/40 mr-8 h-7 flex-shrink-0 rounded-md px-2.5 text-xs font-medium transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2",
                onlyProject ? "bg-elevated text-paper" : "text-fog hover:text-mist",
              )}
            >
              {onlyProject ? "Este proyecto" : "Todos"}
            </button>
          )}
        </div>

        <div
          className="max-h-[min(24rem,calc(100vh-12rem))] space-y-0.5 overflow-y-auto overscroll-contain p-2"
          onKeyDown={onKeyDown}
        >
          {error && <p className="text-coral-red px-3 py-2 text-sm">{error}</p>}
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
                  "relative flex w-full items-start gap-3 rounded-md py-2.5 pr-3 pl-3.5 text-left transition-colors duration-[120ms] focus:outline-none",
                  isSelected ? "bg-elevated" : "hover:bg-obsidian",
                )}
              >
                <Icon className="text-fog mt-0.5 h-4 w-4 flex-shrink-0" />
                <span className="min-w-0 flex-1">
                  <span className="text-paper block truncate text-sm font-medium">{hit.title}</span>
                  <span className="text-fog mt-0.5 line-clamp-2 block text-xs break-words">
                    <Snippet text={hit.snippet} />
                  </span>
                </span>
                <span className="text-fog bg-obsidian flex-shrink-0 rounded-md px-2 py-0.5 text-[11px]">
                  {KIND_LABEL[hit.kind]}
                </span>
              </button>
            );
          })}
          {!error && hasQuery && hits.length === 0 && (
            <p className="text-fog px-3 py-8 text-center text-sm">Sin resultados</p>
          )}
          {!hasQuery && (
            <p className="text-fog px-3 py-8 text-center text-xs">
              ↑↓ navegar · Enter abrir · Esc cerrar
            </p>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
