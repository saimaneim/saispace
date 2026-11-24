import { AlertTriangle, Check, Copy, Loader2 } from "lucide-react";
import { useEffect, useState } from "react";
import { api, formatError } from "../lib/api";
import type { AgentContext } from "../lib/types";
import { Button } from "./ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "./ui/dialog";

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
      // Fallback si clipboard falla
    }
  };

  const sizeKb = context ? (context.bytes / 1024).toFixed(1) : "0.0";
  const isNearBudget = Boolean(context && context.bytes > 6000);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl w-full p-6 bg-elevated">
        <DialogHeader>
          <DialogTitle className="text-base font-semibold text-paper">
            Lo que ve el agente al empezar
          </DialogTitle>
        </DialogHeader>

        {loading && (
          <div className="flex items-center justify-center py-12 text-fog gap-2 text-sm">
            <Loader2 className="w-4 h-4 animate-spin text-acid-lime" />
            <span>Cargando contexto…</span>
          </div>
        )}

        {error && (
          <div className="p-3 bg-coral-red/10 text-coral-red rounded-md text-sm" role="alert">
            {error}
          </div>
        )}

        {!loading && !error && context && (
          <div className="relative mt-2">
            <pre className="font-mono text-xs text-paper bg-carbon p-4 rounded-lg overflow-x-auto overflow-y-auto max-h-[60vh] whitespace-pre select-text leading-relaxed">
              {context.text}
            </pre>
          </div>
        )}

        <div className="flex items-center justify-between mt-4 pt-3 border-t border-obsidian">
          <div className="flex items-center gap-3">
            <span className="text-xs text-fog">{sizeKb} KB</span>
            {isNearBudget && (
              <span className="text-xs text-status-amber flex items-center gap-1">
                <AlertTriangle className="w-3.5 h-3.5 flex-shrink-0" />
                Se acerca al límite que corta Claude Code
              </span>
            )}
          </div>
          <Button
            variant="secondary"
            size="sm"
            onClick={handleCopy}
            disabled={!context || loading}
            className="flex items-center gap-1.5 text-xs"
          >
            {copied ? (
              <>
                <Check className="w-3.5 h-3.5 text-pulse-green" />
                <span>Copiado</span>
              </>
            ) : (
              <>
                <Copy className="w-3.5 h-3.5" />
                <span>Copiar</span>
              </>
            )}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
