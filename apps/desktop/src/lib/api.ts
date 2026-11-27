import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ActivityEvent,
  AgentContext,
  AiInventory,
  ContextFile,
  CreateProjectInput,
  CreateTaskInput,
  DirectoryListing,
  DuplicatePair,
  Goal,
  GoalWithTasks,
  Memory,
  MemoryLink,
  MemoryUsageStats,
  OrphanGroup,
  Plan,
  PlanFilter,
  Project,
  SearchHit,
  SecretGrant,
  SecretMeta,
  StaleReport,
  Task,
  Technology,
  TechRule,
  UpdateGoalInput,
  UpdateMemoryInput,
  UpdateProjectInput,
  UpdateTaskInput,
  UpsertTechRuleInput,
} from "./types";

export interface CommandErrorPayload {
  message: string;
  kind?: string;
}

export function formatError(err: unknown): string {
  if (typeof err === "object" && err !== null && "message" in err) {
    return String((err as CommandErrorPayload).message);
  }
  if (typeof err === "string") {
    return err;
  }
  return "Error inesperado";
}

async function invokeOrFallback<T>(
  cmd: string,
  args?: Record<string, unknown>,
  fallback?: T,
): Promise<T> {
  if (isTauri()) {
    return invoke<T>(cmd, args);
  }
  if (fallback !== undefined) {
    return fallback;
  }
  throw new Error(`Entorno web sin Tauri: comando '${cmd}' no disponible`);
}

