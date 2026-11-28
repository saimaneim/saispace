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
  typescript: "TypeScript",
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
    // Type packages join their library; orphan ones (@types/node, @types/bun)
    // join TypeScript, which is what they exist for.
    if (name.startsWith("@types/")) {
      const base = name.slice("@types/".length).replace(/^(.+)__(.+)$/, "@$1/$2");
      if (names.has(base) || base.startsWith("@")) return familyOf(base);
      return "typescript";
    }
    if (name in FAMILY_OF) return FAMILY_OF[name];
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
        "group hover:bg-obsidian focus-visible:ring-accent/40 flex w-full min-w-0 items-center gap-2.5 rounded-md px-2.5 text-left transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2",
        nested ? "h-8 pl-3.5" : "h-10",
      )}
    >
      {!nested && <TechIcon name={tech.name} className="h-5 w-5" />}
      <span
        className={cn(
          "min-w-0 flex-1 truncate",
          nested ? "text-mist text-xs" : "text-paper text-sm",
        )}
      >
        {tech.name}
      </span>
      <span className="flex flex-shrink-0 items-center gap-1.5">
        <span className="text-fog font-mono text-xs tabular-nums">{tech.version}</span>
        <ExternalLink className="text-fog h-3.5 w-3.5 opacity-0 transition-opacity duration-[120ms] group-hover:opacity-100 group-focus-visible:opacity-100" />
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
  const hasSynced = useRef(false);

  const sync = useCallback(async () => {
    const seq = ++syncSeq.current;
    try {
      const synced = await api.syncProjectStack(project.id);
      if (seq !== syncSeq.current) return;
      hasSynced.current = true;
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
      hasSynced.current = false;
      try {
        const stored = await api.listTechnologies(project.id);
        if (cancelled) return;
        // A sync that already succeeded holds fresher data.
        if (!hasSynced.current) setItems(stored);
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
    <div className="flex min-h-0 flex-1 flex-col gap-4 px-6 pt-2 pb-6">
      {items.length > 12 && (
        <div className="flex items-center justify-end">
          <div className="relative w-56">
            <Search className="text-fog pointer-events-none absolute top-1/2 left-2.5 h-3.5 w-3.5 -translate-y-1/2" />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Filtrar…"
              aria-label="Filtrar tecnologías"
              className="bg-obsidian text-paper placeholder:text-fog focus-visible:ring-accent/40 h-9 w-full rounded-md pr-7 pl-8 text-sm transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2"
            />
            {query && (
              <button
                type="button"
                onClick={() => setQuery("")}
                aria-label="Limpiar filtro"
                className="text-fog hover:text-paper hover:bg-elevated focus-visible:ring-accent/40 absolute top-1/2 right-1.5 flex h-6 w-6 -translate-y-1/2 items-center justify-center rounded transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2"
              >
                <X className="h-3.5 w-3.5" />
              </button>
            )}
          </div>
        </div>
      )}

      {error && (
        <div className="bg-coral-red/10 text-coral-red rounded-md px-3 py-2 text-sm" role="alert">
          {error}
        </div>
      )}

      <div className="flex-1 space-y-6 overflow-y-auto pr-1">
        <TechRulesSection projectId={project.id} stack={items} stackVersion={syncCount} />

        {!isLoaded && items.length === 0 && (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-2.5">
            <div className="bg-carbon h-24 rounded-xl" />
            <div className="bg-carbon/70 h-24 rounded-xl" />
            <div className="bg-carbon/50 h-24 rounded-xl" />
          </div>
        )}

        {grouped.map(([ecosystem, list]) => (
          <section key={ecosystem} className="space-y-2">
            <h3 className="text-fog flex items-center gap-2 px-1 text-xs font-semibold tabular-nums">
              <span>{ECOSYSTEM_LABELS[ecosystem]}</span>
              <span className="text-ash font-normal">{list.length}</span>
            </h3>
            <ul className="bg-carbon columns-[250px] gap-x-1.5 rounded-xl p-1.5">
              {groupFamilies(list).map((family) =>
                family.members.length === 1 ? (
                  <li key={family.key} className="break-inside-avoid">
                    <TechRow tech={family.members[0]} onError={setError} />
                  </li>
                ) : (
                  <li key={family.key} className="break-inside-avoid pb-1">
                    <div className="flex h-10 min-w-0 items-center gap-2.5 px-2.5">
                      <TechIcon name={family.icon} className="h-5 w-5" />
                      <span className="text-paper flex-1 truncate text-sm">{family.label}</span>
                      <span className="text-fog text-xs tabular-nums">{family.members.length}</span>
                    </div>
                    <ul className="border-obsidian ml-[19px] border-l">
                      {family.members.map((tech) => (
                        <li key={tech.id}>
                          <TechRow tech={tech} nested onError={setError} />
                        </li>
                      ))}
                    </ul>
                  </li>
                ),
              )}
            </ul>
          </section>
        ))}

        {isLoaded && syncCount > 0 && grouped.length === 0 && !query && (
          <div className="text-fog py-16 text-center text-sm">Sin dependencias detectadas</div>
        )}

        {query && grouped.length === 0 && (
          <div className="text-fog py-12 text-center text-sm">
            Ninguna tecnología coincide con el filtro
          </div>
        )}
      </div>
    </div>
  );
}
