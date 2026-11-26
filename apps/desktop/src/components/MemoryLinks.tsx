import { Archive, Link2 } from "lucide-react";
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

/** Connections of a memory: what agents see next to it in `get_memory`. */
export function MemoryLinks({ memory, onOpen }: MemoryLinksProps) {
  const [links, setLinks] = useState<MemoryLink[]>([]);

  useEffect(() => {
    let alive = true;
    api
      .memoryLinks(memory.id)
      .then((list) => {
        if (alive) setLinks(list);
      })
      .catch(() => {
        if (alive) setLinks([]);
      });
    return () => {
      alive = false;
    };
  }, [memory.id]);

  if (links.length === 0 && !memory.superseded_by) return null;

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
