import { Boxes, Brain, Check, FileText, History, ListTodo, Pipette, RotateCcw } from "lucide-react";
import { useEffect, useState } from "react";
import { ACCENTS, DEFAULT_ACCENT, readAccent, saveAccent } from "../lib/accent";
import { type DefaultTabId, readDefaultTab, saveDefaultTab } from "../lib/preferences";
import { cn } from "../lib/utils";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "./ui/dialog";

const TAB_OPTIONS: { id: DefaultTabId; label: string; icon: typeof ListTodo }[] = [
  { id: "context", label: "Contexto", icon: FileText },
  { id: "plan", label: "Plan", icon: ListTodo },
  { id: "history", label: "Historial", icon: History },
  { id: "tech", label: "Tecnologías", icon: Boxes },
  { id: "memory", label: "Memoria", icon: Brain },
];

export function SettingsDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const [accent, setAccent] = useState(readAccent);
  const [defaultTab, setDefaultTab] = useState<DefaultTabId>(readDefaultTab);

  useEffect(() => {
    if (open) {
      setAccent(readAccent());
      setDefaultTab(readDefaultTab());
    }
  }, [open]);

  const pickAccent = (value: string) => {
    const normalized = value.toLowerCase();
    setAccent(normalized);
    saveAccent(normalized);
  };

  const pickTab = (tab: DefaultTabId) => {
    setDefaultTab(tab);
    saveDefaultTab(tab);
  };

  const isCustom = !ACCENTS.some((a) => a.value.toLowerCase() === accent.toLowerCase());
  const isDefault = accent.toLowerCase() === DEFAULT_ACCENT.toLowerCase();

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-sm space-y-5">
        <DialogHeader className="mb-0">
          <DialogTitle>Configuración</DialogTitle>
          <DialogDescription className="sr-only">
            Preferencias de color de acento y pestaña inicial.
          </DialogDescription>
        </DialogHeader>

        <section aria-label="Color de acento" className="space-y-3">
          <div className="flex items-center justify-between gap-2">
            <span className="text-fog text-xs font-semibold">Acento</span>
            <div className="flex items-center gap-1.5">
              <span className="text-fog font-mono text-xs">{accent}</span>
              {!isDefault && (
                <button
                  type="button"
                  onClick={() => pickAccent(DEFAULT_ACCENT)}
                  title="Restablecer azul"
                  aria-label="Restablecer color predeterminado"
                  className="text-fog hover:text-paper hover:bg-obsidian focus-visible:ring-accent/40 flex h-6 w-6 items-center justify-center rounded-md transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2"
                >
                  <RotateCcw className="h-3.5 w-3.5" />
                </button>
              )}
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            {ACCENTS.map((a) => {
              const selected = accent.toLowerCase() === a.value.toLowerCase();
              return (
                <button
                  key={a.id}
                  type="button"
                  title={a.label}
                  aria-label={a.label}
                  aria-pressed={selected}
                  onClick={() => pickAccent(a.value)}
                  style={{ backgroundColor: a.value }}
                  className={cn(
                    "focus-visible:ring-paper/70 relative h-8 w-8 rounded-full transition-transform duration-[120ms] hover:scale-110 focus:outline-none focus-visible:ring-2",
                    selected && "ring-paper/80 ring-offset-elevated ring-2 ring-offset-2",
                  )}
                >
                  {selected && (
                    <Check className="text-accent-ink absolute inset-0 m-auto h-4 w-4" />
                  )}
                </button>
              );
            })}

            <label
              title="Color personalizado"
              className={cn(
                "bg-obsidian ring-smoke text-fog hover:text-paper focus-within:ring-paper/70 relative flex h-8 w-8 cursor-pointer items-center justify-center overflow-hidden rounded-full ring-1 transition-colors duration-[120ms] focus-within:ring-2",
                isCustom && "ring-paper/80 ring-offset-elevated text-paper ring-2 ring-offset-2",
              )}
              style={isCustom ? { backgroundColor: accent } : undefined}
            >
              <Pipette
                className={cn("pointer-events-none h-3.5 w-3.5", isCustom && "text-accent-ink")}
              />
              <input
                type="color"
                aria-label="Color personalizado"
                value={accent}
                onChange={(e) => pickAccent(e.target.value)}
                className="absolute inset-0 h-full w-full cursor-pointer opacity-0"
              />
            </label>
          </div>
        </section>

        <section aria-label="Pestaña inicial" className="space-y-2.5">
          <span className="text-fog block text-xs font-semibold">Pestaña inicial</span>
          <div className="bg-obsidian grid grid-cols-5 gap-1 rounded-lg p-1">
            {TAB_OPTIONS.map(({ id, label, icon: Icon }) => {
              const selected = defaultTab === id;
              return (
                <button
                  key={id}
                  type="button"
                  onClick={() => pickTab(id)}
                  title={label}
                  aria-label={label}
                  aria-pressed={selected}
                  className={cn(
                    "focus-visible:ring-accent/40 flex h-8 items-center justify-center rounded-md transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2",
                    selected
                      ? "bg-elevated text-accent"
                      : "text-fog hover:text-paper hover:bg-carbon",
                  )}
                >
                  <Icon className="h-4 w-4" />
                </button>
              );
            })}
          </div>
        </section>
      </DialogContent>
    </Dialog>
  );
}
