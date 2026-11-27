export type ProjectStatus = "active" | "on_hold" | "completed" | "archived";

export interface Project {
  id: string;
  name: string;
  path: string;
  status: ProjectStatus;
  pinned: boolean;
  sort_order: number;
  created_at: string;
  updated_at: string;
}

export interface CreateProjectInput {
  name: string;
  path: string;
}

export interface UpdateProjectInput {
  name?: string;
  path?: string;
  status?: ProjectStatus;
}

export type TaskStatus = "todo" | "in_progress" | "blocked" | "done" | "cancelled";
export type TaskPriority = "low" | "medium" | "high" | "urgent";

export interface Task {
  id: string;
  project_id: string;
  title: string;
  status: TaskStatus;
  priority: TaskPriority;
  goal_id: string | null;
  created_at: string;
  updated_at: string;
  completed_at?: string | null;
}

export interface CreateTaskInput {
  project_id: string;
  title: string;
  priority?: TaskPriority;
  goal_id?: string | null;
}

export interface UpdateTaskInput {
  title?: string;
  status?: TaskStatus;
  priority?: TaskPriority;
}

export type GoalStatus = "active" | "completed" | "archived";
export type PlanFilter = "active" | "completed" | "all";

export interface Goal {
  id: string;
  project_id: string;
  title: string;
  criteria: string | null;
  status: GoalStatus;
  position: number;
  created_at: string;
  updated_at: string;
  completed_at: string | null;
}

export interface UpdateGoalInput {
  title?: string;
  criteria?: string | null;
  status?: GoalStatus;
  position?: number;
}

export interface GoalWithTasks {
  goal: Goal;
  done: number;
  total: number;
  tasks: Task[];
}

export interface Plan {
  goals: GoalWithTasks[];
  loose_tasks: Task[];
  counts: { active: number; completed: number; all: number };
}

export type Actor = { type: "user" } | { type: "agent"; name: string };

export type ActionKind = "create" | "read" | "update" | "delete";
export type EntityKind = "project" | "task" | "context_notes" | "tech_rule" | "stack" | "search";

export interface ActivityEvent {
  id: string;
  project_id?: string | null;
  actor: Actor;
  action: ActionKind;
  entity: EntityKind;
  entity_id?: string | null;
  details?: unknown;
  created_at: string;
}

export interface DirectoryEntry {
  name: string;
  path: string;
  is_dir: boolean;
}

export interface DirectoryListing {
  current_path: string;
  display_path: string;
  parent_path: string | null;
  entries: DirectoryEntry[];
}

export type ContextFileKind =
  | "agents"
  | "claude"
  | "gemini"
  | "design"
  | "rules"
  | "agents_dir"
  | "other";

export interface ContextFile {
  path: string;
  name: string;
  kind: ContextFileKind;
  size: number;
  modified_at: string | null;
  /** Present when the file only points at another one: a symlink, or a stub like `@AGENTS.md`. */
  link?: { target: string; via: "symlink" | "import" } | null;
}

export interface Technology {
  id: string;
  project_id: string;
  name: string;
  version: string;
  ecosystem: "runtime" | "npm" | "cargo" | "pypi" | "go";
  source: string;
  docs_url: string;
  detected_at: string;
}

export interface TechRule {
  id: string;
  technology: string;
  version_req: string | null;
  rule: string;
  created_at: string;
  updated_at: string;
}

export interface UpsertTechRuleInput {
  technology: string;
  version?: string | null;
  rule: string;
}

export type SearchKind = "project" | "task" | "tech_rule" | "memory";

export interface SearchHit {
  kind: SearchKind;
  id: string;
  project_id: string | null;
  title: string;
  snippet: string;
  rank: number;
}

export type MemoryKind =
  | "decision"
  | "architecture"
  | "bugfix"
  | "pattern"
  | "config"
  | "discovery"
  | "preference"
  | "session_summary"
  | "note";

export interface Memory {
  id: string;
  project_id: string | null;
  project_name: string | null;
  kind: MemoryKind;
  title: string;
  content: string;
  topic_key: string | null;
  pinned: boolean;
  created_at: string;
  updated_at: string;
  last_used_at: string | null;
  use_count: number;
  /** Set when the memory was consolidated into another one. */
  superseded_by: string | null;
}

export type MemoryRelation = "related" | "depends_on" | "supersedes";

export interface MemoryLink {
  id: string;
  title: string;
  kind: MemoryKind;
  relation: MemoryRelation;
  direction: "out" | "in";
  archived: boolean;
}

export interface UpdateMemoryInput {
  title?: string;
  content?: string;
  kind?: MemoryKind;
  topic_key?: string;
}

export interface AgentContext {
  text: string;
  bytes: number;
}

export interface OrphanGroup {
  project_name: string;
  count: number;
  last_updated: string;
}

export interface DuplicatePair {
  a: Memory;
  b: Memory;
}

export interface AgentMemorySaves {
  actor_name: string;
  last_7d: number;
  last_30d: number;
}

export interface MostUsedMemory {
  id: string;
  title: string;
  project_name: string | null;
  use_count: number;
  last_used_at: string | null;
}

export interface ProjectMemoryUsage {
  project_name: string;
  total: number;
  used: number;
}

export interface MemoryUsageStats {
  saves_by_agent: AgentMemorySaves[];
  most_used: MostUsedMemory[];
  never_used: number;
  total: number;
  by_project: ProjectMemoryUsage[];
}

export interface StaleReport {
  stale: Memory[];
  surplus_sessions: Memory[];
}

export interface InstructionFile {
  tool: string;
  path: string;
  content: string;
  truncated: boolean;
}

export interface SkillEntry {
  source: string;
  name: string;
  description: string;
  path: string;
}

export interface McpServerEntry {
  tool: string;
  name: string;
  target: string;
  env_keys: string[];
  enabled: boolean;
}

export interface PluginEntry {
  tool: string;
  name: string;
  version: string;
  enabled: boolean;
}

export interface HookEntry {
  tool: string;
  event: string;
  command: string;
}

export interface AiInventory {
  instructions: InstructionFile[];
  skills: SkillEntry[];
  mcp_servers: McpServerEntry[];
  plugins: PluginEntry[];
  hooks: HookEntry[];
  warnings: string[];
}

export interface SecretMeta {
  id: string;
  name: string;
  description: string;
  created_at: string;
  updated_at: string;
  last_used_at: string | null;
}

export type GrantStatus = "pending" | "approved" | "denied";

export interface SecretGrant {
  id: string;
  secret_id: string;
  secret_name: string;
  agent: string;
  status: GrantStatus;
  reason: string;
  created_at: string;
  decided_at: string | null;
}
