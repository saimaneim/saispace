import { Eye, History, Trash2, X } from "lucide-react";
import { useState } from "react";
import { formatError } from "../lib/api";
import type { Memory, StaleReport } from "../lib/types";
import { formatTimeAgo } from "../lib/utils";
import { KIND_LABELS } from "./MemoryTab";
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
  return (
    <div className="bg-carbon rounded-lg px-4 py-3 flex items-center justify-between gap-3">
      <div className="min-w-0 flex-1">
        <h3 className="text-sm font-semibold text-paper truncate">{memory.title}</h3>
        <p className="text-xs text-mist truncate">{preview(memory.content)}</p>
        <p className="text-xs text-fog">
          {KIND_LABELS[memory.kind]} · guardada {formatTimeAgo(memory.updated_at)} · último uso:{" "}
          {memory.last_used_at ? formatTimeAgo(memory.last_used_at) : "nunca"}
        </p>
      </div>
      <div className="flex items-center gap-2 flex-shrink-0">
        <Button variant="secondary" size="sm" onClick={onOpen}>
          <Eye className="w-3.5 h-3.5 mr-1" />
          Abrir
        </Button>
        <Button variant="destructive" size="sm" onClick={onDelete}>
          <Trash2 className="w-3.5 h-3.5 mr-1" />
          Eliminar
        </Button>
      </div>
    </div>
  );
}

export function StalePanel({ report, onOpenMemory, onDeleteMemory, onClose }: StalePanelProps) {
  const [pendingDelete, setPendingDelete] = useState<Memory | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);

  // Old session summaries are not listed: only the latest reaches the agent at
  // session start, so the rest cost nothing and stay searchable as history.
  const total = report.stale.length;

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
    <section className="flex-1 min-w-0 flex flex-col px-6 pb-6">
      <div className="h-14 flex items-center justify-between gap-3 flex-shrink-0">
        <div>
          <h2 className="text-base font-semibold text-paper">Memorias para revisar</h2>
          <p className="text-xs text-fog">
            Sin abrir ni actualizar en 60 días: ningún agente las está usando. Bórralas si ya no
            valen o fíjalas para conservarlas.
          </p>
        </div>
        <Button variant="secondary" onClick={onClose}>
          <X className="w-4 h-4 mr-2" />
          Cerrar
        </Button>
      </div>

      {error && (
        <div
          className="mb-2 px-3 py-2 rounded-md bg-coral-red/10 text-coral-red text-sm"
          role="alert"
        >
          {error}
        </div>
      )}

      {total === 0 ? (
        <div className="flex-1 flex flex-col items-center justify-center gap-2 text-center bg-carbon rounded-lg p-6">
          <History className="w-8 h-8 text-ash" />
          <p className="text-sm text-paper">Sin memorias para revisar</p>
          <p className="text-sm text-fog max-w-sm">
            Todas las memorias se han usado o actualizado en los últimos 60 días.
          </p>
          <Button variant="secondary" onClick={onClose} className="mt-2">
            Volver a memorias
          </Button>
        </div>
      ) : (
        <div className="flex-1 overflow-y-auto space-y-4 pr-1">
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
