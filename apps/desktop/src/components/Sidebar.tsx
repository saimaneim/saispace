import { FolderOpen, Globe, Plus, Search } from "lucide-react";
import { useState } from "react";
import type { Project, ProjectStatus } from "../lib/types";
import { cn } from "../lib/utils";

interface SidebarProps {
  projects: Project[];
  selectedProjectId: string | null;
  onSelectProject: (id: string) => void;
  onOpenDirectoryPicker: () => void;
  taskCounts: Record<string, number>;
  isOpen: boolean;
  isGlobalSelected: boolean;
  onSelectGlobal: () => void;
  pendingRequests: number;
}

const STATUS_DOT: Record<ProjectStatus, string> = {
  active: "bg-pulse-green",
  on_hold: "bg-status-amber",
  completed: "bg-fog",
  archived: "bg-ash",
};

export function Sidebar({
  projects,
  selectedProjectId,
  onSelectProject,
  onOpenDirectoryPicker,
  taskCounts,
  isOpen,
  isGlobalSelected,
  onSelectGlobal,
  pendingRequests,
}: SidebarProps) {
  const [searchQuery, setSearchQuery] = useState("");

  const filteredProjects = projects.filter((p) =>
    p.name.toLowerCase().includes(searchQuery.toLowerCase()),
  );

  return (
    <aside
      aria-hidden={!isOpen}
      inert={!isOpen}
      className={cn(
        "flex-shrink-0 bg-carbon overflow-hidden transition-[width] duration-200 ease-out select-none",
        isOpen ? "w-64" : "w-0",
      )}
    >
      <div className="w-64 h-full flex flex-col text-mist">
        <div data-tauri-drag-region className="h-12 flex-shrink-0 flex items-center gap-2.5 px-4">
          <svg
            width="20"
            height="20"
            viewBox="0 0 24 24"
            fill="none"
            xmlns="http://www.w3.org/2000/svg"
            className="flex-shrink-0 pointer-events-none"
            aria-hidden="true"
          >
            <rect width="24" height="24" rx="6" className="fill-elevated" />
            <path
              d="M15 7.5H9.5C8.11929 7.5 7 8.61929 7 10C7 11.3807 8.11929 12.5 9.5 12.5H14.5C15.8807 12.5 17 13.6193 17 15C17 16.3807 15.8807 17.5 14.5 17.5H9"
              className="stroke-acid-lime"
              strokeWidth="2.2"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
            <circle cx="15.5" cy="7.5" r="1.5" className="fill-acid-lime" />
          </svg>
          <span className="text-sm font-semibold tracking-tight text-paper pointer-events-none">
            saispace
          </span>
        </div>

        <div className="px-3 pb-2 flex-shrink-0">
          <button
            type="button"
            onClick={onSelectGlobal}
            aria-current={isGlobalSelected ? "page" : undefined}
            className={cn(
              "w-full flex items-center gap-2.5 px-2.5 h-10 rounded-md text-sm text-left transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
              isGlobalSelected
                ? "bg-elevated text-paper font-medium"
                : "text-mist hover:text-paper hover:bg-obsidian",
            )}
          >
            <Globe className="w-4 h-4 flex-shrink-0 text-fog" />
            <span className="flex-1">Global</span>
            {pendingRequests > 0 && (
              <span
                className="min-w-5 h-5 px-1.5 rounded-full bg-acid-lime text-void text-xs font-semibold flex items-center justify-center tabular-nums"
                title="Solicitudes de acceso a claves"
              >
                {pendingRequests}
              </span>
            )}
          </button>
        </div>

        {projects.length > 4 && (
          <div className="relative px-3 pb-2 flex-shrink-0">
            <Search className="w-4 h-4 absolute left-5 top-1/2 -translate-y-1/2 text-fog pointer-events-none" />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Buscar proyectos…"
              aria-label="Buscar proyectos"
              className="w-full h-8 pl-8 pr-2.5 text-xs bg-obsidian text-paper placeholder:text-fog rounded-md focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40 transition-colors duration-[120ms]"
            />
          </div>
        )}

        <div className="flex items-center justify-between pl-4 pr-3 h-8 flex-shrink-0">
          <span className="text-xs font-semibold text-fog">Proyectos</span>
          <button
            type="button"
            onClick={onOpenDirectoryPicker}
            aria-label="Añadir carpeta"
            title="Añadir carpeta"
            className="h-7 w-7 flex items-center justify-center rounded-md text-fog hover:text-paper hover:bg-obsidian transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
          >
            <Plus className="w-4 h-4" />
          </button>
        </div>

        <nav className="flex-1 overflow-y-auto px-3 pb-3 space-y-0.5" aria-label="Proyectos">
          {filteredProjects.map((p) => {
            const isSelected = p.id === selectedProjectId;
            const openTasks = taskCounts[p.id] ?? 0;
            return (
              <button
                key={p.id}
                type="button"
                onClick={() => onSelectProject(p.id)}
                aria-current={isSelected ? "page" : undefined}
                className={cn(
                  "w-full flex items-center gap-2.5 px-2.5 h-10 rounded-md text-sm text-left transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
                  isSelected
                    ? "bg-elevated text-paper font-medium"
                    : "text-mist hover:text-paper hover:bg-obsidian",
                )}
              >
                <span className={cn("w-2 h-2 rounded-full flex-shrink-0", STATUS_DOT[p.status])} />
                <span className="truncate flex-1">{p.name}</span>
                {openTasks > 0 && (
                  <span className="text-xs text-fog tabular-nums flex-shrink-0">{openTasks}</span>
                )}
              </button>
            );
          })}

          {filteredProjects.length === 0 && (
            <div className="py-8 text-center text-xs text-fog space-y-1">
              <FolderOpen className="w-6 h-6 mx-auto text-ash mb-2" />
              <p>Sin proyectos</p>
            </div>
          )}
        </nav>
      </div>
    </aside>
  );
}
