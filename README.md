# saispace

Local project memory for AI coding agents. One SQLite database, three front ends:

- **`sai`** — CLI for memories, verifiable repo rules and stored secrets.
- **`saispace-mcp`** — MCP server (stdio) exposing the same data to agents.
- **Desktop app** — Tauri UI over the same database.

The database lives at `~/.local/share/saispace/saispace.db` (override with `SAISPACE_DB_PATH`). Anything saved from one front end is visible in the others. Projects are resolved from the working directory (git root) and registered on the first save.

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

Prebuilt `sai` binaries for linux-x64 and macos-arm64 are attached to each GitHub release (`v*` tags).

## Tests

```sh
cargo test --workspace

cd apps/desktop
bun run check   # tsc --noEmit
bun run lint    # biome
```

## Benchmark

`sai bench` evaluates correction detection, Stop hook accuracy, FTS5 search quality, duplicate detection, session index size, and 1,000-memory latencies against deterministic fixtures in `bench/fixtures/` (never touching the user database).

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
```

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

Tools include `save_memory`, `search_memory`, `memory_context`, `get_memory`, `link_memories`, `consolidate_memories`, `delete_memory`, `list_projects`, `get_project_context`, `list_tasks`, `create_task`, `update_task`, `get_active_plan`, `create_goal`, `update_goal`, `complete_task`, `list_secrets` and `request_secret_access`. Example `save_memory` call:

```json
{
  "title": "SQLite as single store",
  "content": "CLI, MCP and desktop share one SQLite DB.",
  "kind": "decision",
  "topic_key": "architecture/storage"
}
```

The memory is scoped to the project of the server's working directory, or to `path` / `project_id` if given; `"global": true` makes it cross-project.

## Learning from mistakes

Agents save lessons when they happen: a user correction becomes a `preference`, an own mistake with a non-obvious cause a `bugfix` with that cause. Duplicate pairs listed under `consolidate` (CLI index: "Para consolidar") get merged with `consolidate_memories`. `sai setup claude` installs the Claude Code hooks: `sai memory context` on SessionStart and `sai memory nudge` on UserPromptSubmit and Stop (Stop blocks once per session when there was a correction or 3+ failed tools and nothing was saved). The OpenCode plugin adds the same reminder per user message.
