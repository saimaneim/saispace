import { describe, expect, it } from "bun:test";
import {
  type CommandResult,
  type CommandRunner,
  createSaispacePlugin,
  type PluginInput,
} from "./saispace.ts";

type CallRecord = {
  args: string[];
  cwd: string;
  timeoutMs: number;
};

function makeRunner(
  handler: (args: string[], cwd: string, timeoutMs: number) => Promise<CommandResult>,
) {
  const calls: CallRecord[] = [];
  const runner: CommandRunner = async (args, cwd, timeoutMs) => {
    calls.push({ args: [...args], cwd, timeoutMs });
    return handler(args, cwd, timeoutMs);
  };
  return { runner, calls };
}

function makeInput(overrides: Partial<PluginInput> = {}): PluginInput {
  return {
    directory: "/repo/project",
    worktree: "/repo",
    ...overrides,
  };
}

describe("saispace OpenCode plugin", () => {
  it("injects context once per session and caches subsequent transforms", async () => {
    const { runner, calls } = makeRunner(async () => ({
      exitCode: 0,
      stdout: "# Memoria del proyecto\n- Regla clave\n",
    }));
    const hooks = await createSaispacePlugin(runner)(makeInput());

    const first = { system: ["Base prompt"] };
    await hooks["experimental.chat.system.transform"]({ sessionID: "s-1" }, first);
    expect(first.system).toEqual(["Base prompt\n\n# Memoria del proyecto\n- Regla clave"]);

    const second = { system: ["Base prompt"] };
    await hooks["experimental.chat.system.transform"]({ sessionID: "s-1" }, second);
    expect(second.system).toEqual(["Base prompt\n\n# Memoria del proyecto\n- Regla clave"]);

    expect(calls).toHaveLength(1);
    expect(calls[0]).toEqual({
      args: ["memory", "context"],
      cwd: "/repo/project",
      timeoutMs: 3000,
    });
  });

  it("pushes a system entry when system array starts empty and uses worktree fallback", async () => {
    const { runner, calls } = makeRunner(async () => ({
      exitCode: 0,
      stdout: "Memoria",
    }));
    const hooks = await createSaispacePlugin(runner)(
      makeInput({ directory: "", worktree: "/repo/worktree" }),
    );

    const output = { system: [] as string[] };
    await hooks["experimental.chat.system.transform"]({ sessionID: "s-2" }, output);
    expect(output.system).toEqual(["Memoria"]);
    expect(calls[0]?.cwd).toBe("/repo/worktree");
  });

  it("refreshes with --source compact after session.compacted and falls back to plain on error", async () => {
    let failCompact = true;
    const { runner, calls } = makeRunner(async (args) => {
      if (args.includes("--source")) {
        if (failCompact) return { exitCode: 2, stdout: "" };
        return { exitCode: 0, stdout: "Post-compact notice\n# Memoria" };
      }
      return { exitCode: 0, stdout: "Plain context" };
    });

    const hooks = await createSaispacePlugin(runner)(makeInput());

    const out1 = { system: ["Sys"] };
    await hooks["experimental.chat.system.transform"]({ sessionID: "s-3" }, out1);
    expect(out1.system[0]).toBe("Sys\n\nPlain context");

    await hooks.event({
      event: { type: "session.compacted", properties: { sessionID: "s-3" } },
    });

    const out2 = { system: ["Sys"] };
    await hooks["experimental.chat.system.transform"]({ sessionID: "s-3" }, out2);
    expect(out2.system[0]).toBe("Sys\n\nPlain context");

    failCompact = false;
    await hooks.event({
      event: { type: "session.compacted", properties: { sessionID: "s-3" } },
    });

    const out3 = { system: ["Sys"] };
    await hooks["experimental.chat.system.transform"]({ sessionID: "s-3" }, out3);
    expect(out3.system[0]).toBe("Sys\n\nPost-compact notice\n# Memoria");

    const out4 = { system: ["Sys"] };
    await hooks["experimental.chat.system.transform"]({ sessionID: "s-3" }, out4);
    expect(out4.system[0]).toBe("Sys\n\nPost-compact notice\n# Memoria");

    expect(calls.map((c) => c.args)).toEqual([
      ["memory", "context"],
      ["memory", "context", "--source", "compact"],
      ["memory", "context"],
      ["memory", "context", "--source", "compact"],
    ]);
  });

  it("skips child/subagent sessions detected via events or client lookup", async () => {
    const { runner, calls } = makeRunner(async () => ({
      exitCode: 0,
      stdout: "Should not inject",
    }));
    const hooks = await createSaispacePlugin(runner)(
      makeInput({
        client: {
          session: {
            get: async ({ path }) => ({
              data: path.id === "child-late" ? { id: "child-late", parentID: "root-1" } : undefined,
            }),
          },
        },
      }),
    );

    await hooks.event({
      event: {
        type: "session.created",
        properties: { info: { id: "child-1", parentID: "root-1" } },
      },
    });

    const outChild = { system: ["Sub"] };
    await hooks["experimental.chat.system.transform"]({ sessionID: "child-1" }, outChild);
    expect(outChild.system).toEqual(["Sub"]);

    const outLate = { system: ["Sub late"] };
    await hooks["experimental.chat.system.transform"]({ sessionID: "child-late" }, outLate);
    expect(outLate.system).toEqual(["Sub late"]);

    expect(calls).toHaveLength(0);
  });

  it("injects nothing on command failure, exception, or timeout", async () => {
    const failing = makeRunner(async () => ({ exitCode: 1, stdout: "error text" }));
    const hooksFail = await createSaispacePlugin(failing.runner)(makeInput());
    const outFail = { system: ["Base"] };
    await hooksFail["experimental.chat.system.transform"]({ sessionID: "s-err" }, outFail);
    expect(outFail.system).toEqual(["Base"]);

    const throwing = makeRunner(async () => {
      throw new Error("spawn ENOENT");
    });
    const hooksThrow = await createSaispacePlugin(throwing.runner)(makeInput());
    const outThrow = { system: ["Base"] };
    await hooksThrow["experimental.chat.system.transform"]({ sessionID: "s-throw" }, outThrow);
    expect(outThrow.system).toEqual(["Base"]);

    const hanging = makeRunner(() => new Promise<CommandResult>(() => {}));
    const hooksTimeout = await createSaispacePlugin(hanging.runner, { timeoutMs: 25 })(makeInput());
    const outTimeout = { system: ["Base"] };
    await hooksTimeout["experimental.chat.system.transform"]({ sessionID: "s-hang" }, outTimeout);
    expect(outTimeout.system).toEqual(["Base"]);
  });

  it("caps injected context at 8 KB without splitting multi-byte UTF-8 characters", async () => {
    const payload = "🚀á".repeat(3000);
    const { runner } = makeRunner(async () => ({
      exitCode: 0,
      stdout: payload,
    }));
    const hooks = await createSaispacePlugin(runner)(makeInput());

    const output = { system: [] as string[] };
    await hooks["experimental.chat.system.transform"]({ sessionID: "s-cap" }, output);

    expect(output.system).toHaveLength(1);
    const injected = output.system[0] ?? "";
    expect(Buffer.byteLength(injected, "utf8")).toBeLessThanOrEqual(8192);
    expect(injected.endsWith("...[truncated]")).toBe(true);
    expect(injected.includes("\uFFFD")).toBe(false);
  });

  it("uses input.$ shell runner when no custom runner is injected", async () => {
    const recorded: { exprs: unknown[]; cwd: string }[] = [];
    const fakeShell = ((_strings: TemplateStringsArray, ...exprs: unknown[]) => {
      let cwdValue = "";
      const base = Promise.resolve().then(() => {
        recorded.push({ exprs, cwd: cwdValue });
        return { exitCode: 0, text: () => "Shell context" };
      });
      const chain = Object.assign(base, {
        cwd(dir: string) {
          cwdValue = dir;
          return chain;
        },
        quiet() {
          return chain;
        },
        nothrow() {
          return chain;
        },
      });
      return chain;
    }) as unknown as NonNullable<PluginInput["$"]>;

    const hooks = await createSaispacePlugin()(makeInput({ $: fakeShell }));
    const output = { system: [""] };
    await hooks["experimental.chat.system.transform"]({ sessionID: "s-shell" }, output);

    expect(output.system).toEqual(["Shell context"]);
    expect(recorded).toEqual([{ exprs: [["memory", "context"]], cwd: "/repo/project" }]);
  });

  it("cleans session state on session.deleted so a new lifecycle fetches fresh context", async () => {
    let count = 0;
    const { runner, calls } = makeRunner(async () => {
      count += 1;
      return { exitCode: 0, stdout: `Ctx ${count}` };
    });
    const hooks = await createSaispacePlugin(runner)(makeInput());

    const out1 = { system: ["Sys"] };
    await hooks["experimental.chat.system.transform"]({ sessionID: "s-del" }, out1);
    expect(out1.system[0]).toBe("Sys\n\nCtx 1");

    await hooks.event({
      event: { type: "session.deleted", properties: { info: { id: "s-del" } } },
    });

    const out2 = { system: ["Sys"] };
    await hooks["experimental.chat.system.transform"]({ sessionID: "s-del" }, out2);
    expect(out2.system[0]).toBe("Sys\n\nCtx 2");
    expect(calls).toHaveLength(2);
  });

  it("reclassifies a root session into a child on session.updated and ignores stale in-flight writes", async () => {
    let releaseFirst: (res: CommandResult) => void = () => {};
    let markStarted: () => void = () => {};
    const firstStarted = new Promise<void>((resolve) => {
      markStarted = resolve;
    });
    let turn = 0;
    const { runner } = makeRunner(async () => {
      turn += 1;
      if (turn === 1) {
        markStarted();
        return new Promise<CommandResult>((resolve) => {
          releaseFirst = resolve;
        });
      }
      return { exitCode: 0, stdout: "Post-compact fresh" };
    });

    const hooks = await createSaispacePlugin(runner)(makeInput());
    const out1 = { system: ["Sys"] };
    const firstCall = hooks["experimental.chat.system.transform"]({ sessionID: "s-race" }, out1);
    await firstStarted;

    await hooks.event({
      event: { type: "session.compacted", properties: { sessionID: "s-race" } },
    });
    releaseFirst({ exitCode: 0, stdout: "Stale initial" });
    await firstCall;

    const out2 = { system: ["Sys"] };
    await hooks["experimental.chat.system.transform"]({ sessionID: "s-race" }, out2);
    expect(out2.system[0]).toBe("Sys\n\nPost-compact fresh");

    await hooks.event({
      event: {
        type: "session.updated",
        properties: { info: { id: "s-race", parentID: "root-parent" } },
      },
    });
    const outReclassified = { system: ["Child"] };
    await hooks["experimental.chat.system.transform"]({ sessionID: "s-race" }, outReclassified);
    expect(outReclassified.system).toEqual(["Child"]);
  });
});
