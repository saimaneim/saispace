import { Eye, History, Trash2, X } from "lucide-react";
import { useState } from "react";
import { formatError } from "../lib/api";
import type { Memory, StaleReport } from "../lib/types";
import { formatTimeAgo } from "../lib/utils";
import { MemoryKindBadge } from "./MemoryKindBadge";
import { Button } from "./ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./ui/dialog";

export interface StalePanelProps {
  report: StaleReport;
  onOpenMemory: (id: string) => void;
  onDeleteMemory: (id: string) => Promise<void>;
  onClose: () => void;
}

const preview = (content: string) =>
  content
    .replace(/[#*_`>]/g, "")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, 160);

function StaleItem({
  memory,
  onOpen,
  onDelete,
}: {
  memory: Memory;
  onOpen: () => void;
  onDelete: () => void;
}) {
  const snippet = preview(memory.content);
  return (
    <div className="bg-carbon hover:bg-obsidian/80 flex min-w-0 items-center justify-between gap-4 rounded-xl px-4 py-3.5 transition-colors duration-[120ms]">
      <div className="min-w-0 flex-1 space-y-1">
        <div className="flex min-w-0 items-center gap-2">
          <MemoryKindBadge kind={memory.kind} compact />
          <h3 className="text-paper flex-1 truncate text-sm font-semibold">{memory.title}</h3>
        </div>
        {snippet && <p className="text-mist truncate text-xs">{snippet}</p>}
        <p className="text-fog truncate text-xs tabular-nums">
          {formatTimeAgo(memory.updated_at)} ·{" "}
          {memory.last_used_at ? `usada ${formatTimeAgo(memory.last_used_at)}` : "sin uso"}
        </p>
      </div>
      <div className="flex flex-shrink-0 items-center gap-1">
        <Button
          variant="ghost"
          size="icon"
          onClick={onOpen}
          aria-label="Abrir"
          title="Abrir"
          className="text-fog hover:text-paper hover:bg-obsidian h-8 w-8"
        >
          <Eye className="h-4 w-4" />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          onClick={onDelete}
          aria-label="Eliminar"
          title="Eliminar"
          className="text-fog hover:text-coral-red hover:bg-coral-red/10 h-8 w-8"
        >
          <Trash2 className="h-4 w-4" />
        </Button>
      </div>
    </div>
  );
}

export function StalePanel({ report, onOpenMemory, onDeleteMemory, onClose }: StalePanelProps) {
  const [pendingDelete, setPendingDelete] = useState<Memory | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);

  const total = report.stale.length + report.surplus_sessions.length;

  const confirmDelete = async () => {
    if (!pendingDelete) return;
    setIsDeleting(true);
    try {
      await onDeleteMemory(pendingDelete.id);
      setPendingDelete(null);
    } catch (err) {
      setError(formatError(err));
    } finally {
      setIsDeleting(false);
    }
  };

  return (
    <section className="flex min-w-0 flex-1 flex-col px-7 pt-5 pb-6">
      <div className="flex flex-shrink-0 items-center justify-between gap-3 pb-4">
        <div className="flex min-w-0 items-baseline gap-2.5">
          <h2 className="text-paper text-lg font-semibold tracking-tight">Para revisar</h2>
          <span className="text-fog text-xs tabular-nums">{total}</span>
        </div>
        <Button
          variant="ghost"
          size="icon"
          onClick={onClose}
          aria-label="Cerrar"
          title="Cerrar"
          className="text-fog hover:text-paper hover:bg-obsidian h-8 w-8"
        >
          <X className="h-4 w-4" />
        </Button>
      </div>

      {error && (
        <div
          className="bg-coral-red/10 text-coral-red mb-3 rounded-md px-3 py-2 text-sm"
          role="alert"
        >
          {error}
        </div>
      )}

      {total === 0 ? (
        <div className="bg-carbon flex flex-1 flex-col items-center justify-center gap-2.5 rounded-xl p-6 text-center">
          <History className="text-ash h-7 w-7" />
          <p className="text-paper text-sm font-medium">Nada para revisar</p>
          <Button variant="secondary" size="sm" onClick={onClose}>
            Volver
          </Button>
        </div>
      ) : (
        <div className="flex-1 space-y-4 overflow-y-auto pr-1">
          {report.stale.length > 0 && (
            <div className="space-y-2">
              {report.stale.map((m) => (
                <StaleItem
                  key={m.id}
                  memory={m}
                  onOpen={() => onOpenMemory(m.id)}
                  onDelete={() => setPendingDelete(m)}
                />
              ))}
            </div>
          )}
          {report.surplus_sessions.length > 0 && (
            <div className="space-y-2">
              <h3 className="text-fog px-1 text-xs font-semibold">
                Resúmenes antiguos · {report.surplus_sessions.length}
              </h3>
              {report.surplus_sessions.map((m) => (
                <StaleItem
                  key={m.id}
                  memory={m}
                  onOpen={() => onOpenMemory(m.id)}
                  onDelete={() => setPendingDelete(m)}
                />
              ))}
            </div>
          )}
        </div>
      )}

      <Dialog
        open={Boolean(pendingDelete)}
        onOpenChange={(open) => !open && setPendingDelete(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Eliminar memoria</DialogTitle>
            <DialogDescription>
              Los agentes dejarán de recibir «{pendingDelete?.title}». No se puede deshacer.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="secondary"
              onClick={() => setPendingDelete(null)}
              disabled={isDeleting}
            >
              Cancelar
            </Button>
            <Button variant="destructive" onClick={confirmDelete} disabled={isDeleting}>
              Eliminar
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </section>
  );
}
