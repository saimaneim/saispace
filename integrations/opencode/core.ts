// saispace memory for OpenCode >= 2.4 (plugin API v2).
//
// On a session's first execution the plugin stores the output of
// `sai memory context` as a keyed session instruction entry, so the agent
// sees the project's memory index without touching OpenCode's base prompt.
// After a compaction it swaps in the `--source compact` variant (which asks
// the agent to save a session_summary) for one execution, then restores the
// plain index. Every failure is silent: the plugin must never break OpenCode.
//
// Each user prompt (`session.inbox.enqueued`) is piped through
// `sai memory nudge`. When the CLI detects a correction it returns a short
// reminder (<= 300 chars) that is stored as a second instruction entry and
// removed when that execution ends, so it lasts one turn. v2 ignores what
// `setup` returns, so everything goes through events and instruction entries.

// Minimal ambient types: the plugin runs on Bun inside OpenCode and has no
// dependencies, so it does not pull in @types/bun or @types/node.
declare const Bun: {
  spawn(
    cmd: string[],
    options: {
      cwd: string;
      stdin: "ignore" | "pipe";
      stdout: "pipe";
      stderr: "ignore";
    },
  ): {
    stdout: ReadableStream;
    // Bun's FileSink, present when stdin is "pipe".
    stdin: { write(chunk: Uint8Array): number; end(): void } | undefined;
    exited: Promise<number>;
    kill(): void;
  };
};
declare const process: { cwd(): string };

export type CommandResult = { exitCode: number; stdout: string; timedOut?: boolean };
export type CommandRunner = (
  args: string[],
  cwd: string,
  timeoutMs: number,
  stdin?: string,
) => Promise<CommandResult>;

/** Bus event; `session.inbox.enqueued` carries the queued item. */
export type OpenCodeEvent = {
  type?: string;
  data?: {
    sessionID?: string;
    item?: { type?: string; payload?: { text?: string } };
  };
};

/** The slice of OpenCode's v2 plugin context this plugin uses. */
export type PluginContext = {
  location?: { directory?: string };
  event?: { subscribe(): AsyncIterable<OpenCodeEvent> };
  session?: {
    get?(input: { sessionID: string }): Promise<{ parentID?: string } | undefined>;
    instructions?: {
      entry?: {
        put(input: { sessionID: string; key: string; value: string }): Promise<unknown>;
        remove?(input: { sessionID: string; key: string }): Promise<unknown>;
      };
    };
  };
};

export type SaispacePluginOptions = { timeoutMs?: number; maxBytes?: number; nudgeTimeoutMs?: number };

export const ENTRY_KEY = "saispace-memory";
export const NUDGE_ENTRY_KEY = "saispace-nudge";
const DEFAULT_TIMEOUT_MS = 3000;
const DEFAULT_NUDGE_TIMEOUT_MS = 1500;
const MAX_CONTEXT_BYTES = 8192;
// `sai memory nudge` reads only the start of the prompt.
const MAX_NUDGE_PROMPT_CHARS = 2000;
const TRUNCATION_SUFFIX = "\n...[truncated]";
const encoder = new TextEncoder();
const decoder = new TextDecoder();

type Source = "startup" | "compact";

