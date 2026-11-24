---
name: saispace
description: Use in any project registered in saispace. Defines how to read and store project memory (replaces engram), tasks, stack and tech rules through the saispace MCP, and how to add and run verifiable code rules with the `sai` CLI.
---

# saispace

saispace is the local memory and rules hub. The MCP server `saispace` and the `sai` CLI share one SQLite database, so anything saved from one is visible in the other and in the desktop app.

## Session start

1. `get_project_context` with the project path: context notes, open tasks, `stack` with exact versions, and the `tech_rules` that match that stack. Follow those rules.
2. `memory_context` (defaults to the working directory): an index with the user's preferences, the last session summary and recent memories with short previews. Open what you need with `get_memory`.
3. Memories are scoped to the project: the registered project containing the path, else its git root. Saving the first memory registers it; `register_project` is only needed for tasks and stack.

From a shell, `sai memory context` prints the same index for the current directory and `sai memory get <id>` opens one memory.

`save_memory` returns `similar` memories: merge into the existing one (same `topic_key`) or delete the stale one. Use kind `preference` for the user's standing rules; they always appear at session start. `search_memory` stays in the current project plus global entries; pass `all: true` to search every project.

## Memory

Save only knowledge that is durable and costly to rebuild. Group related conclusions in one memory.

| Tool | Use |
|---|---|
| `save_memory` | `title`, `content` (What, Why, Where, Learned), `kind` (decision, architecture, bugfix, pattern, config, discovery, preference, session_summary, note), `topic_key` for things that evolve. The same `topic_key` in the same project replaces the old memory. `global: true` for cross-project knowledge. |
| `search_memory` | FTS5 search over memories, context notes, tasks and tech rules. Search before asking the user to repeat something. |
| `get_memory` | Full content of one memory. |
| `delete_memory` | Remove a memory that turned out to be wrong. |

Never store secrets, raw logs, routine progress, or anything the code or git history already shows. At the end of a significant session, save one `session_summary` with Goal, Discoveries, Accomplished, Next steps.

## Tasks and stack

| Item | Tool |
|---|---|
| Work to do | `create_task`, then `update_task` to `in_progress` and to `done` only when verified. |
| Stack | `sync_project_stack` after dependencies change. Never type versions by hand. |
| Rules about a technology | `upsert_tech_rule`: one short sentence per dependency name and optional version prefix (`tauri`, `2`). |

## Secrets

The user stores tokens and API keys in saispace (app: Global > Claves). Agents never see or print values.

1. `list_secrets` (MCP) or `sai secret list`: names and your access (approved, pending, denied, none).
2. No access: `request_secret_access` with a short `reason`, or `sai secret request NAME --reason "..."`. Tell the user to approve it in the app. Approval is per agent and lasts until revoked.
3. Use it: `sai secret run -s NAME[=ENV_VAR] -- <command>`. The value is injected as an env var and masked as `••••••` in output. Exit code 3 means access is still missing.

Never echo, log, write to files or save to memory a secret value. Never ask the user to paste a secret in chat when a stored one exists.

## Verifiable code rules (`sai`)

When a correction will repeat and can be checked with a regex, a required pattern or a line limit, turn it into a rule instead of prose. Rules live in `.saispace/rules/<id>.toml` and cost zero prompt tokens.

```sh
sai rule add --id rust-no-unwrap \
  --description "Sin .unwrap() en código de producción" \
  --files 'crates/*/src/**/*.rs' \
  --forbid '\.unwrap\(\)' --stop-at '^\s*#\[cfg\(test\)\]' \
  --fix "Propaga el error con ? y un error tipado"
sai check                 # exit 0 ok, 1 errors, 2 config error
sai check --json src/     # machine output for agents
sai context src/app.tsx   # rules and guides that apply to a file
sai rule list | sai rule rm <id>
```

- Kinds: `--forbid <regex>` (per line, optional `--stop-at`), `--require <regex>` (per file), `--max-lines <n>`.
- `--guide "<text>"` adds scoped guidance shown by `sai context`, for rules that cannot be checked by code. A rule can have only a guide.
- `--severity warn` for advisory rules; `sai check --strict` fails on warnings too.
- `rule add` prints how many current violations the rule finds. Fix them or narrow the globs before finishing.
- Silence one line on purpose with `sai-ignore: <rule-id>` on that line or the line above.

Run `sai check` before declaring work done.
