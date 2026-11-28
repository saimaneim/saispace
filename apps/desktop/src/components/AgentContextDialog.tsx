import { AlertTriangle, Check, Copy, Loader2 } from "lucide-react";
import { useEffect, useState } from "react";
import { api, formatError } from "../lib/api";
import type { AgentContext } from "../lib/types";
import { Button } from "./ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "./ui/dialog";

export interface AgentContextDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  projectId?: string | null;
}

export function AgentContextDialog({ open, onOpenChange, projectId }: AgentContextDialogProps) {
  const [loading, setLoading] = useState(false);
  const [context, setContext] = useState<AgentContext | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!open) {
      setCopied(false);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setError(null);

    api
      .agentMemoryContext(projectId)
      .then((data) => {
        if (!cancelled) {
          setContext(data);
          setLoading(false);
        }
      })
      .catch((err) => {
        if (!cancelled) {
          setError(formatError(err));
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [open, projectId]);

  const handleCopy = async () => {
    if (!context?.text) return;
    try {
      await navigator.clipboard.writeText(context.text);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      return;
    }
  };

  const sizeKb = context ? (context.bytes / 1024).toFixed(1) : "0.0";
  const isNearBudget = Boolean(context && context.bytes > 6000);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="bg-elevated max-w-2xl p-6">
        <DialogHeader>
          <DialogTitle className="text-paper text-base font-semibold">
            Contexto del agente
          </DialogTitle>
          <DialogDescription className="sr-only">
            Contexto de memoria serializado que recibe el agente al iniciar sesión.
          </DialogDescription>
        </DialogHeader>

        {loading && (
          <div className="text-fog flex min-h-48 items-center justify-center gap-2 py-12 text-sm">
            <Loader2 className="text-accent h-4 w-4 animate-spin motion-reduce:animate-none" />
            <span>Cargando contexto…</span>
          </div>
        )}

        {error && (
          <div className="bg-coral-red/10 text-coral-red rounded-md p-3 text-sm" role="alert">
            {error}
          </div>
        )}

        {!loading && !error && context && (
          <div className="relative min-w-0">
            <pre className="text-paper bg-carbon max-h-[60vh] overflow-y-auto overscroll-contain rounded-lg p-4 font-mono text-xs leading-relaxed break-words whitespace-pre-wrap select-text">
              {context.text}
            </pre>
          </div>
        )}

        <div className="flex items-center justify-between pt-3">
          <div className="flex min-w-0 items-center gap-3">
            <span className="text-fog text-xs tabular-nums">{sizeKb} KB</span>
            {isNearBudget && (
              <span className="text-status-amber flex min-w-0 items-center gap-1 truncate text-xs">
                <AlertTriangle className="h-3.5 w-3.5 flex-shrink-0" />
                <span className="truncate">Cerca del límite</span>
              </span>
            )}
          </div>
          <Button
            variant="secondary"
            size="sm"
            onClick={handleCopy}
            disabled={!context || loading}
            className="flex flex-shrink-0 items-center gap-1.5 text-xs"
          >
            {copied ? (
              <>
                <Check className="text-pulse-green h-3.5 w-3.5" />
                <span>Copiado</span>
              </>
            ) : (
              <>
                <Copy className="h-3.5 w-3.5" />
                <span>Copiar</span>
              </>
            )}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
