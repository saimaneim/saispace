import { CopyCheck, Trash2, X } from "lucide-react";
import { useState } from "react";
import { formatError } from "../lib/api";
import type { DuplicatePair, Memory } from "../lib/types";
import { formatTimeAgo } from "../lib/utils";
import { Markdown } from "./Markdown";
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

export interface DuplicatesPanelProps {
  pairs: DuplicatePair[];
  onDeleteMemory: (id: string) => Promise<void>;
  onDismissPair: (pair: DuplicatePair) => void;
  onClose: () => void;
}

function DuplicateCard({ memory, onDelete }: { memory: Memory; onDelete: () => void }) {
  return (
    <div className="bg-obsidian rounded-lg p-3.5 flex flex-col gap-2.5 min-w-0">
      <div className="flex items-start justify-between gap-2 min-w-0">
        <div className="min-w-0 flex-1 space-y-1">
          <h3 className="text-sm font-semibold text-paper truncate">{memory.title}</h3>
          <div className="flex items-center gap-1.5 text-xs text-fog min-w-0">
            <MemoryKindBadge kind={memory.kind} compact />
            <span className="tabular-nums truncate">{formatTimeAgo(memory.updated_at)}</span>
          </div>
        </div>
        <Button
          variant="ghost"
          size="icon"
          onClick={onDelete}
          aria-label="Eliminar memoria"
          title="Eliminar esta memoria"
          className="h-7 w-7 text-fog hover:text-coral-red hover:bg-coral-red/10 flex-shrink-0"
        >
          <Trash2 className="w-3.5 h-3.5" />
        </Button>
      </div>
      <div className="h-36 min-w-0 overflow-y-auto overscroll-contain rounded-md bg-void/60 p-3 text-xs text-mist">
        {memory.content.trim() ? (
          <Markdown content={memory.content} />
        ) : (
          <span className="text-ash">Sin contenido</span>
        )}
      </div>
    </div>
  );
}

export function DuplicatesPanel({
  pairs,
  onDeleteMemory,
  onDismissPair,
  onClose,
}: DuplicatesPanelProps) {
  const [pendingDelete, setPendingDelete] = useState<Memory | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);

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
    <section className="flex-1 min-w-0 flex flex-col px-7 pt-5 pb-6">
      <div className="flex items-center justify-between gap-3 pb-4 flex-shrink-0">
        <div className="flex items-baseline gap-2.5 min-w-0">
          <h2 className="text-lg font-semibold text-paper tracking-tight">Duplicados</h2>
          <span className="text-xs text-fog tabular-nums">{pairs.length}</span>
        </div>
        <Button
          variant="ghost"
          size="icon"
          onClick={onClose}
          aria-label="Cerrar"
          title="Cerrar"
          className="h-8 w-8 text-fog hover:text-paper hover:bg-obsidian"
        >
          <X className="w-4 h-4" />
        </Button>
      </div>

      {error && (
        <div
          className="mb-3 px-3 py-2 rounded-md bg-coral-red/10 text-coral-red text-sm"
          role="alert"
        >
          {error}
        </div>
      )}

      {pairs.length === 0 ? (
        <div className="flex-1 flex flex-col items-center justify-center gap-2.5 text-center bg-carbon rounded-xl p-6">
          <CopyCheck className="w-7 h-7 text-ash" />
          <p className="text-sm font-medium text-paper">Sin duplicados</p>
          <Button variant="secondary" size="sm" onClick={onClose}>
            Volver
          </Button>
        </div>
      ) : (
        <div className="flex-1 overflow-y-auto space-y-4 pr-1">
          {pairs.map((pair) => (
            <div
              key={`${pair.a.id}:${pair.b.id}`}
              className="bg-carbon rounded-xl p-4 flex flex-col gap-3"
            >
              <div className="grid grid-cols-1 xl:grid-cols-[repeat(2,minmax(0,1fr))] gap-3">
                <DuplicateCard memory={pair.a} onDelete={() => setPendingDelete(pair.a)} />
                <DuplicateCard memory={pair.b} onDelete={() => setPendingDelete(pair.b)} />
              </div>

              <div className="flex justify-end">
                <Button variant="secondary" size="sm" onClick={() => onDismissPair(pair)}>
                  Descartar
                </Button>
              </div>
            </div>
          ))}
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
