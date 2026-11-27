import {
  AlertCircle,
  Boxes,
  Brain,
  FileText,
  FolderOpen,
  History,
  ListTodo,
  PanelLeft,
  Search,
  X,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { ActivityFeed } from "./components/ActivityFeed";
import { ContextTab } from "./components/ContextTab";
import { DirectoryPickerModal } from "./components/DirectoryPickerModal";
import { GlobalView } from "./components/GlobalView";
import { HistoryTab } from "./components/HistoryTab";
import { MemoryTab } from "./components/MemoryTab";
import { PlanTab } from "./components/PlanTab";
import { ProjectHeader } from "./components/ProjectHeader";
import { SearchDialog } from "./components/SearchDialog";
import { Sidebar } from "./components/Sidebar";
import { TechnologiesTab } from "./components/TechnologiesTab";
import { WindowControls } from "./components/WindowControls";
import { api, formatError } from "./lib/api";
import { type DefaultTabId, readDefaultTab } from "./lib/preferences";
import type { ActivityEvent, Project, ProjectStatus, SearchHit, Task } from "./lib/types";
import { cn } from "./lib/utils";

type TabId = DefaultTabId;

const isOpenTask = (task: Task) => task.status !== "done" && task.status !== "cancelled";

const FEED_STORAGE_KEY = "saispace:feed-open-v2";
const SIDEBAR_STORAGE_KEY = "saispace:sidebar-open";

function readFlag(key: string, fallback: boolean): boolean {
  try {
    const value = localStorage.getItem(key);
    return value === null ? fallback : value === "1";
  } catch {
    return fallback;
  }
}

export function App() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const [tasksByProject, setTasksByProject] = useState<Record<string, Task[]>>({});
  const [activities, setActivities] = useState<ActivityEvent[]>([]);
  const [activeTab, setActiveTab] = useState<TabId>(readDefaultTab);
  const [isFeedOpen, setIsFeedOpen] = useState(() => readFlag(FEED_STORAGE_KEY, false));
  const [isSidebarOpen, setIsSidebarOpen] = useState(() => readFlag(SIDEBAR_STORAGE_KEY, true));
  const [isPickerOpen, setIsPickerOpen] = useState(false);
  const [isSearchOpen, setIsSearchOpen] = useState(false);
  const [focusMemoryId, setFocusMemoryId] = useState<string | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [isGlobal, setIsGlobal] = useState(false);
  const [pendingRequests, setPendingRequests] = useState(0);

  const activeProject = projects.find((p) => p.id === selectedProjectId) ?? projects[0] ?? null;
  const activeProjectId = activeProject?.id ?? null;
  const tasks = activeProjectId ? (tasksByProject[activeProjectId] ?? []) : [];

  const reportError = useCallback((err: unknown) => setErrorMessage(formatError(err)), []);

  // Activity events can fire several refreshes at once: only the newest one
  // may write state, or a slow older response would roll the UI back.
  const refreshSeq = useRef(0);
  const refreshAll = useCallback(async () => {
    const seq = ++refreshSeq.current;
    try {
      const [list, events] = await Promise.all([api.listProjects(), api.listActivity(60)]);
      const taskLists = await Promise.all(list.map((p) => api.listTasks(p.id)));
      if (seq !== refreshSeq.current) return;
      setProjects(list);
      setActivities(events);
      setTasksByProject(Object.fromEntries(list.map((p, i) => [p.id, taskLists[i]])));
      setSelectedProjectId((current) =>
        current && list.some((p) => p.id === current) ? current : (list[0]?.id ?? null),
      );
    } catch (err) {
      reportError(err);
    }
  }, [reportError]);

  const projectsRef = useRef(projects);
  projectsRef.current = projects;

  // Drag and drop: moves a project to a new slot, pinning or unpinning it
  // when it crosses groups. The list updates first so the drop feels instant.
  const moveProject = useCallback(
    async (id: string, target: { pinned: boolean; status: ProjectStatus }, ids: string[]) => {
      const { pinned, status } = target;
      const current = projectsRef.current.find((p) => p.id === id);
      const pinChanged = current?.pinned !== pinned;
      const statusChanged = current?.status !== status;
      setProjects((prev) => {
        const byId = new Map(prev.map((p) => [p.id, p.id === id ? { ...p, pinned, status } : p]));
        const ordered = ids.flatMap((pid) => byId.get(pid) ?? []);
        return [...ordered, ...[...byId.values()].filter((p) => !ids.includes(p.id))];
      });
      try {
        if (pinChanged) await api.setProjectPinned(id, pinned);
        if (statusChanged) await api.updateProject(id, { status });
        await api.reorderProjects(ids);
      } catch (err) {
        reportError(err);
        refreshAll();
      }
    },
    [reportError, refreshAll],
  );

  const refreshPending = useCallback(async () => {
    try {
      const grants = await api.listSecretGrants();
      setPendingRequests(grants.filter((g) => g.status === "pending").length);
    } catch {
      setPendingRequests(0);
    }
  }, []);

  useEffect(() => {
    refreshPending();
    const timer = window.setInterval(refreshPending, 5000);
    return () => window.clearInterval(timer);
  }, [refreshPending]);

  const selectProject = useCallback((id: string) => {
    setIsGlobal(false);
    setSelectedProjectId(id);
  }, []);

  const refreshRef = useRef(refreshAll);
  refreshRef.current = refreshAll;

  useEffect(() => {
    refreshRef.current();
    const unlistenPromise = api.onActivityChanged(() => {
      refreshRef.current();
    });
    return () => {
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, []);

  useEffect(() => {
    try {
      localStorage.setItem(FEED_STORAGE_KEY, isFeedOpen ? "1" : "0");
    } catch {
      return;
    }
  }, [isFeedOpen]);

  useEffect(() => {
    try {
      localStorage.setItem(SIDEBAR_STORAGE_KEY, isSidebarOpen ? "1" : "0");
    } catch {
      return;
    }
  }, [isSidebarOpen]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey)) return;
      const key = e.key.toLowerCase();
      if (key === "b") {
        e.preventDefault();
        setIsSidebarOpen((open) => !open);
      } else if (key === "k") {
        e.preventDefault();
        setIsSearchOpen(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const handleSelectHit = (hit: SearchHit) => {
    if (hit.project_id) selectProject(hit.project_id);
    setFocusMemoryId(hit.kind === "memory" ? hit.id : null);
    const target: Record<SearchHit["kind"], TabId> = {
      task: "plan",
      tech_rule: "tech",
      memory: "memory",
      project: "context",
    };
    setActiveTab(target[hit.kind]);
  };

  const run = useCallback(
    async (action: () => Promise<unknown>) => {
      setErrorMessage(null);
      try {
        await action();
        await refreshAll();
      } catch (err) {
        reportError(err);
      }
    },
    [refreshAll, reportError],
  );

  const handleSelectDirectory = async (folderPath: string) => {
    setErrorMessage(null);
    const existing = projects.find((p) => p.path === folderPath);
    if (existing) {
      selectProject(existing.id);
      return;
    }
    const derivedName =
      folderPath
        .replace(/[/\\]+$/, "")
        .split(/[/\\]/)
        .pop() || "proyecto";
    try {
      const created = await api.createProject({ name: derivedName, path: folderPath });
      selectProject(created.id);
      await refreshAll();
    } catch (err) {
      reportError(err);
    }
  };

  const handleDeleteProject = (id: string) => run(() => api.deleteProject(id));

  const openTasksCount = tasks.filter(isOpenTask).length;

  const tabs: { id: TabId; label: string; icon: typeof ListTodo; count?: number }[] = [
    { id: "context", label: "Contexto", icon: FileText },
    { id: "plan", label: "Plan", icon: ListTodo, count: openTasksCount },
    { id: "history", label: "Historial", icon: History },
    { id: "tech", label: "Tecnologías", icon: Boxes },
    { id: "memory", label: "Memoria", icon: Brain },
  ];

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-void text-mist font-sans select-none">
      <Sidebar
        projects={projects}
        selectedProjectId={isGlobal ? null : (activeProject?.id ?? null)}
        onSelectProject={selectProject}
        isGlobalSelected={isGlobal}
        onSelectGlobal={() => setIsGlobal(true)}
        pendingRequests={pendingRequests}
        onOpenDirectoryPicker={() => setIsPickerOpen(true)}
        onMove={moveProject}
        isOpen={isSidebarOpen}
      />

      <div className="flex-1 flex flex-col min-w-0 min-h-0">
        <header
          data-tauri-drag-region
          className="h-12 flex-shrink-0 flex items-center gap-3 pl-3 bg-void"
        >
          <button
            type="button"
            onClick={() => setIsSidebarOpen((open) => !open)}
            aria-label={isSidebarOpen ? "Ocultar barra lateral" : "Mostrar barra lateral"}
            title="Barra lateral (Ctrl+B)"
            className="h-8 w-8 flex-shrink-0 rounded-md flex items-center justify-center text-fog hover:text-paper hover:bg-obsidian transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40 pointer-events-auto"
          >
            <PanelLeft className="w-4 h-4" />
          </button>
          {isGlobal ? (
            <div className="flex items-baseline gap-3 min-w-0 flex-1 pointer-events-none">
              <h1 className="text-sm font-semibold text-paper">Global</h1>
            </div>
          ) : activeProject ? (
            <ProjectHeader
              project={activeProject}
              onDeleteProject={() => handleDeleteProject(activeProject.id)}
              isFeedOpen={isFeedOpen}
              onToggleFeed={() => setIsFeedOpen((open) => !open)}
              onMove={(target) =>
                void moveProject(
                  activeProject.id,
                  target,
                  projects.map((p) => p.id),
                )
              }
            />
          ) : (
            <div className="flex-1 pointer-events-none" />
          )}
          <button
            type="button"
            onClick={() => setIsSearchOpen(true)}
            aria-label="Buscar"
            title="Buscar (Ctrl+K)"
            className="h-8 w-8 flex-shrink-0 rounded-md flex items-center justify-center text-fog hover:text-paper hover:bg-obsidian transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40 pointer-events-auto"
          >
            <Search className="w-4 h-4" />
          </button>
          <WindowControls />
        </header>

        <div className="flex flex-1 min-h-0">
          <main className="flex-1 flex flex-col min-w-0 min-h-0 bg-void">
            {errorMessage && (
              <div
                className="mx-6 mb-2 flex items-center justify-between gap-2 px-3 h-10 rounded-md bg-coral-red/10 text-coral-red text-sm flex-shrink-0"
                role="alert"
              >
                <div className="flex items-center gap-2 min-w-0 flex-1">
                  <AlertCircle className="w-4 h-4 flex-shrink-0" />
                  <span className="truncate">{errorMessage}</span>
                </div>
                <button
                  type="button"
                  onClick={() => setErrorMessage(null)}
                  aria-label="Cerrar notificación de error"
                  className="p-1 rounded hover:bg-coral-red/20 transition-colors focus-visible:ring-1 focus-visible:ring-coral-red focus:outline-none"
                >
                  <X className="w-4 h-4" />
                </button>
              </div>
            )}

            {isGlobal ? (
              <GlobalView
                refreshKey={activities[0]?.id ?? null}
                pendingRequests={pendingRequests}
                onSecretsChanged={refreshPending}
              />
            ) : activeProject ? (
              <>
                <div className="flex items-center gap-4 px-6 h-12 flex-shrink-0">
                  <div
                    role="tablist"
                    aria-label="Secciones del proyecto"
                    className="flex items-center gap-1"
                  >
                    {tabs.map(({ id, label, icon: Icon, count }) => {
                      const selected = activeTab === id;
                      return (
                        <button
                          key={id}
                          type="button"
                          role="tab"
                          aria-selected={selected}
                          aria-label={label}
                          title={label}
                          onClick={() => setActiveTab(id)}
                          className={cn(
                            "h-9 px-3 rounded-md text-sm font-medium flex items-center gap-2 transition-colors duration-[120ms] focus-visible:ring-2 focus-visible:ring-accent/40 focus:outline-none",
                            selected
                              ? "bg-elevated text-paper font-medium"
                              : "text-fog hover:text-mist hover:bg-obsidian",
                          )}
                        >
                          <Icon className="w-4 h-4" />
                          <span className="hidden lg:inline">{label}</span>
                          {count !== undefined && count > 0 && (
                            <span className="text-xs tabular-nums text-fog">{count}</span>
                          )}
                        </button>
                      );
                    })}
                  </div>
                </div>

                <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
                  {activeTab === "plan" && (
                    <PlanTab
                      key={activeProject.id}
                      project={activeProject}
                      refreshKey={activities[0]?.id ?? null}
                      onError={reportError}
                      onChanged={refreshAll}
                    />
                  )}
                  {activeTab === "history" && (
                    <HistoryTab
                      key={activeProject.id}
                      project={activeProject}
                      refreshKey={activities[0]?.id ?? null}
                      onError={reportError}
                    />
                  )}
                  {activeTab === "memory" && (
                    <MemoryTab
                      key={activeProject.id}
                      project={activeProject}
                      refreshKey={activities[0]?.id ?? null}
                      focusId={focusMemoryId}
                    />
                  )}
                  {activeTab === "tech" && (
                    <TechnologiesTab key={activeProject.id} project={activeProject} />
                  )}
                  {activeTab === "context" && (
                    <ContextTab key={activeProject.id} project={activeProject} />
                  )}
                </div>
              </>
            ) : (
              <div className="flex-1 flex flex-col items-center justify-center text-center gap-2 px-6">
                <div className="w-12 h-12 rounded-xl bg-obsidian flex items-center justify-center text-ash mb-2">
                  <FolderOpen className="w-6 h-6" />
                </div>
                <h2 className="text-base font-semibold text-paper">Sin proyectos</h2>
                <p className="text-sm text-fog max-w-xs leading-relaxed">
                  Usa el botón + de la barra lateral para añadir una carpeta de trabajo.
                </p>
              </div>
            )}
          </main>

          {isFeedOpen && activeProject && !isGlobal && (
            <aside className="w-80 flex-shrink-0 bg-carbon flex flex-col min-h-0">
              <ActivityFeed activities={activities} />
            </aside>
          )}
        </div>
      </div>

      <SearchDialog
        open={isSearchOpen}
        onOpenChange={setIsSearchOpen}
        activeProjectId={activeProjectId}
        onSelectHit={handleSelectHit}
      />

      <DirectoryPickerModal
        isOpen={isPickerOpen}
        onClose={() => setIsPickerOpen(false)}
        onSelect={handleSelectDirectory}
      />
    </div>
  );
}

export default App;
