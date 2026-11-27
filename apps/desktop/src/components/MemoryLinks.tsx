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
          className="w-full flex items-center justify-between gap-2 px-4 h-10 rounded-lg bg-status-amber/10 text-xs font-medium text-status-amber hover:bg-status-amber/15 transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40 min-w-0"
        >
          <span className="flex items-center gap-2 min-w-0">
            <Archive className="w-3.5 h-3.5 flex-shrink-0" />
            <span className="truncate">
              Archivada: se consolidó en otra memoria. Abrir la vigente
            </span>
          </span>
          <ArrowUpRight className="w-3.5 h-3.5 flex-shrink-0" />
        </button>
      )}

      {triggers.length > 0 && (
        <div className="bg-carbon rounded-xl p-4 min-w-0 space-y-2.5">
          <p
            className="flex items-center gap-1.5 text-xs font-semibold text-fog tabular-nums"
            title="El agente recibe esta memoria sola cuando un comando muestra este texto, o al empezar en ese entorno"
          >
            <Zap className="w-3.5 h-3.5 text-acid-lime flex-shrink-0" />
            <span>Vuelve sola cuando aparece · {triggers.length}</span>
          </p>
          <ul className="flex flex-wrap gap-1.5">
            {triggers.map((pattern) => (
              <li
                key={pattern}
                className="max-w-full px-2.5 py-1 rounded-md bg-obsidian text-xs text-mist font-mono break-all select-text"
              >
                {triggerLabel(pattern)}
              </li>
            ))}
          </ul>
        </div>
      )}

      {links.length > 0 && (
        <div className="bg-carbon rounded-xl p-4 min-w-0 space-y-2">
          <p className="flex items-center gap-1.5 text-xs font-semibold text-fog tabular-nums">
            <Link2 className="w-3.5 h-3.5 flex-shrink-0" />
            <span>Enlaces · {links.length}</span>
          </p>
          <ul className="space-y-0.5">
            {links.map((link) => (
              <li key={`${link.id}:${link.relation}:${link.direction}`}>
                <button
                  type="button"
                  onClick={() => onOpen(link.id)}
                  className="w-full flex items-center gap-2.5 px-2.5 h-9 rounded-md text-left text-sm text-mist hover:text-paper hover:bg-obsidian transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40 min-w-0"
                >
                  <span className="text-xs text-fog w-24 flex-shrink-0 truncate">
                    {RELATION_LABEL[`${link.relation}:${link.direction}`]}
                  </span>
                  <span className="truncate flex-1 min-w-0 text-bone">{link.title}</span>
                  <div className="flex items-center gap-1.5 flex-shrink-0">
                    {link.archived && <span className="text-[11px] text-ash">archivada</span>}
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
