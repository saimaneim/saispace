# Comparative Technical Analysis: saispace-mcp vs Engram Memory Standard

## Executive Summary

This document presents an exhaustive architectural and functional comparison between **saispace-mcp** (the local project/context control plane in the saispace suite) and **Engram** (the battle-tested cognitive persistent memory standard for autonomous AI agents). 

While both systems target local-first persistence using SQLite, their core design paradigms diverge fundamentally:
- **saispace-mcp** is currently structured as an **entity-relational workspace management layer** (Projects, Kanban Tasks, Monolithic Markdown Context Notes, and Audit Logging).
- **Engram** is purpose-built as an **epistemic, cross-session cognitive memory kernel** (Selective Persistence, Atomic Categorized Observations, Topic-Key Upserts, Semantic Conflict Resolution, Session Compaction Recovery, and FTS5 Token-Budgeted Retrieval).

To elevate `saispace-mcp` from a project task hub into a resilient cognitive engine capable of autonomous agent orchestration across multiple sessions and context compactions, `saispace-mcp` must adopt Engram's core architectural tenets while preserving its native desktop GUI synergy.

---

## 1. Architectural Paradigms & Identity

| Dimension | saispace-mcp (Current) | Engram Standard |
| :--- | :--- | :--- |
| **Primary Mission** | Local workspace command center (Projects, Tasks, Context Notes, Live Audit) | Cross-session cognitive persistence surviving LLM context compaction |
| **Epistemic Model** | Entity-relational (Projects 1:N Tasks, Projects 1:1 Context Notes) | Observational & Episodic Memory (Atomic observations, Sessions, Summaries) |
| **Memory Granularity** | Monolithic text blob (`context_notes TEXT`) per project | Atomic structured cards (`What`, `Why`, `Where`, `Learned`) categorized by Type |
| **Evolution Mechanism** | Destructive text overwrite or blind string concatenation (`append: true`) | Topic Keys (`topic_key`) with history tracking, supersession, and conflict surfacing |
| **Retrieval Strategy** | Full-payload fetch (`get_project_context`) | Multi-tiered: FTS5 BM25 search, byte-budgeted context recovery (`mem_context`), un-truncated hydration (`mem_get_observation`) |
| **Compaction Recovery** | Unaware of context resets; relies on full dump of notes | Structured compaction protocol (`mem_session_summary` -> `mem_context` recall) |
| **Observability / Health** | Unidirectional activity audit stream (`activity_events`) | Diagnostic engine (`mem_doctor`), passive extraction safety net (`mem_capture_passive`) |
| **Multi-Tenancy / Scopes** | Project-only (resolved via UUID or filesystem path) | Tri-level hierarchy: `project`, `personal`, `global`, plus orthogonal `org` groupings |

---

## 2. Tool-by-Tool Specification Comparison

### saispace-mcp (7 Tools)
1. `list_projects`: Lists all registered projects in the database.
2. `register_project`: Registers a directory path, project name, and optional initial notes.
3. `get_project_context`: Fetches project metadata, open tasks, and entire `context_notes` string.
4. `update_project_context`: Replaces or appends text to the project's monolithic markdown notes.
5. `list_tasks`: Retrieves tasks for a project, with optional status filter.
6. `create_task`: Adds a task to the project with priority and initial `todo` status.
7. `update_task`: Mutates task title, status (`todo`, `in_progress`, `blocked`, `done`, `cancelled`), or priority.

### Engram Standard (18+ Tools)
1. **Core Epistemic Persistence**:
   - `mem_save`: Proactive storage of verified conclusions using **What / Why / Where / Learned** schema and `topic_key`.
   - `mem_update`: Explicit update of an existing observation by ID, retaining history.
   - `mem_pin` / `mem_unpin`: Priority pinning of critical invariants into baseline recall.
2. **Search & Retrieval**:
   - `mem_search`: Full-text search (FTS5) with `match_mode` (`all` vs `any`), type filters, and compact envelopes.
   - `mem_context`: Recovers recent sessions and observations bounded by token/byte budgets (`max_bytes`, default 16KB, clamp 64KB).
   - `mem_get_observation`: Fetches un-truncated content and full metadata for specific observation IDs.
   - `mem_current_project`: Detects project identity from working directory.
