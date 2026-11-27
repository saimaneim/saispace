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
            <span className="text-xs font-semibold text-fog">Acento</span>
            <div className="flex items-center gap-1.5">
              <span className="font-mono text-xs text-fog">{accent}</span>
              {!isDefault && (
                <button
                  type="button"
                  onClick={() => pickAccent(DEFAULT_ACCENT)}
                  title="Restablecer azul"
                  aria-label="Restablecer color predeterminado"
                  className="h-6 w-6 rounded-md flex items-center justify-center text-fog hover:text-paper hover:bg-obsidian transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40"
                >
                  <RotateCcw className="w-3.5 h-3.5" />
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
                    "relative h-8 w-8 rounded-full transition-transform duration-[120ms] hover:scale-110 focus:outline-none focus-visible:ring-2 focus-visible:ring-paper/70",
                    selected && "ring-2 ring-paper/80 ring-offset-2 ring-offset-elevated",
                  )}
                >
                  {selected && (
                    <Check className="absolute inset-0 m-auto h-4 w-4 text-accent-ink" />
                  )}
                </button>
              );
            })}

            <label
              title="Color personalizado"
              className={cn(
                "relative h-8 w-8 cursor-pointer overflow-hidden rounded-full bg-obsidian ring-1 ring-smoke flex items-center justify-center text-fog hover:text-paper transition-colors duration-[120ms] focus-within:ring-2 focus-within:ring-paper/70",
                isCustom && "ring-2 ring-paper/80 ring-offset-2 ring-offset-elevated text-paper",
              )}
              style={isCustom ? { backgroundColor: accent } : undefined}
            >
              <Pipette
                className={cn("h-3.5 w-3.5 pointer-events-none", isCustom && "text-accent-ink")}
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
          <span className="text-xs font-semibold text-fog block">Pestaña inicial</span>
          <div className="grid grid-cols-5 gap-1 bg-obsidian p-1 rounded-lg">
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
                    "h-8 rounded-md flex items-center justify-center transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40",
                    selected
                      ? "bg-elevated text-accent"
                      : "text-fog hover:text-paper hover:bg-carbon",
                  )}
                >
                  <Icon className="w-4 h-4" />
                </button>
              );
            })}
          </div>
        </section>
      </DialogContent>
    </Dialog>
  );
}
