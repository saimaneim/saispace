import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type {
  ActivityEvent,
  CreateProjectInput,
  DirectoryListing,
  Memory,
  MemoryKind,
  Project,
  UpdateMemoryInput,
  UpdateProjectInput,
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
  /** Directory tree for the picker: absolute path -> child folder names. */
  directories?: Record<string, string[]>;
  home?: string;
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
    context_notes: "",
    pinned: false,
    sort_order: 0,
    created_at: now(),
    updated_at: now(),
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
    ...overrides,
  };
}

export function installFakeBackend(options: FakeBackendOptions = {}) {
  const home = options.home ?? "/home/test";
  const projects: Project[] = [...(options.projects ?? [])];
  const memories: Memory[] = [...(options.memories ?? [])];
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

  const handlers: Record<string, Handler> = {
    // Projects
    list_projects: () => [...projects].sort((a, b) => a.sort_order - b.sort_order),
    get_project: ({ id }) => projects.find((p) => p.id === id) ?? null,
    create_project: ({ input }) => {
      const { name, path, context_notes } = input as CreateProjectInput;
      if (!name.trim()) throw new CommandError("El nombre no puede estar vacío");
      if (projects.some((p) => p.path === path)) {
        throw new CommandError(`Ya existe un proyecto en ${path}`, "conflict");
      }
      const project = makeProject({
        name,
        path,
        context_notes: context_notes ?? "",
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
    reorder_projects: () => undefined,

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
    list_tasks: () => [],
    list_activity: ({ limit }) => activity.slice(0, (limit as number | undefined) ?? 60),
    list_activity_by_project: ({ projectId }) => activity.filter((a) => a.project_id === projectId),
    list_secret_grants: () => [],
    list_context_files: () => [],
    list_technologies: () => [],
    list_project_tech_rules: () => [],
    search_memory: () => [],
    tech_icon: () => null,

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
