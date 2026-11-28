import { Archive, ArrowUpRight, Link2, Zap } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { Memory, MemoryLink } from "../lib/types";
import { MemoryKindBadge } from "./MemoryKindBadge";

const RELATION_LABEL: Record<string, string> = {
  "related:out": "Relacionada",
  "related:in": "Relacionada",
  "depends_on:out": "Depende de",
  "depends_on:in": "La necesita",
  "supersedes:out": "Absorbió",
  "supersedes:in": "Absorbida por",
};

interface MemoryLinksProps {
  memory: Memory;
  onOpen: (id: string) => void;
}

const HOST_LABEL: Record<string, string> = {
  t3code: "T3 Code",
  "claude-code": "Claude Code",
  opencode: "OpenCode",
  codex: "Codex",
  gemini: "Gemini CLI",
};

function triggerLabel(pattern: string): string {
  if (!pattern.startsWith("env:")) return pattern;
  const host = pattern.slice(4);
  return `Entorno: ${HOST_LABEL[host] ?? host}`;
}

export function MemoryLinks({ memory, onOpen }: MemoryLinksProps) {
  const [links, setLinks] = useState<MemoryLink[]>([]);
  const [triggers, setTriggers] = useState<string[]>([]);

  useEffect(() => {
    let alive = true;
    setLinks([]);
    setTriggers([]);
    api
      .memoryLinks(memory.id)
      .then((list) => {
        if (alive) setLinks(list);
      })
      .catch(() => {
        if (alive) setLinks([]);
      });
    api
      .memoryTriggers(memory.id)
      .then((list) => {
        if (alive) setTriggers(list);
      })
      .catch(() => {
        if (alive) setTriggers([]);
      });
    return () => {
      alive = false;
    };
  }, [memory.id]);

  if (links.length === 0 && triggers.length === 0 && !memory.superseded_by) return null;

  return (
    <div className="space-y-3">
      {memory.superseded_by && (
        <button
          type="button"
          onClick={() => memory.superseded_by && onOpen(memory.superseded_by)}
          className="bg-status-amber/10 text-status-amber hover:bg-status-amber/15 focus-visible:ring-accent/40 flex h-10 w-full min-w-0 items-center justify-between gap-2 rounded-lg px-4 text-xs font-medium transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2"
        >
          <span className="flex min-w-0 items-center gap-2">
            <Archive className="h-3.5 w-3.5 flex-shrink-0" />
            <span className="truncate">Consolidada · abrir vigente</span>
          </span>
          <ArrowUpRight className="h-3.5 w-3.5 flex-shrink-0" />
        </button>
      )}

      {triggers.length > 0 && (
        <div className="border-obsidian min-w-0 space-y-2.5 border-t pt-4">
          <p
            className="text-fog flex items-center gap-1.5 text-xs font-semibold tabular-nums"
            title="El agente recibe esta memoria sola cuando un comando muestra este texto, o al empezar en ese entorno"
          >
            <Zap className="text-accent h-3.5 w-3.5 flex-shrink-0" />
            <span>Activadores · {triggers.length}</span>
          </p>
          <ul className="flex flex-wrap gap-1.5">
            {triggers.map((pattern) => (
              <li
                key={pattern}
                className="bg-obsidian text-mist max-w-full rounded-md px-2.5 py-1 font-mono text-xs break-all select-text"
              >
                {triggerLabel(pattern)}
              </li>
            ))}
          </ul>
        </div>
      )}

      {links.length > 0 && (
        <div className="border-obsidian min-w-0 space-y-2 border-t pt-4">
          <p className="text-fog flex items-center gap-1.5 text-xs font-semibold tabular-nums">
            <Link2 className="h-3.5 w-3.5 flex-shrink-0" />
            <span>Enlaces · {links.length}</span>
          </p>
          <ul className="space-y-0.5">
            {links.map((link) => (
              <li key={`${link.id}:${link.relation}:${link.direction}`}>
                <button
                  type="button"
                  onClick={() => onOpen(link.id)}
                  className="text-mist hover:text-paper hover:bg-obsidian focus-visible:ring-accent/40 flex h-9 w-full min-w-0 items-center gap-2.5 rounded-md px-2.5 text-left text-sm transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2"
                >
                  <span className="text-fog w-24 flex-shrink-0 truncate text-xs">
                    {RELATION_LABEL[`${link.relation}:${link.direction}`]}
                  </span>
                  <span className="text-bone min-w-0 flex-1 truncate">{link.title}</span>
                  <div className="flex flex-shrink-0 items-center gap-1.5">
                    {link.archived && <span className="text-ash text-[11px]">archivada</span>}
                    <MemoryKindBadge kind={link.kind} compact />
                  </div>
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
