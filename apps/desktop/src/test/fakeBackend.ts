import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type {
  ActivityEvent,
  AiInventory,
  ContextFile,
  CreateProjectInput,
  CreateTaskInput,
  DirectoryListing,
  Goal,
  Memory,
  MemoryKind,
  Plan,
  PlanFilter,
  Project,
  Task,
  Technology,
  TechRule,
  TimelineDay,
  TimelineItem,
  UpdateGoalInput,
  UpdateMemoryInput,
  UpdateProjectInput,
  UpdateTaskInput,
} from "../lib/types";

/**
 * In-memory stand-in for the Rust side of the Tauri bridge.
 *
 * It answers the same `invoke` commands as `src-tauri`, keeps state between
 * calls (so a write can be read back), and lets a test play the role of an
 * agent or the `sai` CLI writing to the shared database and then emitting
 * `saispace:activity`, like the real file watcher does.
 */
export interface FakeBackendOptions {
  projects?: Project[];
  memories?: Memory[];
  tasks?: Task[];
  goals?: Goal[];
  /** Directory tree for the picker: absolute path -> child folder names. */
  directories?: Record<string, string[]>;
  home?: string;
  /** Triggers by memory id (`memory_triggers`). */
  triggers?: Record<string, string[]>;
  contextFiles?: ContextFile[];
  contextContents?: Record<string, string>;
  technologies?: Technology[];
  techRules?: TechRule[];
  inventory?: Partial<AiInventory>;
  /** What `git log` and tags give `project_timeline`, by project id. */
  gitHistory?: Record<string, TimelineItem[]>;
}

export interface InvokeCall {
  cmd: string;
  args: Record<string, unknown> | undefined;
}

type Handler = (args: Record<string, unknown>) => unknown;

export class CommandError {
  constructor(
    public message: string,
    public kind = "invalid_input",
  ) {}
}

const now = () => new Date().toISOString();
let seq = 0;
const nextId = (prefix: string) => `${prefix}-${++seq}`;

export function makeProject(overrides: Partial<Project> = {}): Project {
  const id = overrides.id ?? nextId("proj");
  return {
    id,
    name: overrides.name ?? id,
    path: overrides.path ?? `/home/test/${id}`,
    status: "active",
    pinned: false,
    sort_order: 0,
    created_at: now(),
    updated_at: now(),
    ...overrides,
  };
}

export function makeTask(overrides: Partial<Task> = {}): Task {
  const timestamp = now();
  return {
    id: nextId("task"),
    project_id: "proj-1",
    title: "Tarea",
    status: "todo",
    priority: "medium",
    goal_id: null,
    created_at: timestamp,
    updated_at: timestamp,
    completed_at: null,
    ...overrides,
  };
}

export function makeGoal(overrides: Partial<Goal> = {}): Goal {
  const timestamp = now();
  return {
    id: nextId("goal"),
    project_id: "proj-1",
    title: "Meta",
    criteria: null,
    status: "active",
    position: 0,
    created_at: timestamp,
    updated_at: timestamp,
    completed_at: null,
    ...overrides,
  };
}

export function makeMemory(overrides: Partial<Memory> = {}): Memory {
  return {
    id: nextId("mem"),
    project_id: null,
    project_name: null,
    kind: "note" as MemoryKind,
    title: "Memoria",
    content: "",
    topic_key: null,
    pinned: false,
    created_at: now(),
    updated_at: now(),
    last_used_at: null,
    use_count: 0,
    superseded_by: null,
    ...overrides,
  };
}

export function makeContextFile(overrides: Partial<ContextFile> = {}): ContextFile {
  const path = overrides.path ?? "AGENTS.md";
  return {
    path,
    name: overrides.name ?? path.split("/").pop() ?? path,
    kind: overrides.kind ?? "agents",
    size: overrides.size ?? 100,
    modified_at: overrides.modified_at ?? now(),
    link: overrides.link ?? null,
    ...overrides,
  };
}