3. **Session Lifecycle & Compaction**:
   - `mem_session_start` / `mem_session_end` / `mem_session_finalize`: Formal boundary tracking.
   - `mem_session_summary`: End-of-session and pre-compaction synthesis (**Goal, Instructions, Discoveries, Accomplished, Next Steps, Relevant Files**).
4. **Epistemic Integrity & Diagnostics**:
   - `mem_judge` / `mem_judge_many`: Resolves semantic contradictions (`supersedes`, `conflicts_with`, `compatible`, `scoped`).
   - `mem_doctor`: Diagnostic health check for DB integrity, vector/FTS indexes, and orphaned sessions.
   - `mem_capture_passive`: Heuristic extraction safety net from tool outputs and agent responses.

---

## 3. Storage Engine & Memory Data Structures

### saispace-mcp Storage Layer
Implemented in `crates/core/migrations/20261122000000_init.sql` using SQLite via `sqlx`:
- **`projects`**: Holds `context_notes TEXT NOT NULL DEFAULT ''`.
  - *Limitation*: No full-text search index (FTS5). Context is a single unstructured string. Memory scaling is $O(N)$ with respect to prompt tokens on every retrieval.
- **`tasks`**: Normalized task items with status, priority, and timestamps.
- **`activity_events`**: Audit log recording `(actor_kind, actor_name, action, entity, entity_id, details, created_at)`.
  - *Limitation*: `details` is raw `TEXT` parsed with strict `serde_json::from_str`. Non-JSON strings crash the query pipeline.

### Engram Storage Layer (Target Architecture)
- **`observations`**:
  - `id UUID PRIMARY KEY`, `project_id TEXT`, `scope TEXT` (`project`|`personal`|`global`), `topic_key TEXT`, `type TEXT` (`decision`|`architecture`|`bugfix`|`pattern`|`config`|`discovery`|`learning`), `title TEXT`, `what TEXT`, `why TEXT`, `where_paths TEXT`, `learned TEXT`, `pinned BOOLEAN`, `superseded_by UUID`, `created_at`, `updated_at`.
- **`observations_fts` (Virtual Table FTS5)**:
  - BM25 ranked indexing over `title`, `what`, `why`, `where_paths`, and `learned`.
  - Token-sanitized queries supporting exact phrases and boolean operators.
- **`session_summaries`**:
  - `id UUID PRIMARY KEY`, `session_id TEXT`, `project_id TEXT`, `goal TEXT`, `instructions TEXT`, `discoveries TEXT`, `accomplished TEXT`, `next_steps TEXT`, `relevant_files TEXT`, `created_at`.
- **`memory_conflicts`**:
  - Tracks candidate collisions detected during `topic_key` updates requiring agent/user judgment.

---

## 4. Detailed Gap Analysis

### Gap 1: Context Degradation & The Monolithic Context Blob
- **Symptom**: `update_project_context` appends text: `format!("{}\n\n{}", existing, new_notes)`.
- **Impact**: As coding sessions progress, `context_notes` accumulates duplicate instructions, obsolete decisions, and rambling progress notes. When passed to the LLM, context window waste explodes and attention degradation ("lost in the middle") occurs.
- **Engram Solution**: Atomic observations with explicit deletion or supersession (`topic_key`). Outdated observations are marked superseded and excluded from active context.

### Gap 2: Zero Token-Budgeting or Retrieval Filtering
- **Symptom**: `get_project_context` has no parameters for budget, query, or limit.
- **Impact**: An agent entering a project with 30KB of notes must ingest all 30KB on turn 1. If the project grows to 100KB, it exceeds small model context limits and incurs heavy latency/cost.
- **Engram Solution**: `mem_context` accepts `max_bytes` (default 16KB, clamped to 64KB) and returns only active pins, recent summaries, and high-relevance observations. `mem_search` provides targeted retrieval on demand.

