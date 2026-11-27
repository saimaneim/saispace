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
  "h-9 px-3 text-sm bg-obsidian text-paper placeholder:text-fog rounded-md focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40 transition-colors duration-[120ms]";

function AddSecretDialog({
  open,
  onOpenChange,
  onSaved,
  existingNames,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSaved: () => void;
  existingNames: string[];
}) {
  const [name, setName] = useState("");
  const [value, setValue] = useState("");
  const [description, setDescription] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const replaces = existingNames.includes(name.trim());

  const handleOpenChange = (next: boolean) => {
    if (!next) {
      setName("");
      setValue("");
      setDescription("");
      setError(null);
    }
    onOpenChange(next);
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setIsSaving(true);
    setError(null);
    try {
      await api.setSecret(name.trim(), value, description.trim());
      onSaved();
      handleOpenChange(false);
    } catch (err) {
      setError(formatError(err));
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <form onSubmit={submit} className="space-y-4">
          <DialogHeader>
            <DialogTitle>Guardar clave</DialogTitle>
            <DialogDescription>
              Cifrada en tu equipo; los agentes solo ven el nombre.
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-3">
            <div className="flex flex-col gap-1.5">
              <label htmlFor="secret-name" className="text-xs font-medium text-fog">
                Nombre
              </label>
              <input
                id="secret-name"
                value={name}
                onChange={(e) => setName(e.target.value.toUpperCase().replace(/[^A-Z0-9_]/g, "_"))}
                placeholder="GITHUB_TOKEN…"
                aria-label="Nombre"
                spellCheck={false}
                className={cn(inputClass, "font-mono")}
              />
            </div>
            <div className="flex flex-col gap-1.5">
              <label htmlFor="secret-value" className="text-xs font-medium text-fog">
                Valor
              </label>
              <input
                id="secret-value"
                type="password"
                value={value}
                onChange={(e) => setValue(e.target.value)}
                placeholder="Valor secreto…"
                aria-label="Valor"
                autoComplete="off"
                className={cn(inputClass, "font-mono")}
              />
            </div>
            <div className="flex flex-col gap-1.5">
              <label htmlFor="secret-desc" className="text-xs font-medium text-fog">
                Descripción (opcional)
              </label>
              <input
                id="secret-desc"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder="Para qué sirve…"
                aria-label="Descripción"
                className={inputClass}
              />
            </div>
          </div>
          {replaces && (
            <p className="text-xs text-status-amber break-words">
              Ya existe {name.trim()}: se sustituirá su valor.
            </p>
          )}
          {error && (
            <p className="text-sm text-coral-red break-words" role="alert">
              {error}
            </p>
          )}
          <DialogFooter>
            <Button type="button" variant="secondary" onClick={() => handleOpenChange(false)}>
              Cancelar
            </Button>
            <Button type="submit" variant="default" disabled={!name || !value || isSaving}>
              {isSaving ? "Guardando…" : replaces ? "Sustituir" : "Guardar"}
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
    <div className="flex-1 overflow-y-auto px-6 pb-6 pt-2">
      <div className="max-w-3xl space-y-6">
        {pending.length > 0 && (
          <section className="space-y-2">
            <h3 className="text-xs font-semibold text-accent px-1">Solicitudes de acceso</h3>
            {pending.map((grant) => (
              <div
                key={grant.id}
                className="flex items-center gap-3 px-4 py-3 rounded-xl bg-elevated min-w-0"
              >
                <div className="flex-1 min-w-0">
                  <p className="text-sm text-paper break-words">
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

        <section className="space-y-2">
          <div className="flex items-center justify-between px-1">
            <h3 className="text-xs font-semibold text-fog">Claves guardadas</h3>
            <Button size="sm" variant="secondary" onClick={() => setIsAddOpen(true)}>
              <Plus className="w-3.5 h-3.5 mr-1.5" />
              Añadir
            </Button>
          </div>

          {secrets.length === 0 && (
            <div className="flex flex-col items-center gap-2 py-12 text-center">
              <div className="w-10 h-10 rounded-xl bg-carbon flex items-center justify-center text-ash mb-1">
                <KeyRound className="w-5 h-5" />
              </div>
              <p className="text-sm font-medium text-paper">Sin claves</p>
              <p className="text-xs text-fog max-w-xs leading-relaxed">
                Los agentes las usan con <code className="font-mono text-mist">sai secret run</code>{" "}
                sin leer su valor.
              </p>
            </div>
          )}

          {secrets.map((secret) => {
            const agents = grants.filter(
              (g) => g.secret_id === secret.id && g.status !== "pending",
            );
            return (
              <div
                key={secret.id}
                className="group px-4 py-3 rounded-xl bg-carbon hover:bg-obsidian/80 transition-colors duration-[120ms] min-w-0"
              >
                <div className="flex items-center gap-3 min-w-0">
                  <KeyRound className="w-4 h-4 text-fog flex-shrink-0" />
                  <span className="font-mono text-sm font-medium text-paper truncate">
                    {secret.name}
                  </span>
                  <span className="text-xs text-fog truncate flex-1 min-w-0">
                    {secret.description}
                  </span>
                  <span
                    className="text-xs text-fog tabular-nums flex-shrink-0"
                    title="Cuenta solo cuando un agente la usa con sai secret run"
                  >
                    {secret.last_used_at
                      ? `usada ${formatTimeAgo(secret.last_used_at)}`
                      : "sin usar"}
                  </span>
                  <button
                    type="button"
                    onClick={() => setPendingDelete(secret)}
                    aria-label={`Eliminar ${secret.name}`}
                    className="h-7 w-7 rounded-md flex items-center justify-center text-fog opacity-0 group-hover:opacity-100 hover:text-coral-red hover:bg-coral-red/10 focus:opacity-100 focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40 transition-opacity duration-[120ms] flex-shrink-0"
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
                          className="h-4 w-4 rounded-full flex items-center justify-center hover:bg-obsidian transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40"
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

      <AddSecretDialog
        open={isAddOpen}
        onOpenChange={setIsAddOpen}
        onSaved={load}
        existingNames={secrets.map((s) => s.name)}
      />

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
