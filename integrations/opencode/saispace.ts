export type CommandResult = { exitCode: number; stdout: string; timedOut?: boolean };
export type CommandRunner = (
  args: string[],
  cwd: string,
  timeoutMs: number,
) => Promise<CommandResult>;
type ShellPromise = Promise<{ exitCode: number; text(): string }> & {
  cwd(dir: string): ShellPromise;
  quiet(): ShellPromise;
  nothrow(): ShellPromise;
};
type BunShell = (strings: TemplateStringsArray, ...expressions: unknown[]) => ShellPromise;
type SessionInfo = { id?: string; parentID?: string; directory?: string };
export type PluginInput = {
  directory?: string;
  worktree?: string;
  $?: BunShell;
  client?: {
    session?: {
      get?: (params: { path: { id: string } }) => Promise<{ data?: SessionInfo }>;
    };
  };
};
export type PluginEvent = { type: string; properties?: { sessionID?: string; info?: SessionInfo } };
export type PluginHooks = {
  event: (input: { event: PluginEvent }) => Promise<void>;
  "experimental.chat.system.transform": (
    input: { sessionID?: string; model?: unknown },
    output: { system: string[] },
  ) => Promise<void>;
};
export type Plugin = (input: PluginInput) => Promise<PluginHooks>;
export type SaispacePluginOptions = { timeoutMs?: number; maxBytes?: number };

type SessionState = {
  gen: number;
  isChild?: boolean;
  isRoot?: boolean;
  dir?: string;
  cached?: string;
  inFlight?: Promise<string>;
  compactPending?: boolean;
};

const DEFAULT_TIMEOUT_MS = 3000;
const MAX_CONTEXT_BYTES = 8192;
const TRUNCATION_SUFFIX = "\n...[truncated]";
const encoder = new TextEncoder();
const decoder = new TextDecoder();

function withTimeout<T>(promise: Promise<T>, timeoutMs: number, fallback: T): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<T>((resolve) => {
    timer = setTimeout(() => resolve(fallback), timeoutMs);
  });
  return Promise.race([promise.catch(() => fallback), timeout]).finally(() => {
    if (timer !== undefined) clearTimeout(timer);
  });
}

function createShellRunner(shell?: BunShell): CommandRunner {
  return async (args, cwd, timeoutMs) => {
    if (!shell) return { exitCode: 1, stdout: "" };
    try {
      const proc = shell`sai ${args}`.cwd(cwd).quiet().nothrow();
      return await withTimeout<CommandResult>(
        proc.then((out) => ({ exitCode: out.exitCode, stdout: out.text() })),
        timeoutMs,
        { exitCode: 124, stdout: "", timedOut: true },
      );
    } catch {
      return { exitCode: 1, stdout: "" };
    }
  };
}

function truncateUtf8(text: string, maxBytes: number): string {
  const trimmed = text.trim();
  if (!trimmed) return "";
  const encoded = encoder.encode(trimmed);
  if (encoded.byteLength <= maxBytes) return trimmed;
  const suffixBytes = encoder.encode(TRUNCATION_SUFFIX).byteLength;
  let end = Math.max(0, maxBytes <= suffixBytes ? maxBytes : maxBytes - suffixBytes);
  while (end > 0 && (encoded[end] & 0xc0) === 0x80) end -= 1;
  const head = decoder.decode(encoded.subarray(0, end));
  return maxBytes <= suffixBytes ? head : head.trimEnd() + TRUNCATION_SUFFIX;
}

function resetCache(state: SessionState, compactPending: boolean): void {
  state.gen += 1;
  state.cached = undefined;
  state.inFlight = undefined;
  state.compactPending = compactPending;
}

export function createSaispacePlugin(
  customRunner?: CommandRunner,
  options: SaispacePluginOptions = {},
): Plugin {
  const runnerOverride = typeof customRunner === "function" ? customRunner : undefined;
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const maxBytes = options.maxBytes ?? MAX_CONTEXT_BYTES;

  return async (input) => {
    const runner = runnerOverride ?? createShellRunner(input.$);
    const defaultDir = input.directory || input.worktree || ".";
    const sessions = new Map<string, SessionState>();
    const getState = (id: string): SessionState => {
      const existing = sessions.get(id);
      if (existing) return existing;
      const created: SessionState = { gen: 0 };
      sessions.set(id, created);
      return created;
    };

    const isChildSession = async (sessionID: string): Promise<boolean> => {
      const state = getState(sessionID);
      if (state.isChild) return true;
      if (state.isRoot || !input.client?.session?.get) return false;
      try {
        const res = await withTimeout(
          input.client.session.get({ path: { id: sessionID } }),
          timeoutMs,
          {},
        );
        if (res?.data?.parentID) {
          state.isChild = true;
          return true;
        }
        if (res?.data?.id === sessionID) {
          state.isRoot = true;
          if (res.data.directory) state.dir = res.data.directory;
        }
      } catch {}
      return false;
    };

    const runSafe = (args: string[], cwd: string): Promise<CommandResult> =>
      withTimeout(
        Promise.resolve().then(() => runner(args, cwd, timeoutMs)),
        timeoutMs,
        { exitCode: 124, stdout: "", timedOut: true },
      );

    const fetchContext = async (sessionID: string): Promise<string> => {
      const state = getState(sessionID);
      if (state.inFlight) return state.inFlight;
      const gen = state.gen;
      const cwd = state.dir || defaultDir;
      const useCompact = Boolean(state.compactPending);
      state.compactPending = false;
      const task = (async () => {
        let res = await runSafe(
          useCompact ? ["memory", "context", "--source", "compact"] : ["memory", "context", "--source", "startup"],
          cwd,
        );
        if (useCompact && res.exitCode !== 0 && !res.timedOut) {
          res = await runSafe(["memory", "context", "--source", "startup"], cwd);
        }
        const text = res.exitCode === 0 ? truncateUtf8(res.stdout ?? "", maxBytes) : "";
        if (sessions.get(sessionID) === state && state.gen === gen) state.cached = text;
        return text;
      })().finally(() => {
        if (state.inFlight === task) state.inFlight = undefined;
      });
      state.inFlight = task;
      return task;
    };

    return {
      event: async ({ event }) => {
        try {
          const info = event.properties?.info;
          const id = event.properties?.sessionID ?? info?.id;
          if (event.type === "session.created" || event.type === "session.updated") {
            if (!info?.id) return;
            const state = getState(info.id);
            state.isChild = Boolean(info.parentID);
            state.isRoot = !info.parentID;
            if (info.parentID) resetCache(state, false);
            else if (info.directory) state.dir = info.directory;
          } else if (event.type === "session.compacted" && id) {
            resetCache(getState(id), true);
          } else if (event.type === "session.deleted" && id) {
            const state = sessions.get(id);
            if (state) state.gen += 1;
            sessions.delete(id);
          }
        } catch {}
      },
      "experimental.chat.system.transform": async (hookInput, output) => {
        try {
          const sessionID = hookInput.sessionID;
          if (!sessionID || (await isChildSession(sessionID))) return;
          const state = getState(sessionID);
          const context = state.cached !== undefined ? state.cached : await fetchContext(sessionID);
          if (!context) return;
          const last = output.system.length - 1;
          if (last >= 0) {
            output.system[last] = output.system[last]
              ? `${output.system[last]}\n\n${context}`
              : context;
          } else output.system.push(context);
        } catch {}
      },
    };
  };
}

export const SaispacePlugin: Plugin = createSaispacePlugin();
export default { id: "saispace", server: SaispacePlugin };
