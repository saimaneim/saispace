import { useEffect, useState } from "react";
import { api } from "../lib/api";

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
}

/** The sidebar zone already says the status: the icon is just the project. */
export function ProjectIcon({ projectId, name }: ProjectIconProps) {
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
    <span className="relative h-5 w-5 flex-shrink-0">
      {src ? (
        <img
          src={src}
          alt=""
          width={20}
          height={20}
          draggable={false}
          className="h-5 w-5 rounded object-contain"
          onError={() => setSrc(null)}
        />
      ) : (
        <span className="bg-obsidian text-mist flex h-5 w-5 items-center justify-center rounded text-[11px] font-semibold">
          {initial}
        </span>
      )}
    </span>
  );
}
