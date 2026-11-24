import { CopyCheck, Trash2, X } from "lucide-react";
import { useState } from "react";
import { formatError } from "../lib/api";
import type { DuplicatePair, Memory } from "../lib/types";
import { formatTimeAgo } from "../lib/utils";
import { Markdown } from "./Markdown";
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

export interface DuplicatesPanelProps {
  pairs: DuplicatePair[];
  onDeleteMemory: (id: string) => Promise<void>;
  onDismissPair: (pair: DuplicatePair) => void;
  onClose: () => void;
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
    <section className="flex-1 min-w-0 flex flex-col px-6 pb-6">
      <div className="h-14 flex items-center justify-between gap-3 flex-shrink-0">
        <div>
          <h2 className="text-base font-semibold text-paper">Posibles duplicados</h2>
          <p className="text-xs text-fog">
            {pairs.length} {pairs.length === 1 ? "par detectado" : "pares detectados"}
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

      {pairs.length === 0 ? (
        <div className="flex-1 flex flex-col items-center justify-center gap-2 text-center bg-carbon rounded-lg p-6">
          <CopyCheck className="w-8 h-8 text-ash" />
          <p className="text-sm text-paper">Sin duplicados pendientes</p>
          <p className="text-sm text-fog max-w-sm">
            Todas las memorias duplicadas han sido resueltas o descartadas para esta sesión.
          </p>
          <Button variant="secondary" onClick={onClose} className="mt-2">
            Volver a memorias
          </Button>
        </div>
      ) : (
        <div className="flex-1 overflow-y-auto space-y-4 pr-1">
          {pairs.map((pair) => (
            <div
              key={`${pair.a.id}:${pair.b.id}`}
              className="bg-carbon border border-elevated rounded-lg p-4 flex flex-col gap-3"
            >
              <div className="grid grid-cols-2 gap-4">
                <div className="bg-obsidian rounded-md p-3 flex flex-col gap-2 min-w-0">
                  <div className="flex items-start justify-between gap-2">
                    <div className="min-w-0 flex-1">
                      <h3 className="text-sm font-semibold text-paper truncate">{pair.a.title}</h3>
                      <p className="text-xs text-fog">
                        {KIND_LABELS[pair.a.kind]} · {formatTimeAgo(pair.a.updated_at)}
                      </p>
                    </div>
                    <Button
                      variant="destructive"
                      size="sm"
                      onClick={() => setPendingDelete(pair.a)}
                    >
                      <Trash2 className="w-3.5 h-3.5 mr-1" />
                      Borrar esta
                    </Button>
                  </div>
                  <div className="h-28 overflow-hidden rounded bg-void/50 p-2.5 text-xs text-mist relative">
                    <Markdown content={pair.a.content} />
                    <div className="absolute inset-x-0 bottom-0 h-8 bg-gradient-to-t from-void to-transparent pointer-events-none" />
                  </div>
                </div>

                <div className="bg-obsidian rounded-md p-3 flex flex-col gap-2 min-w-0">
                  <div className="flex items-start justify-between gap-2">
                    <div className="min-w-0 flex-1">
                      <h3 className="text-sm font-semibold text-paper truncate">{pair.b.title}</h3>
                      <p className="text-xs text-fog">
                        {KIND_LABELS[pair.b.kind]} · {formatTimeAgo(pair.b.updated_at)}
                      </p>
                    </div>
                    <Button
                      variant="destructive"
                      size="sm"
                      onClick={() => setPendingDelete(pair.b)}
                    >
                      <Trash2 className="w-3.5 h-3.5 mr-1" />
                      Borrar esta
                    </Button>
                  </div>
                  <div className="h-28 overflow-hidden rounded bg-void/50 p-2.5 text-xs text-mist relative">
                    <Markdown content={pair.b.content} />
                    <div className="absolute inset-x-0 bottom-0 h-8 bg-gradient-to-t from-void to-transparent pointer-events-none" />
                  </div>
                </div>
              </div>

              <div className="flex justify-end pt-1">
                <Button variant="secondary" size="sm" onClick={() => onDismissPair(pair)}>
                  No son duplicados
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
