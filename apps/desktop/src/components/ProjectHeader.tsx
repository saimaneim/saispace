import {
  Activity,
  Archive,
  Check,
  CircleCheck,
  CirclePause,
  CirclePlay,
  Copy,
  MoreHorizontal,
  Pin,
  PinOff,
  Trash2,
} from "lucide-react";
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

interface ProjectHeaderProps {
  project: Project;
  onDeleteProject: () => Promise<void>;
  isFeedOpen: boolean;
  onToggleFeed: () => void;
  onMove: (target: { pinned: boolean; status: ProjectStatus }) => void;
}

const MOVE_TO: { status: ProjectStatus; label: string; icon: typeof Pin }[] = [
  { status: "active", label: "Mover a Activos", icon: CirclePlay },
  { status: "on_hold", label: "Mover a En pausa", icon: CirclePause },
  { status: "completed", label: "Mover a Completados", icon: CircleCheck },
  { status: "archived", label: "Mover a Archivados", icon: Archive },
];

export function ProjectHeader({
  project,
  onDeleteProject,
  isFeedOpen,
  onToggleFeed,
  onMove,
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
      <div className="pointer-events-none flex min-w-0 items-baseline gap-3">
        <h1 className="text-paper truncate text-sm font-semibold" title={project.path}>
          {project.name}
        </h1>
      </div>

      <div className="pointer-events-none flex-1" />

      <div className="pointer-events-auto flex flex-shrink-0 items-center gap-1">
        <button
          type="button"
          onClick={onToggleFeed}
          title={isFeedOpen ? "Ocultar actividad" : "Mostrar actividad"}
          aria-label={isFeedOpen ? "Ocultar actividad" : "Mostrar actividad"}
          aria-pressed={isFeedOpen}
          className={cn(
            "focus-visible:ring-accent/40 flex h-8 w-8 items-center justify-center rounded-md transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2",
            isFeedOpen ? "bg-elevated text-paper" : "text-fog hover:text-paper hover:bg-obsidian",
          )}
        >
          <Activity className="h-4 w-4" />
        </button>

        <Menu
          trigger={({ toggle }) => (
            <button
              type="button"
              onClick={toggle}
              aria-label="Más opciones"
              title="Más opciones"
              className="text-fog hover:text-paper hover:bg-obsidian focus-visible:ring-accent/40 flex h-8 w-8 items-center justify-center rounded-md transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2"
            >
              <MoreHorizontal className="h-4 w-4" />
            </button>
          )}
          items={[
            {
              label: project.pinned ? "Desfijar" : "Fijar",
              icon: project.pinned ? PinOff : Pin,
              onSelect: () => onMove({ pinned: !project.pinned, status: project.status }),
            },
            ...MOVE_TO.filter((m) => project.pinned || m.status !== project.status).map((m) => ({
              label: m.label,
              icon: m.icon,
              onSelect: () => onMove({ pinned: false, status: m.status }),
            })),
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
