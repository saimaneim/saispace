import { type FormEvent, useEffect, useState } from "react";
import { api, formatError } from "../lib/api";
import type { Memory, MemoryKind } from "../lib/types";
import { cn } from "../lib/utils";
import { MEMORY_KIND_LABELS } from "./MemoryKindBadge";
import { Button } from "./ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./ui/dialog";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "./ui/select";

export interface MemoryEditorProps {
  memory: Memory | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSaved: (updated: Memory) => void;
}

const inputClass =
  "h-9 px-3 text-sm bg-obsidian text-paper placeholder:text-fog rounded-md focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40 transition-colors duration-[120ms]";

export function MemoryEditor({ memory, open, onOpenChange, onSaved }: MemoryEditorProps) {
  const [title, setTitle] = useState("");
  const [kind, setKind] = useState<MemoryKind>("note");
  const [topic, setTopic] = useState("");
  const [content, setContent] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    if (memory) {
      setTitle(memory.title);
      setKind(memory.kind);
      setTopic(memory.topic_key ?? "");
      setContent(memory.content);
      setError(null);
    }
  }, [memory]);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!memory) return;
    setIsSaving(true);
    setError(null);
    try {
      const updated = await api.updateMemory(memory.id, {
        title: title.trim(),
        kind,
        topic_key: topic.trim(),
        content,
      });
      onSaved(updated);
      onOpenChange(false);
    } catch (err) {
      setError(formatError(err));
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
        <form onSubmit={submit} className="space-y-4">
          <DialogHeader>
            <DialogTitle>Editar memoria</DialogTitle>
            <DialogDescription className="sr-only">
              Edita el título, tipo, tema y contenido de la memoria.
            </DialogDescription>
          </DialogHeader>

          <div className="flex flex-col gap-3">
            <div className="flex flex-col gap-1.5">
              <label htmlFor="memory-title" className="text-xs font-medium text-fog">
                Título
              </label>
              <input
                id="memory-title"
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                placeholder="Título de la memoria…"
                aria-label="Título"
                className={inputClass}
                required
              />
            </div>

            <div className="grid grid-cols-2 gap-3">
              <div className="flex flex-col gap-1.5">
                <label htmlFor="memory-kind" className="text-xs font-medium text-fog">
                  Tipo
                </label>
                <Select value={kind} onValueChange={(val) => setKind(val as MemoryKind)}>
                  <SelectTrigger id="memory-kind" aria-label="Tipo de memoria">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {(Object.keys(MEMORY_KIND_LABELS) as MemoryKind[]).map((k) => (
                      <SelectItem key={k} value={k}>
                        {MEMORY_KIND_LABELS[k]}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              <div className="flex flex-col gap-1.5">
                <label htmlFor="memory-topic" className="text-xs font-medium text-fog">
                  Tema (opcional)
                </label>
                <input
                  id="memory-topic"
                  value={topic}
                  onChange={(e) => setTopic(e.target.value)}
                  placeholder="ej. arch/auth"
                  aria-label="Tema"
                  spellCheck={false}
                  className={cn(inputClass, "font-mono")}
                />
              </div>
            </div>

            <div className="flex flex-col gap-1.5">
              <label htmlFor="memory-content" className="text-xs font-medium text-fog">
                Contenido
              </label>
              <textarea
                id="memory-content"
                value={content}
                onChange={(e) => setContent(e.target.value)}
                placeholder="Contenido en Markdown o What:/Why:/Where:/Learned:…"
                aria-label="Contenido"
                className="w-full h-60 max-h-[45vh] p-3.5 text-sm font-mono leading-relaxed bg-obsidian text-paper placeholder:text-fog rounded-md focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40 resize-y transition-colors duration-[120ms]"
                required
              />
            </div>
          </div>

          {error && (
            <p className="text-sm text-coral-red break-words" role="alert">
              {error}
            </p>
          )}

          <DialogFooter>
            <Button
              type="button"
              variant="secondary"
              onClick={() => onOpenChange(false)}
              disabled={isSaving}
            >
              Cancelar
            </Button>
            <Button type="submit" variant="default" disabled={isSaving}>
              {isSaving ? "Guardando…" : "Guardar"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
