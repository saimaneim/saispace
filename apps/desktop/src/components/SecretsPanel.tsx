import { Check, KeyRound, Plus, Trash2, X } from "lucide-react";
import { type FormEvent, useCallback, useEffect, useState } from "react";
import { api, formatError } from "../lib/api";
import type { SecretGrant, SecretMeta } from "../lib/types";
import { cn, formatTimeAgo } from "../lib/utils";
import { Button } from "./ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./ui/dialog";

interface SecretsPanelProps {
  onChanged: () => void;
}

const POLL_MS = 3000;

const STATUS_LABEL: Record<SecretGrant["status"], string> = {
  pending: "pendiente",
  approved: "con acceso",
  denied: "denegado",
};

const inputClass =
  "h-9 px-3 text-sm bg-obsidian text-paper placeholder:text-fog rounded-md focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40";

function AddSecretDialog({
  open,
  onOpenChange,
  onSaved,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSaved: () => void;
}) {
  const [name, setName] = useState("");
  const [value, setValue] = useState("");
  const [description, setDescription] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setIsSaving(true);
    setError(null);
    try {
      await api.setSecret(name.trim(), value, description.trim());
      setName("");
      setValue("");
      setDescription("");
      onSaved();
      onOpenChange(false);
    } catch (err) {
      setError(formatError(err));
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <form onSubmit={submit} className="space-y-4">
          <DialogHeader>
            <DialogTitle>Guardar clave</DialogTitle>
            <DialogDescription>
              Se cifra en tu equipo. Los agentes solo ven el nombre y piden permiso para usarla.
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-2">
            <input
              value={name}
              onChange={(e) => setName(e.target.value.toUpperCase().replace(/[^A-Z0-9_]/g, "_"))}
              placeholder="NOMBRE (ej. GITHUB_TOKEN)"
              aria-label="Nombre"
              className={cn(inputClass, "font-mono")}
            />
            <input
              type="password"
              value={value}
              onChange={(e) => setValue(e.target.value)}
              placeholder="Valor"
              aria-label="Valor"
              autoComplete="off"
              className={cn(inputClass, "font-mono")}
            />
            <input
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="Para qué sirve (opcional)"
              aria-label="Descripción"
              className={inputClass}
            />
          </div>
          {error && <p className="text-sm text-coral-red">{error}</p>}
          <DialogFooter>
            <Button type="button" variant="secondary" onClick={() => onOpenChange(false)}>
              Cancelar
            </Button>
            <Button type="submit" variant="default" disabled={!name || !value || isSaving}>
              {isSaving ? "Guardando…" : "Guardar"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function SecretsPanel({ onChanged }: SecretsPanelProps) {
  const [secrets, setSecrets] = useState<SecretMeta[]>([]);
  const [grants, setGrants] = useState<SecretGrant[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [isAddOpen, setIsAddOpen] = useState(false);
  const [pendingDelete, setPendingDelete] = useState<SecretMeta | null>(null);

  const load = useCallback(async () => {
    try {
      const [s, g] = await Promise.all([api.listSecrets(), api.listSecretGrants()]);
      setSecrets(s);
      setGrants(g);
    } catch (err) {
      setError(formatError(err));
    }
  }, []);

  useEffect(() => {
    load();
    const timer = window.setInterval(load, POLL_MS);
    return () => window.clearInterval(timer);
  }, [load]);

  const act = async (action: () => Promise<void>) => {
    setError(null);
    try {
      await action();
      await load();
      onChanged();
    } catch (err) {
      setError(formatError(err));
    }
  };

  const pending = grants.filter((g) => g.status === "pending");

  return (
    <div className="flex-1 overflow-y-auto px-6 pb-6">
      <div className="max-w-3xl space-y-6">
        {pending.length > 0 && (
          <section className="space-y-1.5">
            <h3 className="text-xs font-semibold text-acid-lime px-1">Solicitudes de acceso</h3>
            {pending.map((grant) => (
              <div
                key={grant.id}
                className="flex items-center gap-3 px-4 py-3 rounded-lg bg-elevated"
              >
                <div className="flex-1 min-w-0">
                  <p className="text-sm text-paper">
                    <span className="font-medium">{grant.agent}</span> quiere usar{" "}
                    <span className="font-mono">{grant.secret_name}</span>
                  </p>
                  {grant.reason && <p className="text-xs text-fog truncate">{grant.reason}</p>}
                </div>
                <Button
                  size="sm"
                  variant="secondary"
                  onClick={() => act(() => api.decideSecretGrant(grant.id, false))}
                >
                  <X className="w-4 h-4 mr-1" />
                  Denegar
                </Button>
                <Button
                  size="sm"
                  variant="default"
                  onClick={() => act(() => api.decideSecretGrant(grant.id, true))}
                >
                  <Check className="w-4 h-4 mr-1" />
                  Permitir
                </Button>
              </div>
            ))}
          </section>
        )}

        <section className="space-y-1.5">
          <div className="flex items-center justify-between px-1">
            <h3 className="text-xs font-semibold text-fog">Claves guardadas</h3>
            <Button size="sm" variant="ghost" onClick={() => setIsAddOpen(true)}>
              <Plus className="w-4 h-4 mr-1" />
              Añadir
            </Button>
          </div>

          {secrets.length === 0 && (
            <div className="flex flex-col items-center gap-2 py-12 text-center">
              <KeyRound className="w-8 h-8 text-ash" />
              <p className="text-sm text-paper">Sin claves</p>
              <p className="text-sm text-fog max-w-sm">
                Guarda tokens y API keys una vez. Tu IA los usa con{" "}
                <code className="font-mono text-mist">sai secret run</code> sin ver el valor.
              </p>
            </div>
          )}

          {secrets.map((secret) => {
            const agents = grants.filter(
              (g) => g.secret_id === secret.id && g.status !== "pending",
            );
            return (
              <div key={secret.id} className="group px-4 py-3 rounded-lg bg-carbon">
                <div className="flex items-center gap-3">
                  <KeyRound className="w-4 h-4 text-fog flex-shrink-0" />
                  <span className="font-mono text-sm text-paper">{secret.name}</span>
                  <span className="text-xs text-fog truncate flex-1">{secret.description}</span>
                  <span
                    className="text-xs text-fog flex-shrink-0"
                    title="Cuenta solo cuando un agente la usa con sai secret run"
                  >
                    {secret.last_used_at
                      ? `usada ${formatTimeAgo(secret.last_used_at)}`
                      : "sin usar vía sai"}
                  </span>
                  <button
                    type="button"
                    onClick={() => setPendingDelete(secret)}
                    aria-label={`Eliminar ${secret.name}`}
                    className="h-7 w-7 rounded-md flex items-center justify-center text-fog opacity-0 group-hover:opacity-100 hover:text-coral-red hover:bg-obsidian focus:opacity-100 focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40 transition-opacity"
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                </div>
                {agents.length > 0 && (
                  <div className="flex flex-wrap gap-1.5 mt-2 pl-7">
                    {agents.map((grant) => (
                      <span
                        key={grant.id}
                        className={cn(
                          "inline-flex items-center gap-1 h-6 pl-2 pr-1 rounded-full text-xs",
                          grant.status === "approved"
                            ? "bg-pulse-green/10 text-pulse-green"
                            : "bg-coral-red/10 text-coral-red",
                        )}
                      >
                        {grant.agent} · {STATUS_LABEL[grant.status]}
                        <button
                          type="button"
                          onClick={() => act(() => api.revokeSecretGrant(grant.id))}
                          aria-label={`Quitar permiso de ${grant.agent}`}
                          title="Quitar"
                          className="h-4 w-4 rounded-full flex items-center justify-center hover:bg-obsidian focus:outline-none"
                        >
                          <X className="w-3 h-3" />
                        </button>
                      </span>
                    ))}
                  </div>
                )}
              </div>
            );
          })}
        </section>

        {error && <p className="text-sm text-coral-red">{error}</p>}
      </div>

      <AddSecretDialog open={isAddOpen} onOpenChange={setIsAddOpen} onSaved={load} />

      <Dialog
        open={pendingDelete !== null}
        onOpenChange={(open) => !open && setPendingDelete(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Eliminar {pendingDelete?.name}</DialogTitle>
            <DialogDescription>
              Se borra la clave y todos sus permisos. Los agentes dejarán de poder usarla.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="secondary" onClick={() => setPendingDelete(null)}>
              Cancelar
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                const target = pendingDelete;
                setPendingDelete(null);
                if (target) act(() => api.deleteSecret(target.name));
              }}
            >
              Eliminar
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
