# saispace

Local project memory and static checks for AI coding agents. One SQLite database, three front ends:

- **`sai`** — CLI for memories, the `sai check` static checker, the plan and stored secrets.
- **`saispace-mcp`** — MCP server (stdio) exposing the same data to agents.
- **Desktop app** — Tauri UI over the same database.

The database lives at `~/.local/share/saispace/saispace.db` (override with `SAISPACE_DB_PATH`). Anything saved from one front end is visible in the others. Projects are resolved from the working directory (git root) and registered on the first save.

## Quickstart

```sh
curl -fsSL https://github.com/saimaneim/saispace/releases/latest/download/install.sh | sh
sai setup claude        # or: opencode, cursor, windsurf, vscode, codex, gemini-cli, antigravity, manual
sai doctor              # database, index, vault key and connected agents, read-only
```

## Why saispace

- **Lessons come back on their own.** A memory with `--trigger "database is locked"` is re-injected the moment that error shows up again, and `env:<host>` lessons load only on that host.
- **Measured, not claimed.** `sai bench` scores recall, search, duplicate detection and latency on fixed fixtures and fails CI on regressions; `sai kpi` tracks real use.
- **Rules that run.** `.agents/checks` are executed by `sai check` in Rust: zero tokens, same result every time.
- **Secrets stay secret.** A ChaCha20-Poly1305 vault for keys, credentials scrubbed from every memory before it is stored, and `sai memory purge` for a trace-free delete.
- **Local first.** No account, no server. Share a project's knowledge through git with `sai sync`.
- **Native desktop app** in Tauri 2 over the same database.

## Workspace

```
crates/core     Database, migrations, models, FTS5 search, secret vault
crates/cli      `sai` binary
crates/mcp      `saispace-mcp` binary (rmcp, stdio transport)
crates/rules    Rule engine for `.agents/checks` (`sai check`)
apps/desktop    Tauri 2 app: React + Vite frontend, Rust backend in src-tauri/
integrations/   Agent integrations (OpenCode plugin)
```

## Requirements

