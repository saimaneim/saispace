import { ExternalLink, Search, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, formatError } from "../lib/api";
import type { Project, Technology } from "../lib/types";
import { cn } from "../lib/utils";
import { TechIcon } from "./TechIcon";
import { TechRulesSection } from "./TechRulesSection";

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

function groupFamilies(list: Technology[]): Family[] {
  const names = new Set(list.map((t) => t.name));
  const familyOf = (name: string): string => {
    if (name in FAMILY_OF) return FAMILY_OF[name];
    if (name.startsWith("@types/")) {
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
        "group w-full flex items-center gap-2.5 px-2.5 rounded-md text-left hover:bg-obsidian transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40 min-w-0",
        nested ? "h-8" : "h-10",
      )}
    >
      {nested ? (
        <span className="w-5 flex-shrink-0" />
      ) : (
        <TechIcon name={tech.name} className="w-5 h-5" />
      )}
      <span
        className={cn(
          "truncate flex-1 min-w-0",
          nested ? "text-xs text-mist" : "text-sm text-paper",
        )}
      >
        {tech.name}
      </span>
      <span className="flex items-center gap-1.5 flex-shrink-0">
        <span className="text-xs text-fog tabular-nums font-mono">{tech.version}</span>
        <ExternalLink className="w-3.5 h-3.5 text-fog opacity-0 group-hover:opacity-100 group-focus-visible:opacity-100 transition-opacity duration-[120ms]" />
      </span>
    </button>
  );
}

export function TechnologiesTab({ project }: TechnologiesTabProps) {
  const [items, setItems] = useState<Technology[]>([]);
  const [isLoaded, setIsLoaded] = useState(false);
  const [query, setQuery] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [syncCount, setSyncCount] = useState(0);
  const syncSeq = useRef(0);

  const sync = useCallback(async () => {
    const seq = ++syncSeq.current;
    try {
      const synced = await api.syncProjectStack(project.id);
      if (seq !== syncSeq.current) return;
      setItems(synced);
      setSyncCount((n) => n + 1);
      setError(null);
    } catch (err) {
      if (seq === syncSeq.current) setError(formatError(err));
    }
  }, [project.id]);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const stored = await api.listTechnologies(project.id);
        if (cancelled) return;
        setItems(stored);
      } catch (err) {
        if (!cancelled) setError(formatError(err));
      } finally {
        if (!cancelled) setIsLoaded(true);
      }
      if (!cancelled) await sync();
    })();
    const onFocus = () => void sync();
    window.addEventListener("focus", onFocus);
    return () => {
      cancelled = true;
      window.removeEventListener("focus", onFocus);
    };
  }, [project.id, sync]);

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
    <div className="flex-1 flex flex-col min-h-0 px-6 pt-2 pb-6 gap-4">
      {items.length > 12 && (
        <div className="flex items-center justify-end">
          <div className="relative w-56">
            <Search className="w-3.5 h-3.5 absolute left-2.5 top-1/2 -translate-y-1/2 text-fog pointer-events-none" />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Filtrar…"
              aria-label="Filtrar tecnologías"
              className="w-full h-9 pl-8 pr-7 text-sm bg-obsidian text-paper placeholder:text-fog rounded-md focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40 transition-colors duration-[120ms]"
            />
            {query && (
              <button
                type="button"
                onClick={() => setQuery("")}
                aria-label="Limpiar filtro"
                className="absolute right-1.5 top-1/2 -translate-y-1/2 h-6 w-6 rounded flex items-center justify-center text-fog hover:text-paper hover:bg-elevated transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40"
              >
                <X className="w-3.5 h-3.5" />
              </button>
            )}
          </div>
        </div>
      )}

      {error && (
        <div className="px-3 py-2 rounded-md bg-coral-red/10 text-coral-red text-sm" role="alert">
          {error}
        </div>
      )}

      <div className="flex-1 overflow-y-auto space-y-6 pr-1">
        <TechRulesSection projectId={project.id} stack={items} stackVersion={syncCount} />

        {!isLoaded && items.length === 0 && (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-2.5">
            <div className="h-24 rounded-xl bg-carbon" />
            <div className="h-24 rounded-xl bg-carbon/70" />
            <div className="h-24 rounded-xl bg-carbon/50" />
          </div>
        )}

        {grouped.map(([ecosystem, list]) => {
          const families = groupFamilies(list);
          const multi = families.filter((f) => f.members.length > 1);
          const singles = families.filter((f) => f.members.length === 1).map((f) => f.members[0]);
          return (
            <section key={ecosystem} className="space-y-2">
              <h3 className="flex items-center gap-2 text-xs font-semibold text-fog px-1 tabular-nums">
                <span>{ECOSYSTEM_LABELS[ecosystem]}</span>
                <span className="text-ash font-normal">{list.length}</span>
              </h3>
              {multi.length > 0 && (
                <div className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-2.5 items-start">
                  {multi.map((family) => (
                    <div key={family.key} className="bg-carbon rounded-xl p-1.5 min-w-0">
                      <div className="flex items-center gap-2.5 px-2.5 pt-1.5 pb-1.5">
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
                <div className="bg-carbon rounded-xl p-1.5">
                  <ul className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-0.5">
                    {singles.map((tech) => (
                      <li key={tech.id}>
                        <TechRow tech={tech} onError={setError} />
                      </li>
                    ))}
                  </ul>
                </div>
              )}
            </section>
          );
        })}

        {isLoaded && syncCount > 0 && grouped.length === 0 && !query && (
          <div className="py-16 text-center text-sm text-fog">Sin dependencias detectadas</div>
        )}

        {query && grouped.length === 0 && (
          <div className="py-12 text-center text-sm text-fog">
            Ninguna tecnología coincide con el filtro
          </div>
        )}
      </div>
    </div>
  );
}
