import { Activity, Check, MoreHorizontal, Trash2 } from "lucide-react";
import { useState } from "react";
import type { Project, ProjectStatus } from "../lib/types";
import { cn } from "../lib/utils";
import { Button } from "./ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./ui/dialog";
import { Menu } from "./ui/menu";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "./ui/select";

interface ProjectHeaderProps {
  project: Project;
  onUpdateStatus: (status: ProjectStatus) => Promise<void>;
  onDeleteProject: () => Promise<void>;
  isFeedOpen: boolean;
  onToggleFeed: () => void;
}

const STATUS_OPTIONS: { value: ProjectStatus; label: string; dot: string }[] = [
  { value: "active", label: "Activo", dot: "bg-pulse-green" },
  { value: "on_hold", label: "En pausa", dot: "bg-status-amber" },
  { value: "completed", label: "Completado", dot: "bg-fog" },
  { value: "archived", label: "Archivado", dot: "bg-ash" },
];

export function ProjectHeader({
  project,
  onUpdateStatus,
  onDeleteProject,
  isFeedOpen,
  onToggleFeed,
}: ProjectHeaderProps) {
  const [hasCopiedPath, setHasCopiedPath] = useState(false);
  const [isDeleteDialogOpen, setIsDeleteDialogOpen] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);

  const copyPath = async () => {
    try {
      await navigator.clipboard.writeText(project.path);
      setHasCopiedPath(true);
      setTimeout(() => setHasCopiedPath(false), 1600);
    } catch (err) {
      console.error(err);
    }
  };

  const handleConfirmDelete = async () => {
    setIsDeleting(true);
    try {
      await onDeleteProject();
      setIsDeleteDialogOpen(false);
    } finally {
      setIsDeleting(false);
    }
  };

  return (
    <>
      <div className="flex items-baseline gap-3 min-w-0 pointer-events-none">
        <h1 className="text-sm font-semibold text-paper truncate">{project.name}</h1>
        <button
          type="button"
          onClick={copyPath}
          title="Copiar ruta"
          className="pointer-events-auto flex items-center gap-1.5 min-w-0 text-xs text-fog hover:text-mist transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40 rounded"
        >
          <span className="truncate">{project.path}</span>
          {hasCopiedPath && <Check className="w-3.5 h-3.5 text-pulse-green flex-shrink-0" />}
        </button>
      </div>

      <div className="flex-1 pointer-events-none" />

      <div className="flex items-center gap-1 flex-shrink-0 pointer-events-auto">
        <Select
          value={project.status}
          onValueChange={(val) => onUpdateStatus(val as ProjectStatus)}
        >
          <SelectTrigger
            aria-label="Estado del proyecto"
            className="w-auto gap-2 h-8 bg-transparent rounded-md text-sm text-mist hover:text-paper hover:bg-obsidian focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {STATUS_OPTIONS.map(({ value, label, dot }) => (
              <SelectItem key={value} value={value}>
                <div className="flex items-center gap-2">
                  <span className={cn("w-2 h-2 rounded-full flex-shrink-0", dot)} />
                  <span>{label}</span>
                </div>
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        <button
          type="button"
          onClick={onToggleFeed}
          title={isFeedOpen ? "Ocultar actividad" : "Mostrar actividad"}
          aria-label={isFeedOpen ? "Ocultar actividad" : "Mostrar actividad"}
          aria-pressed={isFeedOpen}
          className={cn(
            "h-8 w-8 rounded-md flex items-center justify-center transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
            isFeedOpen ? "bg-elevated text-paper" : "text-fog hover:text-paper hover:bg-obsidian",
          )}
        >
          <Activity className="w-4 h-4" />
        </button>

        <Menu
          trigger={({ toggle }) => (
            <button
              type="button"
              onClick={toggle}
              aria-label="Más opciones"
              title="Más opciones"
              className="h-8 w-8 rounded-md flex items-center justify-center text-fog hover:text-paper hover:bg-obsidian transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
            >
              <MoreHorizontal className="w-4 h-4" />
            </button>
          )}
          items={[
            {
              label: "Eliminar proyecto…",
              icon: Trash2,
              danger: true,
              onSelect: () => setIsDeleteDialogOpen(true),
            },
          ]}
        />
      </div>

      <Dialog open={isDeleteDialogOpen} onOpenChange={setIsDeleteDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Eliminar proyecto</DialogTitle>
            <DialogDescription>
              Se eliminará el proyecto y sus tareas de saispace. La carpeta en el disco no se toca.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="secondary" onClick={() => setIsDeleteDialogOpen(false)} autoFocus>
              Cancelar
            </Button>
            <Button variant="destructive" onClick={handleConfirmDelete} disabled={isDeleting}>
              {isDeleting ? "Eliminando…" : "Eliminar"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
