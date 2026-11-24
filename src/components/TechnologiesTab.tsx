import { ExternalLink, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { api, formatError } from "../lib/api";
import type { Project, Technology } from "../lib/types";
import { TechIcon } from "./TechIcon";
import { TechRulesSection } from "./TechRulesSection";
import { Button } from "./ui/button";

interface TechnologiesTabProps {
  project: Project;
}

const ECOSYSTEM_LABELS: Record<Technology["ecosystem"], string> = {
  cargo: "Rust",
  npm: "JavaScript y TypeScript",
  pypi: "Python",
  go: "Go",
};

export function TechnologiesTab({ project }: TechnologiesTabProps) {
  const [items, setItems] = useState<Technology[]>([]);
  const [isScanning, setIsScanning] = useState(false);
  const [isLoaded, setIsLoaded] = useState(false);
  const [query, setQuery] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [syncCount, setSyncCount] = useState(0);

  const scan = useCallback(async () => {
    setIsScanning(true);
    setError(null);
    try {
      const synced = await api.syncProjectStack(project.id);
      setItems(synced);
      setSyncCount((n) => n + 1);
    } catch (err) {
      setError(formatError(err));
    } finally {
      setIsScanning(false);
    }
  }, [project.id]);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const stored = await api.listTechnologies(project.id);
        if (cancelled) return;
        setItems(stored);
        setIsLoaded(true);
        if (stored.length === 0) await scan();
      } catch (err) {
        if (!cancelled) {
          setError(formatError(err));
          setIsLoaded(true);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [project.id, scan]);

  const grouped = useMemo(() => {
    const q = query.trim().toLowerCase();
    const map = new Map<Technology["ecosystem"], Technology[]>();
    for (const item of items) {
      if (q && !item.name.toLowerCase().includes(q)) continue;
      map.set(item.ecosystem, [...(map.get(item.ecosystem) ?? []), item]);
    }
    return [...map.entries()];
  }, [items, query]);

  return (
    <div className="flex-1 flex flex-col min-h-0 px-6 pb-6 gap-4">
      <div className="flex items-center gap-3">
        <div className="flex-1 min-w-0">
          <h2 className="text-sm font-semibold text-paper">Stack y reglas</h2>
          <p className="text-xs text-fog">
            Versiones detectadas en los manifiestos y reglas breves que reciben los agentes.
          </p>
        </div>
        {items.length > 12 && (
          <input
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Filtrar…"
            aria-label="Filtrar tecnologías"
            className="w-48 h-9 px-3 text-sm bg-obsidian text-paper placeholder:text-fog rounded-md focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
          />
        )}
        <Button variant="secondary" onClick={scan} disabled={isScanning}>
          <RefreshCw className={`w-4 h-4 mr-2 ${isScanning ? "animate-spin" : ""}`} />
          {isScanning ? "Escaneando…" : "Escanear"}
        </Button>
      </div>

      {error && (
        <div className="px-3 py-2 rounded-md bg-coral-red/10 text-coral-red text-sm" role="alert">
          {error}
        </div>
      )}

      <div className="flex-1 overflow-y-auto space-y-6 pr-1">
        <TechRulesSection projectId={project.id} stack={items} stackVersion={syncCount} />

        {grouped.map(([ecosystem, list]) => (
          <section key={ecosystem}>
            <h3 className="text-xs text-fog px-3 mb-1">
              {ECOSYSTEM_LABELS[ecosystem]} · {list.length}
            </h3>
            <ul>
              {list.map((tech) => (
                <li
                  key={tech.id}
                  className="group flex items-center gap-3 px-3 h-10 rounded-md hover:bg-obsidian transition-colors duration-[120ms]"
                >
                  <TechIcon name={tech.name} />
                  <span className="text-sm text-paper truncate flex-1">{tech.name}</span>
                  <span className="text-sm text-fog tabular-nums">{tech.version}</span>
                  <button
                    type="button"
                    onClick={() =>
                      api.openUrl(tech.docs_url).catch((e) => setError(formatError(e)))
                    }
                    aria-label={`Abrir documentación de ${tech.name} ${tech.version}`}
                    title="Abrir documentación de esta versión"
                    className="h-7 w-7 rounded-md flex items-center justify-center text-fog opacity-0 group-hover:opacity-100 focus-visible:opacity-100 hover:text-paper hover:bg-elevated transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
                  >
                    <ExternalLink className="w-4 h-4" />
                  </button>
                </li>
              ))}
            </ul>
          </section>
        ))}

        {isLoaded && !isScanning && grouped.length === 0 && (
          <div className="py-16 text-center text-sm text-fog">
            No se encontraron manifiestos (package.json, Cargo.toml…) en este proyecto.
          </div>
        )}
      </div>
    </div>
  );
}
