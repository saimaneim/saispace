import { Activity, Check, Copy, MoreHorizontal, Trash2 } from "lucide-react";
import { useState } from "react";
import type { Project } from "../lib/types";
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

interface ProjectHeaderProps {
  project: Project;
  onDeleteProject: () => Promise<void>;
  isFeedOpen: boolean;
  onToggleFeed: () => void;
}

export function ProjectHeader({
  project,
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
        <h1 className="text-sm font-semibold text-paper truncate" title={project.path}>
          {project.name}
        </h1>
      </div>

      <div className="flex-1 pointer-events-none" />

      <div className="flex items-center gap-1 flex-shrink-0 pointer-events-auto">
        <button
          type="button"
          onClick={onToggleFeed}
          title={isFeedOpen ? "Ocultar actividad" : "Mostrar actividad"}
          aria-label={isFeedOpen ? "Ocultar actividad" : "Mostrar actividad"}
          aria-pressed={isFeedOpen}
          className={cn(
            "h-8 w-8 rounded-md flex items-center justify-center transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40",
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
              className="h-8 w-8 rounded-md flex items-center justify-center text-fog hover:text-paper hover:bg-obsidian transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40"
            >
              <MoreHorizontal className="w-4 h-4" />
            </button>
          )}
          items={[
            {
              label: hasCopiedPath ? "Ruta copiada" : "Copiar ruta",
              icon: hasCopiedPath ? Check : Copy,
              onSelect: () => void copyPath(),
            },
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