/** Runs `sai` with optional stdin, kills it if it outlives the timeout. */
export const spawnRunner: CommandRunner = async (args, cwd, timeoutMs, stdinData) => {
  let proc: ReturnType<typeof Bun.spawn> | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    proc = Bun.spawn(["sai", ...args], {
      cwd,
      stdin: stdinData !== undefined ? "pipe" : "ignore",
      stdout: "pipe",
      stderr: "ignore",
    });
    const child = proc;
    let timedOut = false;
    timer = setTimeout(() => {
      timedOut = true;
      child.kill();
    }, timeoutMs);

    if (stdinData !== undefined && child.stdin) {
      child.stdin.write(encoder.encode(stdinData));
      child.stdin.end();
    }

    const [stdout, exitCode] = await Promise.all([new Response(child.stdout).text(), child.exited]);
    return timedOut ? { exitCode: 124, stdout: "", timedOut } : { exitCode, stdout };
  } catch {
    proc?.kill();
    return { exitCode: 1, stdout: "" };
  } finally {
    clearTimeout(timer);
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

/** Text of a user prompt queued in the session inbox, if the event is one. */
export function userPromptText(event: OpenCodeEvent): string {
  if (event.type !== "session.inbox.enqueued") return "";
  const item = event.data?.item;
  if (item?.type !== "user") return "";
  return (item.payload?.text ?? "").trim();
}

export async function runNudge(
  promptText: string,
  runner: CommandRunner,
  dir: string,
  timeoutMs: number,
): Promise<string> {
  if (!promptText) return "";
  try {
    const prompt = promptText.slice(0, MAX_NUDGE_PROMPT_CHARS);
    const payload = JSON.stringify({ hook_event_name: "UserPromptSubmit", prompt });
    const res = await runner(["memory", "nudge"], dir, timeoutMs, payload);
    if (res.exitCode !== 0) return "";
    return (res.stdout ?? "").trim();
  } catch {
    return "";
  }
}

export function createSaispacePlugin(
  runner: CommandRunner = spawnRunner,
  options: SaispacePluginOptions = {},
) {
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const nudgeTimeoutMs = options.nudgeTimeoutMs ?? DEFAULT_NUDGE_TIMEOUT_MS;
  const maxBytes = options.maxBytes ?? MAX_CONTEXT_BYTES;

  return {
    id: "saispace",
    setup: async (ctx: PluginContext): Promise<void> => {
      const dir = ctx.location?.directory || process.cwd();
      const put = ctx.session?.instructions?.entry?.put;
      const events = ctx.event?.subscribe;

      if (put && events) {
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

        const injected = new Set<string>();
        const compacted = new Set<string>();
        const children = new Set<string>();
        const nudged = new Set<string>();
        const entry = () => ctx.session?.instructions?.entry;

        const clearNudge = async (sessionID: string): Promise<void> => {
          if (!nudged.delete(sessionID)) return;
          try {
            const remove = entry()?.remove;
            if (remove) await remove.call(entry(), { sessionID, key: NUDGE_ENTRY_KEY });
            else await put.call(entry(), { sessionID, key: NUDGE_ENTRY_KEY, value: "" });
          } catch {
            // Best effort: the next prompt replaces it anyway.
          }
        };

        const nudge = async (sessionID: string, text: string): Promise<void> => {
          // Subagent prompts are written by the agent, not the user.
          if (children.has(sessionID)) return;
          if (!injected.has(sessionID) && (await isChild(sessionID))) {
            children.add(sessionID);
            return;
          }
          const reminder = await runNudge(text, runner, dir, nudgeTimeoutMs);
          if (!reminder) {
            await clearNudge(sessionID);
            return;
          }
          try {
            await put.call(entry(), { sessionID, key: NUDGE_ENTRY_KEY, value: reminder });
            nudged.add(sessionID);
          } catch {
            // Never break the session over a reminder.
          }
        };

        const handle = async (event: OpenCodeEvent): Promise<void> => {
          const sessionID = event.data?.sessionID;
          if (!sessionID) return;
          const prompt = userPromptText(event);
          if (prompt) {
            await nudge(sessionID, prompt);
          } else if (event.type === "session.execution.started" && !injected.has(sessionID)) {
            if (await isChild(sessionID)) {
              injected.add(sessionID);
              children.add(sessionID);
              return;
            }
            if (await inject(sessionID, "startup")) injected.add(sessionID);
          } else if (event.type === "session.compaction.ended" && injected.has(sessionID)) {
            if (await inject(sessionID, "compact")) compacted.add(sessionID);
          } else if (event.type === "session.execution.succeeded" && compacted.has(sessionID)) {
            compacted.delete(sessionID);
            await inject(sessionID, "startup");
          }
          if (
            event.type === "session.execution.succeeded" ||
            event.type === "session.execution.failed" ||
            event.type === "session.execution.interrupted"
          ) {
            await clearNudge(sessionID);
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
      }

    },
  };
}
