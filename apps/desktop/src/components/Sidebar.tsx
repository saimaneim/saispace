import { FolderOpen, Globe, GripVertical, Pin, PinOff, Plus, Search } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { Project, ProjectStatus } from "../lib/types";
import { cn } from "../lib/utils";

interface SidebarProps {
  projects: Project[];
  selectedProjectId: string | null;
  onSelectProject: (id: string) => void;
  onOpenDirectoryPicker: () => void;
  taskCounts: Record<string, number>;
  onTogglePin: (id: string, pinned: boolean) => void;
  onReorder: (ids: string[]) => void;
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

type Group = "pinned" | "rest";

interface DragState {
  id: string;
  group: Group;
  startY: number;
  active: boolean;
  order: string[];
}

const DRAG_THRESHOLD = 4;

interface ProjectGroupProps {
  group: Group;
  items: Project[];
  selectedProjectId: string | null;
  taskCounts: Record<string, number>;
  draggingId: string | null;
  canDrag: boolean;
  onSelect: (id: string) => void;
  onTogglePin: (id: string, pinned: boolean) => void;
  onDragStart: (id: string, group: Group, startY: number) => void;
}

function ProjectGroup({
  group,
  items,
  selectedProjectId,
  taskCounts,
  draggingId,
  canDrag,
  onSelect,
  onTogglePin,
  onDragStart,
}: ProjectGroupProps) {
  return (
    <ul className="space-y-0.5">
      {items.map((p) => {
        const isSelected = p.id === selectedProjectId;
        const isDragging = p.id === draggingId;
        const openTasks = taskCounts[p.id] ?? 0;
        return (
          <li
            key={p.id}
            data-drag-group={group}
            data-project-id={p.id}
            className={cn(
              "group relative rounded-md",
              isDragging && "bg-elevated shadow-popover z-10",
            )}
          >
            <button
              type="button"
              onClick={() => onSelect(p.id)}
              onPointerDown={(e) => {
                if (canDrag && e.button === 0) onDragStart(p.id, group, e.clientY);
              }}
              aria-current={isSelected ? "page" : undefined}
              className={cn(
                "w-full flex items-center gap-2.5 pl-2.5 pr-9 h-10 rounded-md text-sm text-left transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
                isSelected
                  ? "bg-elevated text-paper font-medium"
                  : "text-mist hover:text-paper hover:bg-obsidian",
                isDragging && "cursor-grabbing",
              )}
            >
              <span className="relative w-2 h-2 flex-shrink-0 flex items-center justify-center">
                <span
                  className={cn(
                    "w-2 h-2 rounded-full",
                    STATUS_DOT[p.status],
                    canDrag && "group-hover:opacity-0",
                  )}
                />
                {canDrag && (
                  <GripVertical className="absolute w-3.5 h-3.5 text-fog opacity-0 group-hover:opacity-100" />
                )}
              </span>
              <span className="truncate flex-1">{p.name}</span>
              {openTasks > 0 && (
                <span className="text-xs text-fog tabular-nums flex-shrink-0 group-hover:opacity-0">
                  {openTasks}
                </span>
              )}
            </button>
            <button
              type="button"
              onClick={() => onTogglePin(p.id, !p.pinned)}
              aria-label={p.pinned ? `Desfijar ${p.name}` : `Fijar ${p.name}`}
              title={p.pinned ? "Desfijar" : "Fijar arriba"}
              className="absolute right-1.5 top-1/2 -translate-y-1/2 h-7 w-7 rounded-md flex items-center justify-center text-fog opacity-0 group-hover:opacity-100 focus-visible:opacity-100 hover:text-paper hover:bg-graphite transition-opacity duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
            >
              {p.pinned ? <PinOff className="w-3.5 h-3.5" /> : <Pin className="w-3.5 h-3.5" />}
            </button>
          </li>
        );
      })}
    </ul>
  );
}

export function Sidebar({
  projects,
  selectedProjectId,
  onSelectProject,
  onOpenDirectoryPicker,
  taskCounts,
  onTogglePin,
  onReorder,
  isOpen,
  isGlobalSelected,
  onSelectGlobal,
  pendingRequests,
}: SidebarProps) {
  const [searchQuery, setSearchQuery] = useState("");
  const [drag, setDrag] = useState<DragState | null>(null);
  const suppressClick = useRef(false);

  const filteredProjects = projects.filter((p) =>
    p.name.toLowerCase().includes(searchQuery.toLowerCase()),
  );
  const isFiltering = searchQuery.trim() !== "";

  const order = (group: Group) => {
    const base = filteredProjects.filter((p) => p.pinned === (group === "pinned"));
    if (!drag || drag.group !== group || !drag.active) return base;
    const byId = new Map(base.map((p) => [p.id, p]));
    return drag.order.flatMap((id) => byId.get(id) ?? []);
  };
  const pinned = order("pinned");
  const rest = order("rest");

  const dragRef = useRef<DragState | null>(null);
  const updateDrag = (next: DragState | null) => {
    dragRef.current = next;
    setDrag(next);
  };
  const isDragArmed = drag !== null;

  useEffect(() => {
    if (!isDragArmed) return;
    const onMove = (e: PointerEvent) => {
      const current = dragRef.current;
      if (!current) return;
      const active = current.active || Math.abs(e.clientY - current.startY) > DRAG_THRESHOLD;
      if (!active) return;
      const nodes = document.querySelectorAll<HTMLElement>(`[data-drag-group="${current.group}"]`);
      const others = [...nodes].filter((n) => n.dataset.projectId !== current.id);
      let index = others.findIndex((n) => {
        const rect = n.getBoundingClientRect();
        return e.clientY < rect.top + rect.height / 2;
      });
      if (index === -1) index = others.length;
      const ids = others.flatMap((n) => n.dataset.projectId ?? []);
      ids.splice(index, 0, current.id);
      dragRef.current = { ...current, active, order: ids };
      setDrag(dragRef.current);
    };
    const onUp = () => {
      const current = dragRef.current;
      dragRef.current = null;
      setDrag(null);
      if (!current?.active) return;
      suppressClick.current = true;
      window.setTimeout(() => {
        suppressClick.current = false;
      }, 0);
      const isPinnedGroup = current.group === "pinned";
      const initial = projects.filter((p) => p.pinned === isPinnedGroup).map((p) => p.id);
      if (current.order.join() === initial.join()) return;
      const pinnedIds = isPinnedGroup
        ? current.order
        : projects.filter((p) => p.pinned).map((p) => p.id);
      const restIds = isPinnedGroup
        ? projects.filter((p) => !p.pinned).map((p) => p.id)
        : current.order;
      onReorder([...pinnedIds, ...restIds]);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
    };
  }, [isDragArmed, projects, onReorder]);

  const groupProps = {
    selectedProjectId,
    taskCounts,
    draggingId: drag?.active ? drag.id : null,
    canDrag: !isFiltering,
    onSelect: (id: string) => {
      if (suppressClick.current) {
        suppressClick.current = false;
        return;
      }
      onSelectProject(id);
    },
    onTogglePin,
    onDragStart: (id: string, group: Group, startY: number) =>
      updateDrag({
        id,
        group,
        startY,
        active: false,
        order: projects.filter((p) => p.pinned === (group === "pinned")).map((p) => p.id),
      }),
  };

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
          <div className="px-3 pb-2 flex-shrink-0">
            <div className="relative">
              <Search className="w-3.5 h-3.5 absolute left-2.5 top-1/2 -translate-y-1/2 text-fog pointer-events-none" />
              <input
                type="text"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder="Buscar proyectos…"
                aria-label="Buscar proyectos"
                className="w-full h-8 pl-8 pr-2.5 text-xs bg-obsidian text-paper placeholder:text-fog rounded-md focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40 transition-colors duration-[120ms]"
              />
            </div>
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

        <nav className="flex-1 overflow-y-auto px-3 pb-3" aria-label="Proyectos">
          {pinned.length > 0 && (
            <>
              <p className="flex items-center gap-1.5 px-2.5 pt-1 pb-1 text-xs text-fog">
                <Pin className="w-3 h-3" />
                Fijados
              </p>
              <ProjectGroup group="pinned" items={pinned} {...groupProps} />
              <div className="h-px bg-obsidian mx-2.5 my-2" />
            </>
          )}
          <ProjectGroup group="rest" items={rest} {...groupProps} />

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
