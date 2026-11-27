import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { ProjectStatus } from "../lib/types";
import { cn } from "../lib/utils";

const STATUS_DOT: Record<ProjectStatus, string> = {
  active: "bg-pulse-green",
  on_hold: "bg-status-amber",
  completed: "bg-fog",
  archived: "bg-ash",
};

const STATUS_LABEL: Record<ProjectStatus, string> = {
  active: "Activo",
  on_hold: "En pausa",
  completed: "Completado",
  archived: "Archivado",
};

const memo = new Map<string, Promise<string | null>>();

function loadFavicon(projectId: string): Promise<string | null> {
  let pending = memo.get(projectId);
  if (!pending) {
    pending = api.projectFavicon(projectId).catch(() => null);
    memo.set(projectId, pending);
  }
  return pending;
}

interface ProjectIconProps {
  projectId: string;
  name: string;
  status: ProjectStatus;
}

export function ProjectIcon({ projectId, name, status }: ProjectIconProps) {
  const [src, setSrc] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    loadFavicon(projectId).then((icon) => {
      if (alive) setSrc(icon);
    });
    return () => {
      alive = false;
    };
  }, [projectId]);

  const initial = (name.trim().charAt(0) || "?").toUpperCase();

  return (
    <span className="relative w-5 h-5 flex-shrink-0" title={STATUS_LABEL[status]}>
      {src ? (
        <img
          src={src}
          alt=""
          width={20}
          height={20}
          draggable={false}
          className="w-5 h-5 rounded object-contain"
          onError={() => setSrc(null)}
        />
      ) : (
        <span className="w-5 h-5 rounded bg-obsidian text-[11px] font-semibold text-mist flex items-center justify-center">
          {initial}
        </span>
      )}
      <span
        className={cn(
          "absolute -right-0.5 -bottom-0.5 w-2 h-2 rounded-full ring-2 ring-carbon",
          STATUS_DOT[status],
        )}
      />
    </span>
  );
}
