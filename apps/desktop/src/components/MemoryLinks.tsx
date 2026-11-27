import { Archive, Link2, Zap } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { Memory, MemoryLink } from "../lib/types";
import { KIND_LABELS } from "./MemoryTab";

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

/** `env:t3code` reads as the host; anything else is the error text itself. */
function triggerLabel(pattern: string): string {
  if (!pattern.startsWith("env:")) return pattern;
  const host = pattern.slice(4);
  return `Entorno: ${HOST_LABEL[host] ?? host}`;
}

/** Connections of a memory: what agents see next to it in `get_memory`. */
export function MemoryLinks({ memory, onOpen }: MemoryLinksProps) {
  const [links, setLinks] = useState<MemoryLink[]>([]);
  const [triggers, setTriggers] = useState<string[]>([]);

  useEffect(() => {
    let alive = true;
    // Never show the previous memory's links while the new ones load.
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
    <div className="mt-3 space-y-2">
      {memory.superseded_by && (
        <button
          type="button"
          onClick={() => memory.superseded_by && onOpen(memory.superseded_by)}
          className="w-full flex items-center gap-2 px-3 h-9 rounded-md bg-obsidian text-xs text-mist hover:text-paper hover:bg-elevated transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
        >
          <Archive className="w-3.5 h-3.5 text-fog flex-shrink-0" />
          Archivada: se consolidó en otra memoria. Abrir la vigente
        </button>
      )}
      {triggers.length > 0 && (
        <div className="bg-carbon rounded-lg p-3 min-w-0">
          <p
            className="flex items-center gap-1.5 text-xs font-semibold text-fog mb-2"
            title="El agente recibe esta memoria sola cuando un comando muestra este texto, o al empezar en ese entorno"
          >
            <Zap className="w-3.5 h-3.5" />
            Vuelve sola cuando aparece · {triggers.length}
          </p>
          <ul className="flex flex-wrap gap-1.5">
            {triggers.map((pattern) => (
              <li
                key={pattern}
                className="max-w-full px-2 py-1 rounded-md bg-obsidian text-xs text-mist font-mono break-all"
              >
                {triggerLabel(pattern)}
              </li>
            ))}
          </ul>
        </div>
      )}
      {links.length > 0 && (
        <div className="bg-carbon rounded-lg p-3">
          <p className="flex items-center gap-1.5 text-xs font-semibold text-fog mb-2">
            <Link2 className="w-3.5 h-3.5" />
            Enlaces · {links.length}
          </p>
          <ul className="space-y-0.5">
            {links.map((link) => (
              <li key={`${link.id}:${link.relation}:${link.direction}`}>
                <button
                  type="button"
                  onClick={() => onOpen(link.id)}
                  className="w-full flex items-center gap-2 px-2 h-8 rounded-md text-left text-sm text-mist hover:text-paper hover:bg-obsidian transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
                >
                  <span className="text-xs text-fog w-24 flex-shrink-0">
                    {RELATION_LABEL[`${link.relation}:${link.direction}`]}
                  </span>
                  <span className="truncate flex-1">{link.title}</span>
                  <span className="text-xs text-ash flex-shrink-0">
                    {KIND_LABELS[link.kind]}
                    {link.archived ? " · archivada" : ""}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