export function makeTechnology(overrides: Partial<Technology> = {}): Technology {
  const name = overrides.name ?? "react";
  return {
    id: overrides.id ?? nextId("tech"),
    project_id: overrides.project_id ?? "proj-1",
    name,
    version: overrides.version ?? "19.0.0",
    ecosystem: overrides.ecosystem ?? "npm",
    source: overrides.source ?? "package.json",
    docs_url: overrides.docs_url ?? `https://example.com/${name}`,
    detected_at: overrides.detected_at ?? now(),
    ...overrides,
  };
}

export function makeTimelineItem(overrides: Partial<TimelineItem> = {}): TimelineItem {
  return {
    kind: "commit",
    at: now(),
    title: "Cambio",
    detail: null,
    label: null,
    goal: null,
    link: null,
    hash: null,
    pr: null,
    files: [],
    file_count: overrides.files?.length ?? 0,
    ...overrides,
  };
}

const localDay = (iso: string) => {
  const date = new Date(iso);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
};

const KIND_ORDER: TimelineItem["kind"][] = [
  "goal_completed",
  "release",
  "task_completed",
  "commit",
  "memory",
];

export function installFakeBackend(options: FakeBackendOptions = {}) {
  const home = options.home ?? "/home/test";
  const projects: Project[] = [...(options.projects ?? [])];
  const memories: Memory[] = [...(options.memories ?? [])];
  const tasks: Task[] = [...(options.tasks ?? [])];
  const goals: Goal[] = [...(options.goals ?? [])];
  const contextFiles: ContextFile[] = [...(options.contextFiles ?? [])];
  const contextContents: Record<string, string> = { ...(options.contextContents ?? {}) };
  const technologies: Technology[] = [...(options.technologies ?? [])];
  const techRules: TechRule[] = [...(options.techRules ?? [])];
  const activity: ActivityEvent[] = [];
  const directories: Record<string, string[]> = options.directories ?? { [home]: [] };
  const calls: InvokeCall[] = [];
  const failures = new Map<string, CommandError>();

  const record = (event: Omit<ActivityEvent, "id" | "created_at">) => {
    activity.unshift({ ...event, id: nextId("act"), created_at: now() });
  };

  const findProject = (id: unknown) => {
    const project = projects.find((p) => p.id === id);
    if (!project) throw new CommandError(`Proyecto no encontrado: ${id}`, "not_found");
    return project;
  };

  const findMemory = (id: unknown) => {
    const memory = memories.find((m) => m.id === id);
    if (!memory) throw new CommandError(`Memoria no encontrada: ${id}`, "not_found");
    return memory;
  };

  const findGoal = (id: unknown) => {
    const goal = goals.find((item) => item.id === id);
    if (!goal) throw new CommandError(`Meta no encontrada: ${id}`, "not_found");
    return goal;
  };

  const findTask = (id: unknown) => {
    const task = tasks.find((item) => item.id === id);
    if (!task) throw new CommandError(`Tarea no encontrada: ${id}`, "not_found");
    return task;
  };

  // Mirrors `Db::project_timeline`: finished goals and tasks plus decisions
  // come from the shared state, git entries from `options.gitHistory`.
  const buildTimeline = (projectId: unknown): TimelineDay[] => {
    findProject(projectId);
    const goalTitle = (id: string | null) => goals.find((goal) => goal.id === id)?.title ?? null;
    const items: TimelineItem[] = [
      ...goals
        .filter((g) => g.project_id === projectId && g.status === "completed" && g.completed_at)
        .map((g) =>
          makeTimelineItem({
            kind: "goal_completed",
            at: g.completed_at as string,
            title: g.title,
            detail: g.criteria,
          }),
        ),
      ...tasks
        .filter((t) => t.project_id === projectId && t.status === "done" && t.completed_at)
        .map((t) =>
          makeTimelineItem({
            kind: "task_completed",
            at: t.completed_at as string,
            title: t.title,
            goal: goalTitle(t.goal_id),
          }),
        ),
      ...memories
        .filter(
          (m) =>
            m.project_id === projectId &&
            (m.kind === "decision" || m.kind === "architecture") &&
            !m.superseded_by,
        )
        .map((m) =>
          makeTimelineItem({
            kind: "memory",
            at: m.created_at,
            title: m.title,
            label: m.kind === "decision" ? "Decisión" : "Arquitectura",
          }),
        ),
      ...(options.gitHistory?.[String(projectId)] ?? []),
    ].sort(
      (a, b) => b.at.localeCompare(a.at) || KIND_ORDER.indexOf(a.kind) - KIND_ORDER.indexOf(b.kind),
    );
    const days: TimelineDay[] = [];
    for (const item of items) {
      const date = localDay(item.at);
      const last = days.at(-1);
      if (last?.date === date) last.items.push(item);
      else days.push({ date, items: [item] });
    }
    return days;
  };

  const buildPlan = (projectId: unknown, filter: PlanFilter): Plan => {
    findProject(projectId);
    const projectGoals = goals
      .filter((goal) => goal.project_id === projectId)
      .sort((a, b) => a.position - b.position);
    const visibleGoals = projectGoals.filter((goal) => {
      if (filter === "all") return true;
      return goal.status === filter;
    });

    return {
      goals: visibleGoals.map((goal) => {
        const goalTasks = tasks
          .filter((task) => task.goal_id === goal.id)
          .map((task) => ({ ...task }));
        return {
          goal: { ...goal },
          done: goalTasks.filter((task) => task.status === "done").length,
          total: goalTasks.length,
          tasks: goalTasks,
        };
      }),
      loose_tasks: tasks
        .filter((task) => task.project_id === projectId && task.goal_id === null)
        .map((task) => ({ ...task })),
      counts: {
        active: projectGoals.filter((goal) => goal.status === "active").length,
        completed: projectGoals.filter((goal) => goal.status === "completed").length,
        all: projectGoals.length,
      },
    };
  };

  const handlers: Record<string, Handler> = {
    // Projects
    list_projects: () =>
      [...projects].sort(
        (a, b) => Number(b.pinned) - Number(a.pinned) || a.sort_order - b.sort_order,
      ),
    get_project: ({ id }) => projects.find((p) => p.id === id) ?? null,
    create_project: ({ input }) => {
      const { name, path } = input as CreateProjectInput;
      if (!name.trim()) throw new CommandError("El nombre no puede estar vacío");
      if (projects.some((p) => p.path === path)) {
        throw new CommandError(`Ya existe un proyecto en ${path}`, "conflict");
      }
      const project = makeProject({
        name,
        path,
        sort_order: projects.length,
      });
      projects.push(project);
      record({
        actor: { type: "user" },
        action: "create",
        entity: "project",
        project_id: project.id,
      });
      return project;
    },
    update_project: ({ id, input }) => {
      const project = findProject(id);
      Object.assign(project, input as UpdateProjectInput, { updated_at: now() });
      return project;
    },
    delete_project: ({ id }) => {
      findProject(id);
      projects.splice(
        projects.findIndex((p) => p.id === id),
        1,
      );
    },
    set_project_pinned: ({ id, pinned }) => {
      const project = findProject(id);
      project.pinned = Boolean(pinned);
      return project;
    },
    reorder_projects: ({ ids }) => {
      (ids as string[]).forEach((id, index) => {
        findProject(id).sort_order = index + 1;
      });
    },
    project_favicon: () => null,

    // Plan and tasks
    plan_get: ({ projectId, filter }) =>
      buildPlan(projectId, (filter as PlanFilter | undefined) ?? "active"),
    goal_create: ({ projectId, title: titleValue, criteria: criteriaValue, tasks: taskValues }) => {
      findProject(projectId);
      const title = String(titleValue).trim();
      if (!title) throw new CommandError("El título no puede estar vacío");
      const criteria =
        typeof criteriaValue === "string" && criteriaValue.trim() ? criteriaValue.trim() : null;
      const siblings = goals.filter((goal) => goal.project_id === projectId);
      const goal = makeGoal({
        project_id: String(projectId),
        title,
        criteria,
        position: siblings.length ? Math.max(...siblings.map((item) => item.position)) + 1 : 0,
      });
      goals.push(goal);
      const createdTasks = (taskValues as string[]).map((taskTitle) =>
        makeTask({
          project_id: String(projectId),
          goal_id: goal.id,
          title: taskTitle,
        }),
      );
      tasks.push(...createdTasks);
      return {
        goal: { ...goal },
        done: 0,
        total: createdTasks.length,
        tasks: createdTasks.map((task) => ({ ...task })),
      };
    },
    goal_update: ({ id, input }) => {
      const goal = findGoal(id);
      const patch = input as UpdateGoalInput;
      if (patch.title !== undefined) {
        if (!patch.title.trim()) throw new CommandError("El título no puede estar vacío");
        goal.title = patch.title.trim();
      }
      if (patch.criteria !== undefined) goal.criteria = patch.criteria?.trim() || null;
      if (patch.position !== undefined) goal.position = patch.position;
      if (patch.status !== undefined) {
        goal.status = patch.status;
        if (patch.status === "completed") goal.completed_at = now();
        if (patch.status === "active") goal.completed_at = null;
      }
      goal.updated_at = now();
      return { ...goal };
    },
    goal_delete: ({ id }) => {
      findGoal(id);
      for (const task of tasks) {
        if (task.goal_id === id) {
          task.goal_id = null;
          task.updated_at = now();
        }
      }
      goals.splice(
        goals.findIndex((goal) => goal.id === id),
        1,
      );
    },
    list_tasks: ({ projectId }) => {
      findProject(projectId);
      return tasks.filter((task) => task.project_id === projectId).map((task) => ({ ...task }));
    },
    create_task: ({ input }) => {
      const create = input as CreateTaskInput;
      findProject(create.project_id);
      const title = create.title.trim();
      if (!title) throw new CommandError("El título no puede estar vacío");
      const goalId = create.goal_id ?? null;
      if (goalId && findGoal(goalId).project_id !== create.project_id) {
        throw new CommandError("La meta pertenece a otro proyecto", "invalid_input");
      }
      const task = makeTask({
        project_id: create.project_id,
        title,
        priority: create.priority ?? "medium",
        goal_id: goalId,
      });
      tasks.push(task);
      return { ...task };
    },
    update_task: ({ id, input }) => {
      const task = findTask(id);
      const patch = input as UpdateTaskInput;
      if (patch.title !== undefined) {
        if (!patch.title.trim()) throw new CommandError("El título no puede estar vacío");
        task.title = patch.title.trim();
      }
      if (patch.priority !== undefined) task.priority = patch.priority;
      if (patch.status !== undefined) {
        task.status = patch.status;
        task.completed_at = patch.status === "done" ? now() : null;
      }
      task.updated_at = now();
      return { ...task };
    },
    delete_task: ({ id }) => {
      findTask(id);
      tasks.splice(
        tasks.findIndex((task) => task.id === id),
        1,
      );
    },

    project_timeline: ({ projectId }) => buildTimeline(projectId),

    // Directory picker
    list_directory: ({ path }) => {
      const current = (path as string | undefined) ?? home;
      const children = directories[current];
      if (!children) throw new CommandError(`No existe la carpeta ${current}`, "not_found");
      const parent = current === "/" ? null : current.replace(/\/[^/]+$/, "") || "/";
      const listing: DirectoryListing = {
        current_path: current,
        display_path: current.replace(home, "~"),
        parent_path: parent,
        entries: children.map((name) => ({
          name,
          path: `${current}/${name}`,
          is_dir: true,
        })),
      };
      return listing;
    },

    // Memory
    list_memories: ({ projectId, limit }) =>
      memories
        .filter((m) => projectId == null || m.project_id === projectId)
        .sort((a, b) => b.updated_at.localeCompare(a.updated_at))
        .slice(0, (limit as number | undefined) ?? 200),
    get_memory: ({ id }) => findMemory(id),
    memory_links: () => [],
    memory_triggers: (args) => options.triggers?.[String(args.id)] ?? [],
    update_memory: ({ id, input }) => {
      const memory = findMemory(id);
      const patch = input as UpdateMemoryInput;
      if (patch.title !== undefined && !patch.title.trim()) {
        throw new CommandError("El título no puede estar vacío");
      }
      Object.assign(memory, patch, {
        topic_key: patch.topic_key === undefined ? memory.topic_key : patch.topic_key || null,
        updated_at: now(),
      });
      return { ...memory };
    },
    set_memory_pinned: ({ id, pinned }) => {
      const memory = findMemory(id);
      memory.pinned = Boolean(pinned);
      return { ...memory };
    },
    delete_memory: ({ id }) => {
      findMemory(id);
      memories.splice(
        memories.findIndex((m) => m.id === id),
        1,
      );
    },
    memory_duplicates: () => [],
    memory_stale: () => ({ stale: [], surplus_sessions: [] }),

    // Everything else the shell polls on mount.
    list_activity: ({ limit }) => activity.slice(0, (limit as number | undefined) ?? 60),
    list_activity_by_project: ({ projectId }) => activity.filter((a) => a.project_id === projectId),
    list_secret_grants: () => [],
    list_context_files: () => contextFiles,
    read_context_file: ({ relativePath }) => contextContents[String(relativePath)] ?? "",
    write_context_file: ({ relativePath, content }) => {
      const path = String(relativePath);
      const text = String(content);
      contextContents[path] = text;
      const idx = contextFiles.findIndex((f) => f.path === path);
      const file: ContextFile = {
        path,
        name: path.split("/").pop() || path,
        // Saving keeps the class the listing gave the file.
        kind: contextFiles[idx]?.kind ?? "other",
        size: text.length,
        modified_at: now(),
      };
      if (idx >= 0) contextFiles[idx] = file;
      else contextFiles.push(file);
      return file;
    },
    list_technologies: () => technologies,
    sync_project_stack: () => technologies,
    list_project_tech_rules: () => techRules,
    search_memory: ({ query }) =>
      memories
        .filter((m) => m.title.toLowerCase().includes(String(query).toLowerCase()))
        .map((m) => ({
          kind: "memory",
          id: m.id,
          project_id: m.project_id,
          title: m.title,
          snippet: m.content,
          rank: 0,
        })),
    open_url: () => undefined,
    tech_icon: () => null,
    memory_usage_stats: () => ({
      saves_by_agent: [],
      most_used: [],
      never_used: 0,
      total: memories.length,
      by_project: [],
    }),
    global_ai_inventory: () => ({
      instructions: [],
      skills: [],
      mcp_servers: [],
      plugins: [],
      hooks: [],
      warnings: [],
      ...options.inventory,
    }),

    // Window plugin (WindowControls).
    "plugin:window|is_maximized": () => false,
  };

  (globalThis as { isTauri?: boolean }).isTauri = true;
  mockWindows("main");
  mockIPC(
    (cmd, payload) => {
      const args = payload as Record<string, unknown> | undefined;
      calls.push({ cmd, args });
      const failure = failures.get(cmd);
      if (failure) {
        failures.delete(cmd);
        throw failure;
      }
      const handler = handlers[cmd];
      if (!handler) {
        throw new CommandError(`fakeBackend: comando sin mock '${cmd}'`, "unmocked");
      }
      return handler(args ?? {});
    },
    { shouldMockEvents: true },
  );

  return {
    projects,
    memories,
    tasks,
    goals,
    contextFiles,
    contextContents,
    technologies,
    techRules,
    calls,
    /** Calls to one command, in order. */
    callsTo: (cmd: string) => calls.filter((c) => c.cmd === cmd),
    /** Make the next call to `cmd` reject with a CommandError. */
    failNext: (cmd: string, message: string, kind?: string) => {
      failures.set(cmd, new CommandError(message, kind));
    },
    /**
     * Simulate an agent or `sai memory save` writing a memory straight to the
     * database, then the watcher notifying the window.
     */
    async agentSavesMemory(memory: Partial<Memory>, agent = "claude") {
      const existing = memory.topic_key
        ? memories.find(
            (m) => m.topic_key === memory.topic_key && m.project_id === (memory.project_id ?? null),
          )
        : undefined;
      if (existing) {
        Object.assign(existing, memory, { updated_at: now() });
      } else {
        memories.push(makeMemory(memory));
      }
      record({
        actor: { type: "agent", name: agent },
        action: existing ? "update" : "create",
        entity: "context_notes",
        project_id: memory.project_id ?? null,
      });
      await emit("saispace:activity");
    },
  };
}

export type FakeBackend = ReturnType<typeof installFakeBackend>;
