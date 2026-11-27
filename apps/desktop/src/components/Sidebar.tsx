import { FolderOpen, Globe, Pin, PinOff, Plus, Search, X } from "lucide-react";
import { Fragment, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { Project } from "../lib/types";
import { cn } from "../lib/utils";
import { ProjectIcon } from "./ProjectIcon";

interface SidebarProps {
  projects: Project[];
  selectedProjectId: string | null;
  onSelectProject: (id: string) => void;
  onOpenDirectoryPicker: () => void;
  taskCounts: Record<string, number>;
  onTogglePin: (id: string, pinned: boolean) => void;
  onMove: (id: string, pinned: boolean, ids: string[]) => void;
  isOpen: boolean;
  isGlobalSelected: boolean;
  onSelectGlobal: () => void;
  pendingRequests: number;
}

type Group = "pinned" | "rest";

interface DragState {
  id: string;
  startY: number;
  pointerY: number;
  grabOffset: number;
  left: number;
  width: number;
  active: boolean;
  target: { group: Group; index: number };
}

const DRAG_THRESHOLD = 4;
const EDGE_SCROLL_PX = 40;
const PLACEHOLDER = "__placeholder__";

interface RowProps {
  project: Project;
  isSelected: boolean;
  openTasks: number;
  canDrag: boolean;
  onSelect: (id: string) => void;
  onTogglePin: (id: string, pinned: boolean) => void;
  onPointerDown?: (e: React.PointerEvent<HTMLElement>) => void;
  floating?: boolean;
}

function ProjectRow({
  project: p,
  isSelected,
  openTasks,
  canDrag,
  onSelect,
  onTogglePin,
  onPointerDown,
  floating = false,
}: RowProps) {
  return (
    <div className="group relative rounded-md">
      <button
        type="button"
        onClick={() => onSelect(p.id)}
        onPointerDown={onPointerDown}
        aria-current={isSelected ? "page" : undefined}
        className={cn(
          "relative w-full flex items-center gap-2.5 pl-3 pr-9 h-10 rounded-md text-sm text-left transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
          isSelected || floating
            ? "bg-elevated text-paper font-medium"
            : "text-mist hover:text-paper hover:bg-obsidian",
          canDrag && "cursor-grab",
          floating && "cursor-grabbing",
        )}
      >
        {isSelected && !floating && (
          <span
            className="absolute left-1 top-2.5 bottom-2.5 w-0.5 rounded-full bg-acid-lime"
            aria-hidden="true"
          />
        )}
        <ProjectIcon projectId={p.id} name={p.name} status={p.status} />
        <span className="truncate flex-1">{p.name}</span>
        {openTasks > 0 && (
          <span className="text-xs text-fog tabular-nums flex-shrink-0 transition-opacity duration-[120ms] group-hover:opacity-0">
            {openTasks}
          </span>
        )}
      </button>
      {!floating && (
        <button
          type="button"
          onClick={() => onTogglePin(p.id, !p.pinned)}
          aria-label={p.pinned ? `Desfijar ${p.name}` : `Fijar ${p.name}`}
          title={p.pinned ? "Desfijar" : "Fijar arriba"}
          className="absolute right-1.5 top-1/2 -translate-y-1/2 h-7 w-7 rounded-md flex items-center justify-center text-fog opacity-0 group-hover:opacity-100 focus-visible:opacity-100 hover:text-paper hover:bg-graphite transition-opacity duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
        >
          {p.pinned ? <PinOff className="w-3.5 h-3.5" /> : <Pin className="w-3.5 h-3.5" />}
        </button>
      )}
    </div>
  );
}

function useFlip(order: string) {
  const nodes = useRef(new Map<string, HTMLElement>());
  const last = useRef(new Map<string, number>());

  useLayoutEffect(() => {
    const next = new Map<string, number>();
    for (const [id, el] of nodes.current) {
      const top = el.getBoundingClientRect().top;
      next.set(id, top);
      const prev = last.current.get(id);
      if (prev === undefined || prev === top) continue;
      el.style.transition = "none";
      el.style.transform = `translateY(${prev - top}px)`;
      el.getBoundingClientRect();
      el.style.transition = "transform 150ms cubic-bezier(0.2, 0, 0, 1)";
      el.style.transform = "";
    }
    last.current = next;
  }, [order]);

  return (id: string) => (el: HTMLElement | null) => {
    if (el) nodes.current.set(id, el);
    else nodes.current.delete(id);
  };
}

export function Sidebar({
  projects,
  selectedProjectId,
  onSelectProject,
  onOpenDirectoryPicker,
  taskCounts,
  onTogglePin,
  onMove,
  isOpen,
  isGlobalSelected,
  onSelectGlobal,
  pendingRequests,
}: SidebarProps) {
  const [searchQuery, setSearchQuery] = useState("");
  const [drag, setDrag] = useState<DragState | null>(null);
  const dragRef = useRef<DragState | null>(null);
  const suppressClick = useRef(false);
  const navRef = useRef<HTMLElement | null>(null);
  const zoneRefs = useRef<Record<Group, HTMLElement | null>>({ pinned: null, rest: null });

  const isFiltering = searchQuery.trim() !== "";
  const filteredProjects = projects.filter((p) =>
    p.name.toLowerCase().includes(searchQuery.toLowerCase()),
  );
  const byId = new Map(projects.map((p) => [p.id, p]));
  const dragging = drag?.active ? drag : null;

  const columns = (group: Group): string[] => {
    const ids = filteredProjects
      .filter((p) => p.pinned === (group === "pinned") && p.id !== dragging?.id)
      .map((p) => p.id);
    if (dragging?.target.group === group) {
      ids.splice(Math.min(dragging.target.index, ids.length), 0, PLACEHOLDER);
    }
    return ids;
  };
  const pinnedIds = columns("pinned");
  const restIds = columns("rest");
  const setRowRef = useFlip(`${pinnedIds.join()}|${restIds.join()}`);

  const update = (next: DragState | null) => {
    dragRef.current = next;
    setDrag(next);
  };

  const isArmed = drag !== null;

  useEffect(() => {
    if (!isArmed) return;

    const targetAt = (y: number, id: string): DragState["target"] => {
      const zones = (["pinned", "rest"] as const).flatMap((group) => {
        const el = zoneRefs.current[group];
        return el ? [{ group, rect: el.getBoundingClientRect(), el }] : [];
      });
      const zone =
        zones.find((z) => y >= z.rect.top && y <= z.rect.bottom) ??
        zones.reduce((best, z) => {
          const dist = Math.min(Math.abs(y - z.rect.top), Math.abs(y - z.rect.bottom));
          const bestDist = Math.min(Math.abs(y - best.rect.top), Math.abs(y - best.rect.bottom));
          return dist < bestDist ? z : best;
        });
      const rows = [...zone.el.querySelectorAll<HTMLElement>("[data-row-id]")].filter(
        (n) => n.dataset.rowId !== id && n.dataset.rowId !== PLACEHOLDER,
      );
      const index = rows.filter((n) => {
        const rect = n.getBoundingClientRect();
        return rect.top + rect.height / 2 < y;
      }).length;
      return { group: zone.group, index };
    };

    const onMovePointer = (e: PointerEvent) => {
      const current = dragRef.current;
      if (!current) return;
      const active = current.active || Math.abs(e.clientY - current.startY) > DRAG_THRESHOLD;
      if (!active) return;
      const nav = navRef.current;
      if (nav) {
        const rect = nav.getBoundingClientRect();
        if (e.clientY < rect.top + EDGE_SCROLL_PX) nav.scrollBy({ top: -8 });
        else if (e.clientY > rect.bottom - EDGE_SCROLL_PX) nav.scrollBy({ top: 8 });
      }
      update({ ...current, active, pointerY: e.clientY, target: targetAt(e.clientY, current.id) });
    };

    const finish = (commit: boolean) => {
      const current = dragRef.current;
      update(null);
      if (!current?.active) return;
      suppressClick.current = true;
      window.setTimeout(() => {
        suppressClick.current = false;
      }, 0);
      if (!commit) return;
      const pinned = current.target.group === "pinned";
      const without = (group: Group) =>
        projects
          .filter((p) => p.pinned === (group === "pinned") && p.id !== current.id)
          .map((p) => p.id);
      const pinnedOrder = without("pinned");
      const restOrder = without("rest");
      (pinned ? pinnedOrder : restOrder).splice(current.target.index, 0, current.id);
      const ids = [...pinnedOrder, ...restOrder];
      const before = projects.map((p) => p.id).join();
      const wasPinned = projects.find((p) => p.id === current.id)?.pinned;
      if (ids.join() === before && wasPinned === pinned) return;
      onMove(current.id, pinned, ids);
    };

    const onUp = () => finish(true);
    const onCancel = () => finish(false);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") finish(false);
    };
    window.addEventListener("pointermove", onMovePointer);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onCancel);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointermove", onMovePointer);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onCancel);
      window.removeEventListener("keydown", onKey);
    };
  }, [isArmed, projects, onMove]);

  const startDrag = (project: Project, e: React.PointerEvent<HTMLElement>) => {
    if (isFiltering || e.button !== 0) return;
    const row = e.currentTarget.getBoundingClientRect();
    const group: Group = project.pinned ? "pinned" : "rest";
    const index = projects
      .filter((p) => p.pinned === project.pinned)
      .findIndex((p) => p.id === project.id);
    update({
      id: project.id,
      startY: e.clientY,
      pointerY: e.clientY,
      grabOffset: e.clientY - row.top,
      left: row.left,
      width: row.width,
      active: false,
      target: { group, index },
    });
  };

  const select = (id: string) => {
    if (suppressClick.current) {
      suppressClick.current = false;
      return;
    }
    onSelectProject(id);
  };

  const renderColumn = (group: Group, ids: string[]) => (
    <ul
      ref={(el) => {
        zoneRefs.current[group] = el;
      }}
      className={cn("space-y-0.5", ids.length === 0 && "min-h-2")}
    >
      {ids.map((id) => {
        if (id === PLACEHOLDER) {
          return (
            <li
              key={PLACEHOLDER}
              data-row-id={PLACEHOLDER}
              ref={setRowRef(PLACEHOLDER)}
              className="h-10 rounded-md bg-obsidian"
            />
          );
        }
        const p = byId.get(id);
        if (!p) return null;
        return (
          <li key={id} data-row-id={id} ref={setRowRef(id)}>
            <ProjectRow
              project={p}
              isSelected={id === selectedProjectId}
              openTasks={taskCounts[id] ?? 0}
              canDrag={!isFiltering}
              onSelect={select}
              onTogglePin={onTogglePin}
              onPointerDown={(e) => startDrag(p, e)}
            />
          </li>
        );
      })}
    </ul>
  );

  const showPinned = pinnedIds.length > 0 || dragging !== null;
  const draggedProject = dragging ? byId.get(dragging.id) : undefined;

  return (
    <aside
      aria-hidden={!isOpen}
      inert={!isOpen}
      className={cn(
        "flex-shrink-0 bg-carbon overflow-hidden transition-[width] duration-[150ms] ease-out select-none",
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
              "relative w-full flex items-center gap-2.5 px-3 h-10 rounded-md text-sm text-left transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
              isGlobalSelected
                ? "bg-elevated text-paper font-medium"
                : "text-mist hover:text-paper hover:bg-obsidian",
            )}
          >
            {isGlobalSelected && (
              <span
                className="absolute left-1 top-2.5 bottom-2.5 w-0.5 rounded-full bg-acid-lime"
                aria-hidden="true"
              />
            )}
            <Globe
              className={cn("w-4 h-4 flex-shrink-0", isGlobalSelected ? "text-paper" : "text-fog")}
            />
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
                className="w-full h-8 pl-8 pr-7 text-xs bg-obsidian text-paper placeholder:text-fog rounded-md focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40 transition-colors duration-[120ms]"
              />
              {searchQuery && (
                <button
                  type="button"
                  onClick={() => setSearchQuery("")}
                  aria-label="Limpiar búsqueda de proyectos"
                  className="absolute right-1 top-1/2 -translate-y-1/2 h-6 w-6 rounded flex items-center justify-center text-fog hover:text-paper hover:bg-elevated transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
                >
                  <X className="w-3 h-3" />
                </button>
              )}
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

        <nav ref={navRef} className="flex-1 overflow-y-auto px-3 pb-3" aria-label="Proyectos">
          {showPinned && (
            <Fragment>
              <p className="flex items-center gap-1.5 px-2.5 pt-1 pb-1 text-xs text-fog">
                <Pin className="w-3 h-3" />
                Fijados
                {dragging && pinnedIds.length === 0 && (
                  <span className="text-ash">· suelta aquí para fijar</span>
                )}
              </p>
              {renderColumn("pinned", pinnedIds)}
              <div className="h-px bg-obsidian mx-2.5 my-2" />
            </Fragment>
          )}
          {renderColumn("rest", restIds)}

          {filteredProjects.length === 0 && (
            <div className="py-8 text-center text-xs text-fog space-y-1">
              <FolderOpen className="w-6 h-6 mx-auto text-ash mb-2" />
              <p>Sin proyectos</p>
            </div>
          )}
        </nav>
      </div>
      {dragging && draggedProject && (
        <div
          className="fixed z-50 pointer-events-none rounded-md shadow-popover rotate-1 scale-[1.02]"
          style={{
            top: dragging.pointerY - dragging.grabOffset,
            left: dragging.left,
            width: dragging.width,
          }}
        >
          <ProjectRow
            project={draggedProject}
            isSelected={false}
            openTasks={taskCounts[draggedProject.id] ?? 0}
            canDrag
            onSelect={() => {}}
            onTogglePin={() => {}}
            floating
          />
        </div>
      )}
    </aside>
  );
}
