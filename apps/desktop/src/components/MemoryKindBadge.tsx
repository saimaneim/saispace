import type { MemoryKind } from "../lib/types";
import { cn } from "../lib/utils";

export const MEMORY_KIND_LABELS: Record<MemoryKind, string> = {
  decision: "Decisión",
  architecture: "Arquitectura",
  bugfix: "Corrección",
  pattern: "Patrón",
  config: "Configuración",
  discovery: "Hallazgo",
  preference: "Preferencia",
  session_summary: "Resumen",
  note: "Nota",
};

export const MEMORY_KIND_STYLE: Record<MemoryKind, { badge: string; dot: string }> = {
  decision: { badge: "bg-accent/12 text-accent", dot: "bg-accent" },
  architecture: { badge: "bg-iris/15 text-iris", dot: "bg-iris" },
  bugfix: { badge: "bg-coral-red/12 text-coral-red", dot: "bg-coral-red" },
  pattern: { badge: "bg-signal-teal/12 text-signal-teal", dot: "bg-signal-teal" },
  config: { badge: "bg-status-amber/12 text-status-amber", dot: "bg-status-amber" },
  discovery: { badge: "bg-pulse-green/12 text-pulse-green", dot: "bg-pulse-green" },
  preference: { badge: "bg-elevated text-bone", dot: "bg-bone" },
  session_summary: { badge: "bg-obsidian text-mist", dot: "bg-fog" },
  note: { badge: "bg-obsidian text-mist", dot: "bg-ash" },
};

interface MemoryKindBadgeProps {
  kind: MemoryKind;
  compact?: boolean;
  className?: string;
}

export function MemoryKindBadge({ kind, compact = false, className }: MemoryKindBadgeProps) {
  const style = MEMORY_KIND_STYLE[kind] ?? MEMORY_KIND_STYLE.note;
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-md font-medium flex-shrink-0",
        compact ? "px-1.5 py-0.5 text-[11px] leading-none" : "px-2 py-0.5 text-xs",
        style.badge,
        className,
      )}
    >
      <span
        className={cn("w-1.5 h-1.5 rounded-full flex-shrink-0", style.dot)}
        aria-hidden="true"
      />
      <span>{MEMORY_KIND_LABELS[kind]}</span>
    </span>
  );
}