export const api = {
  isAvailable: isTauri,

  listDirectory: (path?: string, showHidden?: boolean) =>
    invokeOrFallback<DirectoryListing>("list_directory", {
      path,
      showHidden,
    }),

  listProjects: () => invokeOrFallback<Project[]>("list_projects", undefined, []),

  getProject: (id: string) => invokeOrFallback<Project | null>("get_project", { id }, null),

  createProject: (input: CreateProjectInput) =>
    invokeOrFallback<Project>("create_project", { input }),

  updateProject: (id: string, input: UpdateProjectInput) =>
    invokeOrFallback<Project>("update_project", { id, input }),

  deleteProject: (id: string) => invokeOrFallback<void>("delete_project", { id }),

  setProjectPinned: (id: string, pinned: boolean) =>
    invokeOrFallback<Project>("set_project_pinned", { id, pinned }),

  reorderProjects: (ids: string[]) =>
    invokeOrFallback<void>("reorder_projects", { ids }, undefined),

  getPlan: (projectId: string, filter: PlanFilter) =>
    invokeOrFallback<Plan>(
      "plan_get",
      { projectId, filter },
      { goals: [], loose_tasks: [], counts: { active: 0, completed: 0, all: 0 } },
    ),

  createGoal: (projectId: string, title: string, criteria: string | null, tasks: string[]) =>
    invokeOrFallback<GoalWithTasks>("goal_create", { projectId, title, criteria, tasks }),

  updateGoal: (id: string, input: UpdateGoalInput) =>
    invokeOrFallback<Goal>("goal_update", { id, input }),

  deleteGoal: (id: string) => invokeOrFallback<void>("goal_delete", { id }),

  listTasks: (projectId: string) => invokeOrFallback<Task[]>("list_tasks", { projectId }, []),

  createTask: (input: CreateTaskInput) => invokeOrFallback<Task>("create_task", { input }),

  updateTask: (id: string, input: UpdateTaskInput) =>
    invokeOrFallback<Task>("update_task", { id, input }),

  deleteTask: (id: string) => invokeOrFallback<void>("delete_task", { id }),

  listActivity: (limit?: number) =>
    invokeOrFallback<ActivityEvent[]>("list_activity", { limit }, []),

  listActivityByProject: (projectId: string, limit?: number) =>
    invokeOrFallback<ActivityEvent[]>("list_activity_by_project", { projectId, limit }, []),

  listContextFiles: (projectId: string) =>
    invokeOrFallback<ContextFile[]>("list_context_files", { projectId }, []),

  readContextFile: (projectId: string, relativePath: string) =>
    invokeOrFallback<string>("read_context_file", { projectId, relativePath }),

  writeContextFile: (projectId: string, relativePath: string, content: string) =>
    invokeOrFallback<ContextFile>("write_context_file", { projectId, relativePath, content }),

  listTechnologies: (projectId: string) =>
    invokeOrFallback<Technology[]>("list_technologies", { projectId }, []),

  syncProjectStack: (projectId: string) =>
    invokeOrFallback<Technology[]>("sync_project_stack", { projectId }, []),

  listProjectTechRules: (projectId: string) =>
    invokeOrFallback<TechRule[]>("list_project_tech_rules", { projectId }, []),

  upsertTechRule: (input: UpsertTechRuleInput) =>
    invokeOrFallback<TechRule>("upsert_tech_rule", { input }),

  deleteTechRule: (id: string) => invokeOrFallback<void>("delete_tech_rule", { id }),

  searchMemory: (query: string, projectId?: string | null, limit?: number) =>
    invokeOrFallback<SearchHit[]>(
      "search_memory",
      { query, projectId: projectId ?? null, limit },
      [],
    ),

  listMemories: (projectId: string | null, limit?: number) =>
    invokeOrFallback<Memory[]>("list_memories", { projectId, limit }, []),

  memoryDuplicates: (projectId?: string | null) =>
    invokeOrFallback<DuplicatePair[]>("memory_duplicates", { projectId: projectId ?? null }, []),

  memoryUsageStats: () =>
    invokeOrFallback<MemoryUsageStats>("memory_usage_stats", undefined, {
      saves_by_agent: [],
      most_used: [],
      never_used: 0,
      total: 0,
      by_project: [],
    }),
  memoryStale: (projectId?: string | null, days?: number) =>
    invokeOrFallback<StaleReport>(
      "memory_stale",
      { projectId: projectId ?? null, days },
      { stale: [], surplus_sessions: [] },
    ),

  getMemory: (id: string) => invokeOrFallback<Memory>("get_memory", { id }),
  memoryLinks: (id: string) => invokeOrFallback<MemoryLink[]>("memory_links", { id }, []),
  memoryTriggers: (id: string) => invokeOrFallback<string[]>("memory_triggers", { id }, []),

  globalAiInventory: () =>
    invokeOrFallback<AiInventory>("global_ai_inventory", undefined, {
      instructions: [],
      skills: [],
      mcp_servers: [],
      plugins: [],
      hooks: [],
      warnings: [],
    }),

  listSecrets: () => invokeOrFallback<SecretMeta[]>("list_secrets", undefined, []),

  setSecret: (name: string, value: string, description: string) =>
    invokeOrFallback<SecretMeta>("set_secret", { name, value, description }),

  deleteSecret: (name: string) => invokeOrFallback<void>("delete_secret", { name }),

  listSecretGrants: () => invokeOrFallback<SecretGrant[]>("list_secret_grants", undefined, []),

  decideSecretGrant: (id: string, approve: boolean) =>
    invokeOrFallback<void>("decide_secret_grant", { id, approve }),

  revokeSecretGrant: (id: string) => invokeOrFallback<void>("revoke_secret_grant", { id }),

  projectFavicon: (id: string) => invokeOrFallback<string | null>("project_favicon", { id }, null),
  techIcon: (name: string, pkg?: string) =>
    invokeOrFallback<string | null>("tech_icon", { name, package: pkg ?? null }, null),

  deleteMemory: (id: string) => invokeOrFallback<void>("delete_memory", { id }),

  updateMemory: (id: string, input: UpdateMemoryInput) =>
    invokeOrFallback<Memory>("update_memory", { id, input }),
  setMemoryPinned: (id: string, pinned: boolean) =>
    invokeOrFallback<Memory>("set_memory_pinned", { id, pinned }),
  agentMemoryContext: (projectId?: string | null) =>
    invokeOrFallback<AgentContext>("agent_memory_context", {
      projectId: projectId ?? null,
    }),

  listOrphanMemoryGroups: () =>
    invokeOrFallback<OrphanGroup[]>("list_orphan_memory_groups", undefined, []),

  linkOrphanMemories: (projectName: string, projectId: string) =>
    invokeOrFallback<number>("link_orphan_memories", {
      projectName,
      projectId,
    }),

  openUrl: (url: string) => invokeOrFallback<void>("open_url", { url }),

  onActivityChanged: async (callback: () => void): Promise<UnlistenFn> => {
    if (!isTauri()) {
      return () => {};
    }
    return listen("saispace:activity", () => callback());
  },
};
