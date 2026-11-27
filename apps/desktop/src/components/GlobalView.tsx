import { Brain, FileText, KeyRound, Plug, Search, Sparkles, X } from "lucide-react";
import type React from "react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { api, formatError } from "../lib/api";
import type { AiInventory } from "../lib/types";
import { cn } from "../lib/utils";
import { Markdown } from "./Markdown";
import { MemoryTab } from "./MemoryTab";
import { MemoryUsagePanel } from "./MemoryUsagePanel";
import { SecretsPanel } from "./SecretsPanel";
import { TechIcon } from "./TechIcon";

type GlobalTab = "memory" | "instructions" | "skills" | "connections" | "secrets";

interface GlobalViewProps {
  refreshKey: string | null;
  pendingRequests: number;
  onSecretsChanged: () => void;
}

const EMPTY: AiInventory = {
  instructions: [],
  skills: [],
  mcp_servers: [],
  plugins: [],
  hooks: [],
  warnings: [],
};

function SectionTitle({ children, className }: { children: string; className?: string }) {
  return (
    <h3 className={cn("text-xs font-semibold text-fog px-1 pt-5 pb-2 tabular-nums", className)}>
      {children}
    </h3>
  );
}

function ToolBadge({ tool, disabled }: { tool: string; disabled?: boolean }) {
  return (
    <span
      className="flex items-center gap-1.5 flex-shrink-0"
      title={disabled ? `${tool} · desactivado` : tool}
    >
      {disabled && <span className="text-xs text-fog">desactivado</span>}
      <TechIcon name={tool} className="w-4 h-4" />
      <span className="sr-only">{tool}</span>
    </span>
  );
}

function InstructionsPanel({ inventory }: { inventory: AiInventory }) {
  const [selected, setSelected] = useState(0);
  const file = inventory.instructions[selected];
  if (!file) {
    return (
      <p className="flex-1 flex items-center justify-center text-sm text-fog">
        Sin instrucciones globales.
      </p>
    );
  }
  return (
    <div className="flex-1 flex min-h-0">
      <aside className="w-64 flex-shrink-0 bg-carbon p-3 space-y-0.5 overflow-y-auto">
        {inventory.instructions.map((item, index) => {
          const isSelected = index === selected;
          return (
            <button
              key={item.path}
              type="button"
              onClick={() => setSelected(index)}
              aria-current={isSelected ? "true" : undefined}
              className={cn(
                "relative w-full flex items-center gap-2.5 pl-3 pr-2.5 py-2 rounded-md text-left transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40",
                isSelected ? "bg-elevated text-paper" : "text-mist hover:bg-obsidian",
              )}
            >
              {isSelected && (
                <span
                  className="absolute left-1 top-2 bottom-2 w-0.5 rounded-full bg-accent"
                  aria-hidden="true"
                />
              )}
              <TechIcon name={item.tool} className="w-5 h-5" />
              <span className="min-w-0 flex-1">
                <span className="block text-sm font-medium truncate">{item.tool}</span>
                <span className="block text-xs text-fog truncate font-mono">
                  {item.path.split("/").pop()}
                </span>
              </span>
            </button>
          );
        })}
      </aside>
      <div className="flex-1 overflow-y-auto px-7 py-5">
        <div className="max-w-[76ch] space-y-3">
          <p className="text-xs text-fog font-mono truncate select-text" title={file.path}>
            {file.path}
          </p>
          <div className="bg-carbon rounded-xl px-6 py-5">
            <Markdown content={file.content} />
          </div>
          {file.truncated && <p className="text-xs text-status-amber">Fichero recortado.</p>}
        </div>
      </div>
    </div>
  );
}

function SkillsPanel({ inventory }: { inventory: AiInventory }) {
  const [query, setQuery] = useState("");
  const groups = useMemo(() => {
    const q = query.trim().toLowerCase();
    const map = new Map<string, AiInventory["skills"]>();
    for (const skill of inventory.skills) {
      if (q && !`${skill.name} ${skill.description}`.toLowerCase().includes(q)) continue;
      map.set(skill.source, [...(map.get(skill.source) ?? []), skill]);
    }
    return [...map.entries()];
  }, [inventory.skills, query]);

  return (
    <div className="flex-1 overflow-y-auto px-6 pt-3 pb-6">
      <div className="relative w-full max-w-sm mb-4">
        <Search className="w-3.5 h-3.5 absolute left-3 top-1/2 -translate-y-1/2 text-fog pointer-events-none" />
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={`Filtrar ${inventory.skills.length} skills…`}
          aria-label="Filtrar skills"
          className="w-full h-9 pl-8 pr-8 text-sm bg-obsidian text-paper placeholder:text-fog rounded-md focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40 transition-colors duration-[120ms]"
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
      {groups.length === 0 ? (
        <p className="py-12 text-center text-sm text-fog">
          {query ? "Ninguna skill coincide con el filtro." : "Sin skills instaladas."}
        </p>
      ) : (
        groups.map(([source, skills], index) => (
          <section key={source}>
            <SectionTitle className={index === 0 ? "pt-1" : "pt-6"}>
              {`${source} · ${skills.length}`}
            </SectionTitle>
            <div className="grid grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-3">
              {skills.map((skill) => (
                <div
                  key={skill.path}
                  className="bg-carbon hover:bg-obsidian/80 transition-colors duration-[120ms] rounded-xl px-4 py-3.5 min-w-0"
                  title={skill.path}
                >
                  <p className="text-sm font-medium text-paper truncate">{skill.name}</p>
                  <p className="text-xs text-fog line-clamp-2 mt-1 leading-relaxed break-words">
                    {skill.description || "Sin descripción"}
                  </p>
                </div>
              ))}
            </div>
          </section>
        ))
      )}
    </div>
  );
}