- Rust stable (edition 2024)
- [Bun](https://bun.sh) for `apps/desktop`
- Tauri 2 system dependencies to build the desktop app

## Build and install

```sh
cargo build --release -p saispace-cli -p saispace-mcp

cargo install --path crates/cli   # installs `sai`
cargo install --path crates/mcp   # installs `saispace-mcp`
```

Desktop app:

```sh
cd apps/desktop
bun install
bunx tauri dev      # or: bunx tauri build
```

Each GitHub release (`vX.Y.Z` tags) ships `sai` and `saispace-mcp` for linux-x64, macos-arm64, macos-x64 and windows-x64, with `SHA256SUMS` and `install.sh`. `sai --version` prints the version and commit.

## Tests

```sh
cargo test --workspace

cd apps/desktop
bun run check   # tsc --noEmit
bun run lint    # biome
```

## Benchmark

`sai bench` evaluates correction detection, Stop hook accuracy, recall by tool error (`recall_lessons.jsonl` / `recall_cases.jsonl`), FTS5 search quality, duplicate detection, session index size, and 1,000-memory latencies against deterministic fixtures in `bench/fixtures/` (never touching the user database).

```sh
sai bench              # summary table
sai bench --json       # machine-readable report
sai bench --compare    # compare against bench/history.jsonl (fails on regressions)
sai bench --record     # append a baseline entry to bench/history.jsonl
```

## Usage

### CLI

```sh
$ sai memory save --kind decision --topic architecture/storage \
    --title "SQLite as single store" \
    --content "CLI, MCP and desktop share one SQLite DB."
90f0ad12  2026-11-25  [Decision] SQLite as single store  (saispace)

$ sai memory search "sqlite"
Memory  90f0ad12  SQLite as single store

$ sai memory get 90f0ad12   # short id (last 8 hex digits) or full id; shows links
$ sai memory connect 90f0ad12 1ae0c0f3 --relation depends_on
$ sai memory merge 90f0ad12 55b298f2 --title "…" --content -   # archive, never delete
$ sai memory context        # memory index for the current project
$ sai check                 # run .agents/checks against the repo
$ sai skills sync           # link .agents/skills into .claude/skills (Claude Code)
$ sai setup opencode        # install the OpenCode plugin
$ sai setup claude          # install the Claude Code memory hooks
$ sai setup rules           # copy ~/.config/saispace/rules.md into every installed agent's global file
$ sai setup cursor          # MCP for cursor, windsurf, vscode, codex, gemini-cli, antigravity (backs up with .bak)
$ sai doctor [--deep] [--json]
$ sai init [name]           # pin this repo to a project name with .saispace.toml
$ sai memory save --personal …   # personal note: never exported
$ sai memory tidy [--apply] # merge look-alikes, archive surplus sessions, attach orphans
$ sai memory purge <id> --yes    # irreversible, trace-free delete
$ sai sync export | import  # share project memories through .saispace/sync/ (no personal notes, no secrets)
$ sai export obsidian --out ~/vault   # notes with [[wikilinks]] from the memory graph
```

The project is resolved from `SAISPACE_PROJECT`, then the nearest `.saispace.toml`, then the git root. The session context is capped at 8,000 bytes (`--max-bytes` or `SAISPACE_CONTEXT_BYTES`), filled by priority: preferences, host lessons, temporary facts, pinned, plan, last session, recent titles.

Saving again with the same `--topic` updates the memory instead of creating a duplicate. Mentioning another memory's id or `topic_key` in the content links them, and only the latest 3 session summaries per project stay active.

### MCP

Register the server with an MCP client, e.g. Claude Code:

```sh
claude mcp add saispace -- saispace-mcp
```

Or in a JSON client config:

```json
{
  "mcpServers": {
    "saispace": {
      "command": "saispace-mcp",
      "env": { "SAISPACE_AGENT_NAME": "my-agent" }
    }
  }
}
```

Or let `sai setup <client>` write it for you. Tools include `doctor`, `save_memory`, `search_memory`, `memory_context`, `get_memory`, `link_memories`, `consolidate_memories`, `delete_memory`, `list_projects`, `get_project_context` (re-detects the stack), `create_task`, `update_task`, `get_active_plan`, `create_goal`, `update_goal`, `complete_task`, `list_secrets` and `request_secret_access`. Example `save_memory` call:

```json
{
  "title": "SQLite as single store",
  "content": "CLI, MCP and desktop share one SQLite DB.",
  "kind": "decision",
  "topic_key": "architecture/storage"
}
```

The memory is scoped to the project of the server's working directory, or to `path` / `project_id` if given; `"global": true` makes it cross-project.

## Static checks (`sai check`)

Project rules live as TOML files in `.agents/checks/` and run with `sai check`: a Rust engine, parallel and deterministic, that costs agents zero tokens. A rule that can be checked is a check, not a line in `AGENTS.md`.

- Match kinds: `forbid`, `require`, `max_lines`, path references that must exist, and more (see [crates/rules/README.md](crates/rules/README.md)).
- Scope with globs, file guards (`only_if`, `unless`), line and match exceptions, `within` a captured span.
- `--json` output for agents, `--strict` to fail on warnings; the pre-push hook and CI run it.

```sh
$ sai check                 # all rules
$ sai check src/ --rule ui-no-raw-colors --json
```

## Learning from mistakes

Agents save a lesson when it happens: a user correction becomes a `preference`; an own mistake with a non-obvious cause becomes a `bugfix` with that cause.

| When | What saispace does |
|---|---|
| Session starts | Injects the memory index, plus lessons tied to the host the agent runs in. |
| The user corrects the agent | Reminds it to save the rule. |
| A tool fails with a known error | Hands back the saved fix before the agent retries (once per session). |
| The session ends | Asks to save what was learned, naming errors that repeated. |

A lesson comes back on its own through triggers: a stable piece of an error message (`--trigger "<error text>"`), or `env:<host>` for lessons about a specific host. `sai setup claude` installs the Claude Code hooks; the OpenCode plugin does the same for OpenCode.

## License

MIT, see [LICENSE](LICENSE).
