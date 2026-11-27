import { FolderOpen, Globe, Plus, Search, Settings, X } from "lucide-react";
import { Fragment, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { Project, ProjectStatus } from "../lib/types";
import { cn } from "../lib/utils";
import { ProjectIcon } from "./ProjectIcon";
import { SettingsDialog } from "./SettingsDialog";

interface SidebarProps {
  projects: Project[];
  selectedProjectId: string | null;
  onSelectProject: (id: string) => void;
  onOpenDirectoryPicker: () => void;
  onMove: (id: string, target: { pinned: boolean; status: ProjectStatus }, ids: string[]) => void;
  isOpen: boolean;
  isGlobalSelected: boolean;
  onSelectGlobal: () => void;
  pendingRequests: number;
}

/** Sidebar zones: dropping a project in one pins it or sets its status. */
type Group = "pinned" | ProjectStatus;
const GROUPS: Group[] = ["pinned", "active", "on_hold", "completed", "archived"];
const GROUP_LABEL: Record<Group, string> = {
  pinned: "Fijados",
  active: "Activos",
  on_hold: "En pausa",
  completed: "Completados",
  archived: "Archivados",
};
const groupOf = (p: Project): Group => (p.pinned ? "pinned" : p.status);

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
  canDrag: boolean;
  onSelect: (id: string) => void;
  onPointerDown?: (e: React.PointerEvent<HTMLElement>) => void;
  floating?: boolean;
}