function Row({
  icon,
  title,
  detail,
  meta,
  muted,
}: {
  icon: string;
  title: string;
  detail: string;
  meta?: React.ReactNode;
  muted?: boolean;
}) {
  return (
    <div
      className={cn(
        "flex items-center gap-3 px-3.5 h-11 rounded-lg bg-carbon hover:bg-obsidian/80 transition-colors duration-[120ms] min-w-0",
        muted && "opacity-50",
      )}
    >
      <TechIcon name={icon} className="w-5 h-5" />
      <span className="text-sm font-medium text-paper w-44 truncate flex-shrink-0">{title}</span>
      <span className="text-xs text-fog font-mono truncate flex-1 min-w-0 select-text">
        {detail}
      </span>
      {meta}
    </div>
  );
}

function ConnectionsPanel({ inventory }: { inventory: AiInventory }) {
  const isEmpty =
    inventory.mcp_servers.length === 0 &&
    inventory.plugins.length === 0 &&
    inventory.hooks.length === 0;

  if (isEmpty && inventory.warnings.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center text-sm text-fog">
        Sin integraciones detectadas.
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto px-6 pt-2 pb-6 space-y-1.5">
      {inventory.mcp_servers.length > 0 && (
        <>
          <SectionTitle className="pt-1">Servidores MCP</SectionTitle>
          {inventory.mcp_servers.map((server) => (
            <Row
              key={`${server.tool}-${server.name}`}
              icon={server.name}
              title={server.name}
              detail={server.target}
              meta={<ToolBadge tool={server.tool} disabled={!server.enabled} />}
              muted={!server.enabled}
            />
          ))}
        </>
      )}
      {inventory.plugins.length > 0 && (
        <>
          <SectionTitle className={inventory.mcp_servers.length === 0 ? "pt-1" : undefined}>
            Plugins
          </SectionTitle>
          {inventory.plugins.map((plugin) => (
            <Row
              key={`${plugin.tool}-${plugin.name}`}
              icon={plugin.name.split("@")[0]}
              title={plugin.name.split("@")[0]}
              detail={plugin.version ? `v${plugin.version}` : ""}
              meta={<ToolBadge tool={plugin.tool} disabled={!plugin.enabled} />}
              muted={!plugin.enabled}
            />
          ))}
        </>
      )}
      {inventory.hooks.length > 0 && (
        <>
          <SectionTitle>Hooks</SectionTitle>
          {inventory.hooks.map((hook) => (
            <Row
              key={`${hook.tool}-${hook.event}-${hook.command}`}
              icon={hook.tool}
              title={hook.event}
              detail={hook.command}
              meta={<ToolBadge tool={hook.tool} />}
            />
          ))}
        </>
      )}
      {inventory.warnings.map((warning) => (
        <p key={warning} className="text-xs text-status-amber pt-2 break-words">
          {warning}
        </p>
      ))}
    </div>
  );
}

export function GlobalView({ refreshKey, pendingRequests, onSecretsChanged }: GlobalViewProps) {
  const [tab, setTab] = useState<GlobalTab>("memory");
  const [inventory, setInventory] = useState<AiInventory>(EMPTY);
  const [focusMemoryId, setFocusMemoryId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const loadInventory = useCallback(async () => {
    try {
      setInventory(await api.globalAiInventory());
    } catch (err) {
      setError(formatError(err));
    }
  }, []);

  useEffect(() => {
    loadInventory();
  }, [loadInventory]);

  const tabs: { id: GlobalTab; label: string; icon: typeof Brain; count?: number }[] = [
    { id: "memory", label: "Memoria", icon: Brain },
    { id: "instructions", label: "Instrucciones", icon: FileText },
    { id: "skills", label: "Skills", icon: Sparkles, count: inventory.skills.length },
    {
      id: "connections",
      label: "MCP y plugins",
      icon: Plug,
      count: inventory.mcp_servers.length + inventory.plugins.length,
    },
    { id: "secrets", label: "Claves", icon: KeyRound, count: pendingRequests },
  ];

  return (
    <>
      <div className="flex items-center gap-1 px-6 h-12 flex-shrink-0" role="tablist">
        {tabs.map(({ id, label, icon: Icon, count }) => (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={tab === id}
            onClick={() => setTab(id)}
            className={cn(
              "h-9 px-3 rounded-md text-sm font-medium flex items-center gap-2 transition-colors duration-[120ms] focus-visible:ring-2 focus-visible:ring-accent/40 focus:outline-none",
              tab === id ? "bg-elevated text-paper" : "text-fog hover:text-mist hover:bg-obsidian",
            )}
          >
            <Icon className="w-4 h-4" />
            <span>{label}</span>
            {count !== undefined && count > 0 && (
              <span
                className={cn(
                  "text-xs tabular-nums",
                  id === "secrets" ? "text-accent" : "text-fog",
                )}
              >
                {count}
              </span>
            )}
          </button>
        ))}
      </div>
      {error && <p className="px-6 text-sm text-coral-red">{error}</p>}
      <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
        {tab === "memory" && (
          <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
            <MemoryUsagePanel refreshKey={refreshKey} onSelectMemory={setFocusMemoryId} />
            <MemoryTab project={null} refreshKey={refreshKey} focusId={focusMemoryId} />
          </div>
        )}
        {tab === "instructions" && <InstructionsPanel inventory={inventory} />}
        {tab === "skills" && <SkillsPanel inventory={inventory} />}
        {tab === "connections" && <ConnectionsPanel inventory={inventory} />}
        {tab === "secrets" && <SecretsPanel onChanged={onSecretsChanged} />}
      </div>
    </>
  );
}
