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
  focusMemoryId?: string | null;
  focusSeq?: number;
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
    <h3 className={cn("text-fog px-1 pt-5 pb-2 text-xs font-semibold tabular-nums", className)}>
      {children}
    </h3>
  );
}

function ToolBadge({ tool, disabled }: { tool: string; disabled?: boolean }) {
  return (
    <span
      className="flex flex-shrink-0 items-center gap-1.5"
      title={disabled ? `${tool} · desactivado` : tool}
    >
      {disabled && <span className="text-fog text-xs">desactivado</span>}
      <TechIcon name={tool} className="h-4 w-4" />
      <span className="sr-only">{tool}</span>
    </span>
  );
}

function InstructionsPanel({ inventory }: { inventory: AiInventory }) {
  const [selected, setSelected] = useState(0);
  const file = inventory.instructions[selected];
  if (!file) {
    return (
      <p className="text-fog flex flex-1 items-center justify-center text-sm">
        Sin instrucciones globales.
      </p>
    );
  }
  return (
    <div className="flex min-h-0 flex-1">
      <aside className="bg-carbon w-64 flex-shrink-0 space-y-0.5 overflow-y-auto p-3">
        {inventory.instructions.map((item, index) => {
          const isSelected = index === selected;
          return (
            <button
              key={item.path}
              type="button"
              onClick={() => setSelected(index)}
              aria-current={isSelected ? "true" : undefined}
              className={cn(
                "focus-visible:ring-accent/40 relative flex w-full items-center gap-2.5 rounded-md py-2 pr-2.5 pl-3 text-left transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2",
                isSelected ? "bg-elevated text-paper" : "text-mist hover:bg-obsidian",
              )}
            >
              <TechIcon name={item.tool} className="h-5 w-5" />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm font-medium">{item.tool}</span>
                <span className="text-fog block truncate font-mono text-xs">
                  {item.path.split("/").pop()}
                </span>
              </span>
            </button>
          );
        })}
      </aside>
      <div className="flex-1 overflow-y-auto px-7 py-5">
        <div className="max-w-[76ch] space-y-3">
          <p className="text-fog truncate font-mono text-xs select-text" title={file.path}>
            {file.path}
          </p>
          <div className="bg-carbon rounded-xl px-6 py-5">
            <Markdown content={file.content} />
          </div>
          {file.truncated && <p className="text-status-amber text-xs">Fichero recortado.</p>}
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
      <div className="relative mb-4 w-full max-w-sm">
        <Search className="text-fog pointer-events-none absolute top-1/2 left-3 h-3.5 w-3.5 -translate-y-1/2" />
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={`Filtrar ${inventory.skills.length} skills…`}
          aria-label="Filtrar skills"
          className="bg-obsidian text-paper placeholder:text-fog focus-visible:ring-accent/40 h-9 w-full rounded-md pr-8 pl-8 text-sm transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2"
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
      {groups.length === 0 ? (
        <p className="text-fog py-12 text-center text-sm">
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
                  className="bg-carbon hover:bg-obsidian/80 min-w-0 rounded-xl px-4 py-3.5 transition-colors duration-[120ms]"
                  title={skill.path}
                >
                  <p className="text-paper truncate text-sm font-medium">{skill.name}</p>
                  <p className="text-fog mt-1 line-clamp-2 text-xs leading-relaxed break-words">
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
        "bg-carbon hover:bg-obsidian/80 flex h-11 min-w-0 items-center gap-3 rounded-lg px-3.5 transition-colors duration-[120ms]",
        muted && "opacity-50",
      )}
    >
      <TechIcon name={icon} className="h-5 w-5" />
      <span className="text-paper w-44 flex-shrink-0 truncate text-sm font-medium">{title}</span>
      <span className="text-fog min-w-0 flex-1 truncate font-mono text-xs select-text">
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
      <div className="text-fog flex flex-1 items-center justify-center text-sm">
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
        "focus-visible:ring-accent/40 flex h-8 items-center gap-1.5 rounded-md px-2.5 text-sm transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2",
        agent === id ? "bg-elevated text-paper" : "text-fog hover:text-paper hover:bg-obsidian",
      )}
    >
      {id ? <TechIcon name={id} className="h-4 w-4" /> : <Globe className="h-4 w-4" />}
      <span>{label}</span>
      <span className="text-ash text-xs tabular-nums">{count}</span>
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
              <span className="flex flex-shrink-0 items-center gap-1.5">
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
    <div className="flex-1 space-y-1.5 overflow-y-auto px-6 pt-2 pb-6">
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
              className="bg-carbon flex min-h-10 min-w-0 items-center gap-3 rounded-md px-3 py-2"
            >
              <TechIcon name={hook.tool} className="h-5 w-5" />
              <span className="text-paper min-w-0 flex-1 text-sm break-words">{hook.purpose}</span>
              <span className="flex flex-shrink-0 flex-wrap justify-end gap-1">
                {hook.events.map((event) => (
                  <span
                    key={event}
                    className="bg-obsidian text-fog flex h-5 items-center rounded px-1.5 font-mono text-[11px]"
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
        <p key={warning} className="text-status-amber pt-2 text-xs break-words">
          {warning}
        </p>
      ))}
    </div>
  );
}

export function GlobalView({
  refreshKey,
  pendingRequests,
  onSecretsChanged,
  focusMemoryId,
  focusSeq,
}: GlobalViewProps) {
  const [tab, setTab] = useState<GlobalTab>("memory");

  useEffect(() => {
    void focusSeq;
    if (focusMemoryId) setTab("memory");
  }, [focusMemoryId, focusSeq]);
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
      <div className="flex h-12 flex-shrink-0 items-center gap-1 px-6" role="tablist">
        {tabs.map(({ id, label, icon: Icon, count }) => (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={tab === id}
            onClick={() => setTab(id)}
            className={cn(
              "focus-visible:ring-accent/40 flex h-9 items-center gap-2 rounded-md px-3 text-sm font-medium transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2",
              tab === id ? "bg-elevated text-paper" : "text-fog hover:text-mist hover:bg-obsidian",
            )}
          >
            <Icon className="h-4 w-4" />
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
      {error && <p className="text-coral-red px-6 text-sm">{error}</p>}
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
        {tab === "memory" && (
          <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
            <MemoryTab
              project={null}
              refreshKey={refreshKey}
              focusId={focusMemoryId}
              focusSeq={focusSeq}
            />
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