function ProjectRow({
  project: p,
  isSelected,
  canDrag,
  onSelect,
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
          "relative w-full flex items-center gap-2.5 px-3 h-10 rounded-md text-sm text-left transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40",
          isSelected || floating
            ? "bg-elevated text-paper font-medium"
            : "text-mist hover:text-paper hover:bg-obsidian",
          canDrag && "cursor-grab",
          floating && "cursor-grabbing",
        )}
      >
        <ProjectIcon projectId={p.id} name={p.name} />
        <span className="truncate flex-1">{p.name}</span>
      </button>
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
  onMove,
  isOpen,
  isGlobalSelected,
  onSelectGlobal,
  pendingRequests,
}: SidebarProps) {
  const [searchQuery, setSearchQuery] = useState("");
  const [isSearchOpen, setIsSearchOpen] = useState(false);
  const closeSearch = () => {
    setSearchQuery("");
    setIsSearchOpen(false);
  };
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [drag, setDrag] = useState<DragState | null>(null);
  const dragRef = useRef<DragState | null>(null);
  const suppressClick = useRef(false);
  const navRef = useRef<HTMLElement | null>(null);
  const zoneRefs = useRef<Partial<Record<Group, HTMLElement | null>>>({});

  const isFiltering = searchQuery.trim() !== "";
  const filteredProjects = projects.filter((p) =>
    p.name.toLowerCase().includes(searchQuery.toLowerCase()),
  );
  const byId = new Map(projects.map((p) => [p.id, p]));
  const dragging = drag?.active ? drag : null;

  useEffect(() => {
    if (projects.length <= 4 && searchQuery) {
      setSearchQuery("");
    }
  }, [projects.length, searchQuery]);

  useEffect(() => {
    if (!isOpen && dragRef.current) {
      dragRef.current = null;
      setDrag(null);
    }
  }, [isOpen]);

  const columns = (group: Group): string[] => {
    const ids = filteredProjects
      .filter((p) => groupOf(p) === group && p.id !== dragging?.id)
      .map((p) => p.id);
    if (dragging?.target.group === group) {
      ids.splice(Math.min(dragging.target.index, ids.length), 0, PLACEHOLDER);
    }
    return ids;
  };
  const groupIds = Object.fromEntries(GROUPS.map((g) => [g, columns(g)])) as Record<
    Group,
    string[]
  >;
  const setRowRef = useFlip(GROUPS.map((g) => groupIds[g].join()).join("|"));

  const update = (next: DragState | null) => {
    dragRef.current = next;
    setDrag(next);
  };

  const isArmed = drag !== null;

  useEffect(() => {
    if (!isArmed) return;

    const targetAt = (y: number, id: string): DragState["target"] => {
      const zones = GROUPS.flatMap((group) => {
        const el = zoneRefs.current[group];
        return el ? [{ group, rect: el.getBoundingClientRect(), el }] : [];
      });
      if (zones.length === 0) return { group: "active", index: 0 };
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
      const moved = projects.find((p) => p.id === current.id);
      if (!moved) return;
      const targetGroup = current.target.group;
      const orders = Object.fromEntries(
        GROUPS.map((g) => [
          g,
          projects.filter((p) => groupOf(p) === g && p.id !== current.id).map((p) => p.id),
        ]),
      ) as Record<Group, string[]>;
      orders[targetGroup].splice(current.target.index, 0, current.id);
      const ids = GROUPS.flatMap((g) => orders[g]);
      const pinned = targetGroup === "pinned";
      // Pinning keeps the status; any other zone is the new status.
      const status = pinned ? moved.status : targetGroup;
      const before = projects.map((p) => p.id).join();
      if (ids.join() === before && groupOf(moved) === targetGroup) return;
      onMove(current.id, { pinned, status }, ids);
    };

    const onUp = () => finish(true);
    const onCancel = () => finish(false);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") finish(false);
    };
    window.addEventListener("pointermove", onMovePointer);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onCancel);
    window.addEventListener("blur", onCancel);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointermove", onMovePointer);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onCancel);
      window.removeEventListener("blur", onCancel);
      window.removeEventListener("keydown", onKey);
    };
  }, [isArmed, projects, onMove]);

  const startDrag = (project: Project, e: React.PointerEvent<HTMLElement>) => {
    if (isFiltering || e.button !== 0) return;
    const row = e.currentTarget.getBoundingClientRect();
    const group = groupOf(project);
    const index = projects
      .filter((p) => groupOf(p) === group)
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
              canDrag={!isFiltering}
              onSelect={select}
              onPointerDown={(e) => startDrag(p, e)}
            />
          </li>
        );
      })}
    </ul>
  );

  const draggedProject = dragging ? byId.get(dragging.id) : undefined;

  return (
    <aside
      aria-hidden={!isOpen}
      inert={!isOpen}
      className={cn(
        "flex-shrink-0 bg-carbon overflow-hidden transition-[width] duration-[150ms] ease-out select-none border-r border-obsidian",
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
              className="stroke-accent"
              strokeWidth="2.2"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
            <circle cx="15.5" cy="7.5" r="1.5" className="fill-accent" />
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
              "relative w-full flex items-center gap-2.5 px-3 h-10 rounded-md text-sm text-left transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40",
              isGlobalSelected
                ? "bg-elevated text-paper font-medium"
                : "text-mist hover:text-paper hover:bg-obsidian",
            )}
          >
            <Globe
              className={cn("w-4 h-4 flex-shrink-0", isGlobalSelected ? "text-paper" : "text-fog")}
            />
            <span className="flex-1">Global</span>
            {pendingRequests > 0 && (
              <span
                className="min-w-5 h-5 px-1.5 rounded-full bg-accent text-accent-ink text-xs font-semibold flex items-center justify-center tabular-nums"
                title="Solicitudes de acceso a claves"
              >
                {pendingRequests}
              </span>
            )}
          </button>
        </div>

        <div className="flex items-center gap-0.5 pl-4 pr-3 h-9 flex-shrink-0">
          {isSearchOpen ? (
            <div className="relative flex-1 min-w-0 -ml-1.5">
              <Search className="w-3.5 h-3.5 absolute left-2 top-1/2 -translate-y-1/2 text-fog pointer-events-none" />
              <input
                // biome-ignore lint/a11y/noAutofocus: opened on demand by the search button
                autoFocus
                type="text"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                onKeyDown={(e) => e.key === "Escape" && closeSearch()}
                onBlur={() => !searchQuery && setIsSearchOpen(false)}
                placeholder="Buscar…"
                aria-label="Buscar proyectos"
                className="w-full h-7 pl-7 pr-7 text-xs bg-obsidian text-paper placeholder:text-fog rounded-md focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40"
              />
              <button
                type="button"
                onClick={closeSearch}
                aria-label="Cerrar búsqueda"
                className="absolute right-0.5 top-1/2 -translate-y-1/2 h-6 w-6 rounded flex items-center justify-center text-fog hover:text-paper hover:bg-elevated transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40"
              >
                <X className="w-3 h-3" />
              </button>
            </div>
          ) : (
            <>
              <span className="flex-1 text-xs font-semibold text-fog">Proyectos</span>
              {projects.length > 4 && (
                <button
                  type="button"
                  onClick={() => setIsSearchOpen(true)}
                  aria-label="Buscar proyectos"
                  title="Buscar proyectos"
                  className="h-7 w-7 flex items-center justify-center rounded-md text-fog hover:text-paper hover:bg-obsidian transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40"
                >
                  <Search className="w-3.5 h-3.5" />
                </button>
              )}
            </>
          )}
          <button
            type="button"
            onClick={onOpenDirectoryPicker}
            aria-label="Añadir carpeta"
            title="Añadir carpeta"
            className="h-7 w-7 flex-shrink-0 flex items-center justify-center rounded-md text-fog hover:text-paper hover:bg-obsidian transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40"
          >
            <Plus className="w-4 h-4" />
          </button>
        </div>

        <nav
          ref={navRef}
          className="flex-1 min-h-0 overflow-y-auto px-3 pb-3"
          aria-label="Proyectos"
        >
          {GROUPS.map((group) => {
            const ids = groupIds[group];
            // Empty zones appear only while dragging, as drop targets.
            if (ids.length === 0 && !dragging) return null;
            return (
              <Fragment key={group}>
                <p className="px-2.5 pt-3 pb-1 text-[11px] font-medium text-ash first:pt-1">
                  {GROUP_LABEL[group]}
                </p>
                {renderColumn(group, ids)}
              </Fragment>
            );
          })}

          {filteredProjects.length === 0 && (
            <div className="py-8 text-center text-xs text-fog space-y-1">
              <FolderOpen className="w-6 h-6 mx-auto text-ash mb-2" />
              <p>Sin proyectos</p>
            </div>
          )}
        </nav>
        <div className="px-3 pb-3 pt-1 flex-shrink-0">
          <button
            type="button"
            onClick={() => setSettingsOpen(true)}
            aria-label="Configuración"
            title="Configuración"
            className="w-full flex h-9 items-center gap-2.5 px-3 rounded-md text-sm text-fog hover:text-paper hover:bg-obsidian transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40"
          >
            <Settings className="h-4 w-4 flex-shrink-0" />
            <span className="truncate">Configuración</span>
          </button>
        </div>
        <SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />
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
            canDrag
            onSelect={() => {}}
            floating
          />
        </div>
      )}
    </aside>
  );
}
