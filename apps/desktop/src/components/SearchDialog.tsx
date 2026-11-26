import { Brain, FileText, ListTodo, ScrollText, Search } from "lucide-react";
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
          <span key={i} className="text-acid-lime">
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
      // Invalidate any search still in flight so it cannot refill the list.
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
      <DialogContent className="max-w-xl p-0 top-[22%] translate-y-0 overflow-hidden">
        <DialogTitle className="sr-only">Buscar</DialogTitle>
        <DialogDescription className="sr-only">
          Busca en notas de contexto, tareas y reglas por tecnología.
        </DialogDescription>

        <div className="flex items-center gap-3 px-4 h-12 bg-obsidian" onKeyDown={onKeyDown}>
          <Search className="w-4 h-4 text-fog flex-shrink-0" />
          <input
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Buscar en contexto, tareas y reglas…"
            aria-label="Consulta de búsqueda"
            className="flex-1 min-w-0 h-full bg-transparent text-sm text-paper placeholder:text-fog focus:outline-none"
          />
          {activeProjectId && (
            <button
              type="button"
              onClick={() => setOnlyProject((v) => !v)}
              aria-pressed={onlyProject}
              className={cn(
                "h-7 px-2.5 mr-6 rounded-md text-xs flex-shrink-0 transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
                onlyProject ? "bg-elevated text-paper" : "text-fog hover:text-mist",
              )}
            >
              {onlyProject ? "Este proyecto" : "Todos"}
            </button>
          )}
        </div>

        <div className="max-h-96 overflow-y-auto p-2" onKeyDown={onKeyDown}>
          {error && <p className="px-3 py-2 text-sm text-coral-red">{error}</p>}
          {hits.map((hit, i) => {
            const Icon = KIND_ICON[hit.kind];
            return (
              <button
                key={`${hit.kind}-${hit.id}`}
                type="button"
                onClick={() => choose(hit)}
                onMouseEnter={() => setSelected(i)}
                className={cn(
                  "w-full flex items-start gap-3 px-3 py-2 rounded-md text-left transition-colors duration-[120ms] focus:outline-none",
                  i === selected ? "bg-elevated" : "hover:bg-obsidian",
                )}
              >
                <Icon className="w-4 h-4 mt-0.5 text-fog flex-shrink-0" />
                <span className="flex-1 min-w-0">
                  <span className="block text-sm text-paper truncate">{hit.title}</span>
                  <span className="block text-xs text-fog line-clamp-2">
                    <Snippet text={hit.snippet} />
                  </span>
                </span>
                <span className="text-xs text-fog flex-shrink-0">{KIND_LABEL[hit.kind]}</span>
              </button>
            );
          })}
          {!error && hasQuery && hits.length === 0 && (
            <p className="px-3 py-6 text-center text-sm text-fog">Sin resultados.</p>
          )}
          {!hasQuery && (
            <p className="px-3 py-6 text-center text-sm text-fog">
              Escribe para buscar. Usa las flechas y Enter para abrir.
            </p>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
