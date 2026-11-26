import { ExternalLink, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { api, formatError } from "../lib/api";
import type { Project, Technology } from "../lib/types";
import { cn } from "../lib/utils";
import { TechIcon } from "./TechIcon";
import { TechRulesSection } from "./TechRulesSection";
import { Button } from "./ui/button";

interface TechnologiesTabProps {
  project: Project;
}

const ECOSYSTEM_LABELS: Record<Technology["ecosystem"], string> = {
  runtime: "Entorno de ejecución",
  cargo: "Rust",
  npm: "JavaScript y TypeScript",
  pypi: "Python",
  go: "Go",
};

const SCOPE_FAMILY: Record<string, string> = {
  "@vitejs": "vite",
  "@tailwindcss": "tailwindcss",
  "@biomejs": "biome",
  "@tauri-apps": "tauri",
  "@radix-ui": "radix-ui",
  "@babel": "babel",
  "@hugeicons": "hugeicons",
  "@hono": "hono",
  "@tanstack": "tanstack",
  "@whiskeysockets": "baileys",
  "@fontsource": "fonts",
  "@fontsource-variable": "fonts",
};

/** Packages that belong to a family their name does not reveal. */
const FAMILY_OF: Record<string, string> = {
  "tailwind-merge": "tailwindcss",
  "tw-animate-css": "tailwindcss",
  "tailwindcss-animate": "tailwindcss",
  "prettier-plugin-tailwindcss": "tailwindcss",
  "babel-plugin-react-compiler": "babel",
  "drizzle-orm": "drizzle",
  "drizzle-kit": "drizzle",
  "@types/node": "node",
  "@types/bun": "bun",
  "radix-ui": "radix-ui",
  "eslint-config-next": "eslint",
  pg: "postgres",
};

const FAMILY_LABELS: Record<string, string> = {
  "radix-ui": "Radix UI",
  tailwindcss: "Tailwind CSS",
  drizzle: "Drizzle",
  node: "Node.js",
  babel: "Babel",
  hugeicons: "Hugeicons",
  tanstack: "TanStack",
  postgres: "PostgreSQL",
  "@types": "Tipos",
  fonts: "Fuentes",
};

const ECOSYSTEM_ORDER: Technology["ecosystem"][] = ["runtime", "npm", "cargo", "pypi", "go"];

interface Family {
  key: string;
  label: string;
  icon: string;
  members: Technology[];
}

/** Groups related packages: react + react-dom + @types/react, @radix-ui/*, serde + serde_json… */
function groupFamilies(list: Technology[]): Family[] {
  const names = new Set(list.map((t) => t.name));
  const familyOf = (name: string): string => {
    if (name in FAMILY_OF) return FAMILY_OF[name];
    if (name.startsWith("@types/")) {
      // @types/babel__traverse types @babel/traverse.
      const base = name.slice("@types/".length).replace(/^(.+)__(.+)$/, "@$1/$2");
      return names.has(base) || base.startsWith("@") ? familyOf(base) : "@types";
    }
    const scope = name.match(/^(@[^/]+)\//)?.[1];
    if (scope) return SCOPE_FAMILY[scope] ?? scope;
    const prefix = name.split(/[-_]/)[0];
    return prefix !== name && names.has(prefix) ? prefix : name;
  };
  const map = new Map<string, Technology[]>();
  for (const tech of list) {
    const key = familyOf(tech.name);
    map.set(key, [...(map.get(key) ?? []), tech]);
  }
  return [...map.entries()]
    .map(([key, members]) => {
      const sorted = [...members].sort(
        (a, b) =>
          Number(b.name === key) - Number(a.name === key) ||
          Number(a.name.startsWith("@types/")) - Number(b.name.startsWith("@types/")) ||
          a.name.localeCompare(b.name),
      );
      return {
        key,
        label: FAMILY_LABELS[key] ?? key,
        // Members map to their brand logo (tailwind-merge shows Tailwind).
        icon: sorted[0].name,
        members: sorted,
      };
    })
    .sort((a, b) => a.label.replace(/^@/, "").localeCompare(b.label.replace(/^@/, "")));
}

function TechRow({
  tech,
  nested,
  onError,
}: {
  tech: Technology;
  nested?: boolean;
  onError: (message: string) => void;
}) {
  return (
    <button
      type="button"
      onClick={() => api.openUrl(tech.docs_url).catch((e) => onError(formatError(e)))}
      title={`Abrir documentación de ${tech.name} ${tech.version}`}
      className={cn(
        "group w-full flex items-center gap-2.5 px-2 rounded-md text-left hover:bg-obsidian transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
        nested ? "h-8" : "h-9",
      )}
    >
      {nested ? (
        <span className="w-5 flex-shrink-0" />
      ) : (
        <TechIcon name={tech.name} className="w-5 h-5" />
      )}
      <span className={cn("truncate flex-1", nested ? "text-xs text-mist" : "text-sm text-paper")}>
        {tech.name}
      </span>
      <span className="text-xs text-fog tabular-nums group-hover:hidden group-focus-visible:hidden">
        {tech.version}
      </span>
      <ExternalLink className="w-3.5 h-3.5 text-fog hidden group-hover:block group-focus-visible:block" />
    </button>
  );
}

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
    return [...map.entries()].sort(
      ([a], [b]) => ECOSYSTEM_ORDER.indexOf(a) - ECOSYSTEM_ORDER.indexOf(b),
    );
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

        {grouped.map(([ecosystem, list]) => {
          const families = groupFamilies(list);
          const multi = families.filter((f) => f.members.length > 1);
          const singles = families.filter((f) => f.members.length === 1).map((f) => f.members[0]);
          return (
            <section key={ecosystem} className="space-y-2">
              <h3 className="text-xs font-semibold text-fog px-1">
                {ECOSYSTEM_LABELS[ecosystem]} · {list.length} paquetes
              </h3>
              {multi.length > 0 && (
                <div className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-2 items-start">
                  {multi.map((family) => (
                    <div key={family.key} className="bg-carbon rounded-lg p-1.5 min-w-0">
                      <div className="flex items-center gap-2.5 px-2 pt-1 pb-1.5">
                        <TechIcon name={family.icon} className="w-5 h-5" />
                        <span className="text-sm font-medium text-paper truncate flex-1">
                          {family.label}
                        </span>
                        <span className="text-xs text-fog tabular-nums">
                          {family.members.length}
                        </span>
                      </div>
                      <ul>
                        {family.members.map((tech) => (
                          <li key={tech.id}>
                            <TechRow tech={tech} nested onError={setError} />
                          </li>
                        ))}
                      </ul>
                    </div>
                  ))}
                </div>
              )}
              {singles.length > 0 && (
                <ul className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-x-2">
                  {singles.map((tech) => (
                    <li key={tech.id}>
                      <TechRow tech={tech} onError={setError} />
                    </li>
                  ))}
                </ul>
              )}
            </section>
          );
        })}

        {isLoaded && !isScanning && grouped.length === 0 && (
          <div className="py-16 text-center text-sm text-fog">
            No se encontraron manifiestos (package.json, Cargo.toml…) en este proyecto.
          </div>
        )}
      </div>
    </div>
  );
}
