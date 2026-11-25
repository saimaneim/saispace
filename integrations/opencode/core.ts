// saispace memory for OpenCode >= 2.4 (plugin API v2).
//
// On a session's first execution the plugin stores the output of
// `sai memory context` as a keyed session instruction entry, so the agent
// sees the project's memory index without touching OpenCode's base prompt.
// After a compaction it swaps in the `--source compact` variant (which asks
// the agent to save a session_summary) for one execution, then restores the
// plain index. Every failure is silent: the plugin must never break OpenCode.

// Minimal ambient types: the plugin runs on Bun inside OpenCode and has no
// dependencies, so it does not pull in @types/bun or @types/node.
declare const Bun: {
  spawn(
    cmd: string[],
    options: { cwd: string; stdin: "ignore"; stdout: "pipe"; stderr: "ignore" },
  ): { stdout: ReadableStream; exited: Promise<number>; kill(): void };
};
declare const process: { cwd(): string };

export type CommandResult = { exitCode: number; stdout: string; timedOut?: boolean };
export type CommandRunner = (
  args: string[],
  cwd: string,
  timeoutMs: number,
) => Promise<CommandResult>;

export type OpenCodeEvent = { type?: string; data?: { sessionID?: string } };

/** The slice of OpenCode's v2 plugin context this plugin uses. */
export type PluginContext = {
  location?: { directory?: string };
  event?: { subscribe(): AsyncIterable<OpenCodeEvent> };
  session?: {
    get?(input: { sessionID: string }): Promise<{ parentID?: string } | undefined>;
    instructions?: {
      entry?: {
        put(input: { sessionID: string; key: string; value: string }): Promise<unknown>;
      };
    };
  };
};

export type SaispacePluginOptions = { timeoutMs?: number; maxBytes?: number };

export const ENTRY_KEY = "saispace-memory";
const DEFAULT_TIMEOUT_MS = 3000;
const MAX_CONTEXT_BYTES = 8192;
const TRUNCATION_SUFFIX = "\n...[truncated]";
const encoder = new TextEncoder();
const decoder = new TextDecoder();

type Source = "startup" | "compact";

/** Runs `sai` with stdin closed and kills it if it outlives the timeout. */
export const spawnRunner: CommandRunner = async (args, cwd, timeoutMs) => {
  try {
    const proc = Bun.spawn(["sai", ...args], {
      cwd,
      stdin: "ignore",
      stdout: "pipe",
      stderr: "ignore",
    });
    let timedOut = false;
    const timer = setTimeout(() => {
      timedOut = true;
      proc.kill();
    }, timeoutMs);
    const [stdout, exitCode] = await Promise.all([new Response(proc.stdout).text(), proc.exited]);
    clearTimeout(timer);
    return timedOut ? { exitCode: 124, stdout: "", timedOut } : { exitCode, stdout };
  } catch {
    return { exitCode: 1, stdout: "" };
  }
};

export function truncateUtf8(text: string, maxBytes: number): string {
  const trimmed = text.trim();
  if (!trimmed) return "";
  const encoded = encoder.encode(trimmed);
  if (encoded.byteLength <= maxBytes) return trimmed;
  const suffixBytes = encoder.encode(TRUNCATION_SUFFIX).byteLength;
  let end = Math.max(0, maxBytes - suffixBytes);
  while (end > 0 && (encoded[end] & 0xc0) === 0x80) end -= 1;
  return decoder.decode(encoded.subarray(0, end)).trimEnd() + TRUNCATION_SUFFIX;
}

export function createSaispacePlugin(
  runner: CommandRunner = spawnRunner,
  options: SaispacePluginOptions = {},
) {
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const maxBytes = options.maxBytes ?? MAX_CONTEXT_BYTES;

  return {
    id: "saispace",
    setup: async (ctx: PluginContext) => {
      const dir = ctx.location?.directory || process.cwd();
      const put = ctx.session?.instructions?.entry?.put;
      const events = ctx.event?.subscribe;
      if (!put || !events) return;

      const fetchContext = async (source: Source): Promise<string | undefined> => {
        try {
          const res = await runner(["memory", "context", "--source", source], dir, timeoutMs);
          return res.exitCode === 0 ? truncateUtf8(res.stdout ?? "", maxBytes) : undefined;
        } catch {
          return undefined;
        }
      };

      const isChild = async (sessionID: string): Promise<boolean> => {
        try {
          const info = await ctx.session?.get?.({ sessionID });
          return Boolean(info?.parentID);
        } catch {
          return false;
        }
      };

      const inject = async (sessionID: string, source: Source): Promise<boolean> => {
        const value = await fetchContext(source);
        if (!value) return false;
        try {
          await put.call(ctx.session?.instructions?.entry, { sessionID, key: ENTRY_KEY, value });
          return true;
        } catch {
          return false;
        }
      };

      // Sessions that already have the index, and sessions showing the
      // one-shot post-compaction notice.
      const injected = new Set<string>();
      const compacted = new Set<string>();

      const handle = async (event: OpenCodeEvent): Promise<void> => {
        const sessionID = event.data?.sessionID;
        if (!sessionID) return;
        if (event.type === "session.execution.started" && !injected.has(sessionID)) {
          if (await isChild(sessionID)) {
            injected.add(sessionID);
            return;
          }
          // Failures stay retryable on the next execution.
          if (await inject(sessionID, "startup")) injected.add(sessionID);
        } else if (event.type === "session.compaction.ended" && injected.has(sessionID)) {
          if (await inject(sessionID, "compact")) compacted.add(sessionID);
        } else if (event.type === "session.execution.succeeded" && compacted.has(sessionID)) {
          compacted.delete(sessionID);
          await inject(sessionID, "startup");
        }
      };

      void (async () => {
        try {
          for await (const event of events.call(ctx.event)) {
            await handle(event).catch(() => {});
          }
        } catch {
          // Stream closed (e.g. location shutdown): nothing to clean up.
        }
      })();
    },
  };
}
