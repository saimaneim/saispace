import { Brain, FileText, Globe, KeyRound, Plug, Search, Wand2, X } from "lucide-react";
import type React from "react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { api, formatError } from "../lib/api";
import type { AiInventory } from "../lib/types";
import { cn } from "../lib/utils";
import { Markdown } from "./Markdown";
import { MemoryTab } from "./MemoryTab";
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

/** What a hook does, in words: the command itself stays in the tooltip-free detail. */
function hookPurpose(command: string, event: string): string {
  if (command.includes("sai memory context")) return "Carga la memoria al empezar";
  if (command.includes("sai memory nudge")) {
    if (event === "UserPromptSubmit") return "Detecta cuando corriges al agente";
    if (event === "Stop") return "Pide guardar lo aprendido al terminar";
    if (event.startsWith("PostToolUse")) return "Recuerda la solución si un error se repite";
  }
  return command;
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

/** One row per hook purpose: the same job on several events shows once. */
function groupHooks(hooks: AiInventory["hooks"]) {
  const map = new Map<
    string,
    { tool: string; command: string; purpose: string; events: string[] }
  >();
  for (const hook of hooks) {
    const purpose = hookPurpose(hook.command, hook.event);
    const key = `${hook.tool}\u0000${hook.command}\u0000${purpose}`;
    const entry = map.get(key) ?? { tool: hook.tool, command: hook.command, purpose, events: [] };
    if (!entry.events.includes(hook.event)) entry.events.push(hook.event);
    map.set(key, entry);
  }
  return [...map.values()];
}

interface Merged {
  name: string;
  detail: string;
  tools: { tool: string; enabled: boolean }[];
}

/** One row per name; the agents that use it become badges. */
function mergeByName(items: { name: string; detail: string; tool: string; enabled: boolean }[]) {
  const map = new Map<string, Merged>();
  for (const item of items) {
    const entry = map.get(item.name) ?? { name: item.name, detail: item.detail, tools: [] };
    if (!entry.tools.some((t) => t.tool === item.tool)) {
      entry.tools.push({ tool: item.tool, enabled: item.enabled });
    }
    map.set(item.name, entry);
  }
  return [...map.values()].sort(
    (a, b) => b.tools.length - a.tools.length || a.name.localeCompare(b.name),
  );
}

function ConnectionsPanel({ inventory }: { inventory: AiInventory }) {
  const [agent, setAgent] = useState<string | null>(null);
  const servers = inventory.mcp_servers.map((s) => ({
    name: s.name,
    detail: s.target,
    tool: s.tool,
    enabled: s.enabled,
  }));
  const plugins = inventory.plugins.map((p) => ({
    name: p.name.split("@")[0],
    detail: p.version ? `v${p.version}` : "",
    tool: p.tool,
    enabled: p.enabled,
  }));
  const agents = [
    ...new Set([...servers, ...plugins, ...inventory.hooks].map((i) => i.tool)),
  ].sort();
  const mine = <T extends { tool: string }>(list: T[]) =>
    agent ? list.filter((i) => i.tool === agent) : list;
  const shownServers = mergeByName(mine(servers));
  const shownPlugins = mergeByName(mine(plugins));
  const shownHooks = groupHooks(mine(inventory.hooks));

  if (agents.length === 0 && inventory.warnings.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center text-sm text-fog">
        Sin integraciones detectadas.
      </div>
    );
  }

  const chip = (id: string | null, label: string, count: number) => (
    <button
      key={label}
      type="button"
      onClick={() => setAgent(id)}
      aria-pressed={agent === id}
      className={cn(
        "h-8 px-2.5 rounded-md flex items-center gap-1.5 text-sm transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40",
        agent === id ? "bg-elevated text-paper" : "text-fog hover:text-paper hover:bg-obsidian",
      )}
    >
      {id ? <TechIcon name={id} className="w-4 h-4" /> : <Globe className="w-4 h-4" />}
      <span>{label}</span>
      <span className="text-xs text-ash tabular-nums">{count}</span>
    </button>
  );
  const countFor = (tool: string) => [...servers, ...plugins].filter((i) => i.tool === tool).length;

  const renderMerged = (list: Merged[], iconOf: (m: Merged) => string) =>
    list.map((m) => {
      const off = m.tools.every((t) => !t.enabled);
      return (
        <Row
          key={m.name}
          icon={iconOf(m)}
          title={m.name}
          detail={m.detail}
          muted={off}
          meta={
            !agent && (
              <span className="flex items-center gap-1.5 flex-shrink-0">
                {m.tools.map((t) => (
                  <ToolBadge key={t.tool} tool={t.tool} disabled={!t.enabled && !off} />
                ))}
              </span>
            )
          }
        />
      );
    });

  return (
    <div className="flex-1 overflow-y-auto px-6 pt-2 pb-6 space-y-1.5">
      <div className="flex flex-wrap items-center gap-1 pb-1">
        {chip(null, "Global", mergeByName([...servers, ...plugins]).length)}
        {agents.map((tool) => chip(tool, tool, countFor(tool)))}
      </div>
      {shownServers.length > 0 && (
        <>
          <SectionTitle className="pt-1">Servidores MCP</SectionTitle>
          {renderMerged(shownServers, (m) => m.name)}
        </>
      )}
      {shownPlugins.length > 0 && (
        <>
          <SectionTitle>Plugins</SectionTitle>
          {renderMerged(shownPlugins, (m) => m.name)}
        </>
      )}
      {shownHooks.length > 0 && (
        <>
          <SectionTitle>Hooks</SectionTitle>
          {shownHooks.map((hook) => (
            <div
              key={`${hook.tool}-${hook.command}-${hook.purpose}`}
              title={hook.command}
              className="flex items-center gap-3 px-3 min-h-10 py-2 rounded-md bg-carbon min-w-0"
            >
              <TechIcon name={hook.tool} className="w-5 h-5" />
              <span className="text-sm text-paper flex-1 min-w-0 break-words">{hook.purpose}</span>
              <span className="flex flex-wrap justify-end gap-1 flex-shrink-0">
                {hook.events.map((event) => (
                  <span
                    key={event}
                    className="h-5 px-1.5 rounded bg-obsidian text-[11px] text-fog font-mono flex items-center"
                  >
                    {event}
                  </span>
                ))}
              </span>
            </div>
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
    { id: "skills", label: "Skills", icon: Wand2, count: inventory.skills.length },
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
            <MemoryTab project={null} refreshKey={refreshKey} />
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