### Gap 3: Compaction Ignorance & Session Resets
- **Symptom**: `saispace-mcp` has no session lifecycle tools. When an IDE agent undergoes context truncation or compaction, all in-flight working memory is lost unless manually pasted into `context_notes`.
- **Impact**: Discontinuity between agent iterations; loss of discoveries and next steps.
- **Engram Solution**: Explicit Compaction Protocol:
  1. Compaction warning -> `mem_session_summary` persists structured milestone state.
  2. Post-compaction trigger -> `mem_context` restores state and pending tasks without manual re-prompting.

### Gap 4: Semantic Conflicts & Contradictions
- **Symptom**: If Agent A decides "Use JWT authentication" and Agent B later writes "Switched to Session Cookies", both statements remain in `context_notes` with no reconciliation.
- **Engram Solution**: Topic keys (`architecture/auth`) flag collisions. Engram surfaces `judgment_required` and prompts the agent to declare whether the new observation `supersedes`, `conflicts_with`, or is `compatible` with the existing record.

---

## 5. Strategic Roadmap for saispace-mcp

```
               ┌────────────────────────────────────────────────────────┐
               │                saispace Evolutionary Plan               │
               └────────────────────────────────────────────────────────┘
                                           │
         ┌─────────────────────────────────┴─────────────────────────────────┐
         ▼                                                                   ▼
┌─────────────────────────────────┐                         ┌─────────────────────────────────┐
│     Phase 1: DB & Core Layer    │                         │  Phase 2: Cognitive MCP Tools   │
│  - SQLite Schema Evolution      │                         │  - mem_save (Atomic / Topics)   │
│  - FTS5 Full-Text Search        │ ──────────────────────► │  - mem_search (BM25 Ranked)     │
│  - Resilient JSON & Deserializ. │                         │  - mem_context (Budget-clamped) │
│  - Observations & Sessions tbls │                         │  - mem_session_summary          │
└─────────────────────────────────┘                         └─────────────────────────────────┘
                                                                             │
         ┌───────────────────────────────────────────────────────────────────┘
         ▼
┌─────────────────────────────────┐                         ┌─────────────────────────────────┐
│   Phase 3: Desktop UI Synergy   │                         │  Phase 4: Conflict Engine & Auto│
│  - "Memory & Decisions" Tab     │                         │  - Semantic Conflict Detection  │
│  - Visual Timeline of Learnings │ ──────────────────────► │  - Passive Response Capture     │
│  - Interactive Conflict Judge   │                         │  - Cross-agent Collaboration    │
└─────────────────────────────────┘                         └─────────────────────────────────┘
```

### Phase 1: Storage Layer Evolution (`saispace-core`)
1. **Resilient Deserialization**:
   - Patch `ActivityEventRow::into_event` to safely handle non-JSON strings, empty payloads, and whitespace.
2. **Schema Migration `20261124000000_memory.sql`**:
   - Add `observations` table with topic keys and categories.
   - Add SQLite FTS5 table `observations_fts` with external content triggers.
   - Add `session_summaries` table.

### Phase 2: MCP Tool Implementation (`saispace-mcp`)
1. Implement `mem_save`:
   - Validates 4-part structure (**What, Why, Where, Learned**).
   - Upserts on `(project_id, topic_key)`.
2. Implement `mem_search`:
   - Executes FTS5 MATCH queries with snippet generation and BM25 ranking.
3. Implement `mem_context`:
   - Bounded by `max_bytes` (default 16384 bytes). Emits pinned observations, last session summary, and top recent items.
4. Implement `mem_session_summary`:
   - Records structured milestones.

### Phase 3: Desktop UI Integration (`apps/desktop`)
1. Create new **Memory Tab** alongside Tasks, Context, and Live Audit.
2. Display memory cards grouped by type (`decision`, `bugfix`, `architecture`, `pattern`).
3. Search bar wired to the backend FTS5 query command.

### Phase 4: Autonomous Conflict Resolution & Passive Safety
1. Implement candidate conflict detection on topic-key upserts.
2. Add `mem_doctor` diagnostic tool to check SQLite integrity and index health.
