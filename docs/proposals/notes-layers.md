# Propuesta: notas de proyecto en 3 capas

Estado: borrador · 2026-11-27 · rama de partida `feat/ui-polish`

Objetivo: documentación de proyecto que conviva con cualquier repo sin tocar sus ficheros, en tres capas separadas y trazables:

| Capa | Quién escribe | Coste | Se sobrescribe sola |
|---|---|---|---|
| 1. Estática | Personas y agentes (vía propuesta) | 0 | Nunca. Solo con aprobación explícita |
| 2. Hechos (checkers) | Código determinista de saispace | 0 tokens | Sí: es derivada, se regenera |
| 3. Síntesis IA | El agente que llama por MCP/CLI | Tokens, solo si cambia el hash | Se versiona por snapshot, nunca pisa la 1 |

## 1. Encaje en la arquitectura actual

### Piezas que ya existen y se reutilizan

| Pieza | Dónde | Papel en esta propuesta |
|---|---|---|
| `detect_technologies` | `crates/core/src/tech.rs` | Primer checker (`stack`) casi hecho: manifiestos, versiones de lock, límites `MAX_DEPTH`/`MAX_MANIFESTS` |
| `sync_project_stack` | `crates/core/src/db/technology.rs:69` | Patrón "refresh perezoso, escribe solo si cambia" que se generaliza. Hoy compara con `same_stack` y hace `DELETE`+`INSERT`: sin historial ni hash |
| `SaiSpaceServer::sync_stack` | `crates/mcp/src/server.rs:75` | Punto donde el MCP ya refresca al consultar contexto; se amplía a todos los checkers |
| `search_index` (FTS5) | migración `20261124…` | Indexar notas estáticas e IA con `kind = 'note' / 'ai_note'` |
| `projects.context_notes` | migración `init` | Proto nota estática (solo FTS y `EntityKind::ContextNotes`). Se migra a la primera nota estática y se deja de usar |
| `activity_events` + `Actor` | `db/activity.rs` | Trazabilidad de quién aprobó, editó o envió síntesis |
| `render_context` + `CONTEXT_BUDGET` | `memory_render.rs` | Una línea de estado de notas en el contexto de inicio, dentro del presupuesto |
| `memory_stale.rs` | `db/` | Mismo principio: informe de revisión, nunca borrado automático |
| `rules` (`sai check`) | `crates/rules` | No se mezcla: `sai check` valida código contra reglas que viven en el repo (`.agents/checks`). Los checkers extraen hechos y no escriben nada en el repo. Sí se reutiliza `globset` para entradas |
| Vault de claves | `vault.rs`, `db/secret.rs` | El checker `env` solo guarda nombres; puede cruzar "variable usada sin clave en el vault" (fase 5) |

### Dónde va cada cosa

```
crates/checkers/            (nuevo, puro: sin DB, sync, testeable con fixtures)
  src/lib.rs                trait Checker, Fact, SourceRef, canonical_json, hash
  src/repo.rs               RepoView: git ls-files -s -z, git status, fallback walk
  src/stack.rs              mueve tech.rs (core re-exporta detect_technologies)
  src/env.rs, layout.rs
  src/diff.rs               FactDiff por clave, detección de renombres por blob
  src/drift.rs              extracción de afirmaciones de Markdown + cruce
crates/core/src/db/
  facts.rs                  snapshots, estado del proyecto, refresh_facts()
  notes.rs                  notas estáticas, revisiones, propuestas
  ai_notes.rs, drift.rs
crates/cli/src/notes.rs     sai notes …, sai facts …
crates/mcp/src/tools/notes.rs
apps/desktop: pestaña "Notas" + commands/notes.rs
```

- Crate aparte porque los checkers son lógica pura y determinista: se testean sin SQLite y se pueden medir en `sai bench`.
- Dependencias nuevas: `blake3` (hash) y `globset` (ya en `rules`). Git por CLI (`std::process::Command`), sin `git2`/`gix`, para no inflar el binario. Sin git: recorrido de ficheros con la lista `IGNORED_DIRS` actual.

### Frontera memorias / notas

| | Memorias | Notas |
|---|---|---|
| Naturaleza | Lecciones episódicas del agente: bugfix, preferencia, decisión puntual | Documentación curada del proyecto: specs, decisiones de negocio, estado |
| Escritura | Agente directo (`save_memory`), upsert por `topic_key` | Estática: persona directa, agente solo por propuesta. IA: solo por job con hash |
| Vida | Envejecen (stale, recall por triggers) | Versionadas; la estática no caduca, la marca la deriva |
| Tamaño | Corta (What/Why/Where) | Larga, Markdown |
| Anclaje | Triggers de errores, `env:<host>` | Anclas a hechos (`env:DATABASE_URL`, `path:crates/core`) |

Regla práctica para agentes: "si sirve para no repetir un error, memoria; si describe cómo es o debe ser el proyecto, nota". Una memoria `decision` puede promoverse a nota estática mediante propuesta (fase 5).

## 2. Modelo de datos (migración `20261205000000_notes_layers.sql`)

Convenciones del repo: ids `TEXT` (uuid v7), fechas `TEXT` RFC 3339, `ON DELETE CASCADE` por proyecto.

```sql
-- Capa 2: hechos. Snapshots direccionados por contenido.
CREATE TABLE fact_snapshots (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    checker_id TEXT NOT NULL,              -- 'stack' | 'env' | 'layout' …
    checker_version INTEGER NOT NULL,
    hash TEXT NOT NULL,                    -- blake3(id, version, canonical(key,value))
    fact_count INTEGER NOT NULL,
    truncated INTEGER NOT NULL DEFAULT 0,
    git_head TEXT, git_branch TEXT,        -- metadatos, fuera del hash
    dirty INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    UNIQUE (project_id, checker_id, hash)  -- volver a una rama reutiliza la fila
);

CREATE TABLE facts (
    snapshot_id TEXT NOT NULL REFERENCES fact_snapshots(id) ON DELETE CASCADE,
    key TEXT NOT NULL,                     -- 'env:DATABASE_URL', 'dep:cargo:sqlx', 'path:crates/core'
    value TEXT NOT NULL,                   -- JSON canónico, nunca valores de secretos
    value_hash TEXT NOT NULL,
    sources TEXT NOT NULL,                 -- JSON [{path,line,blob}], fuera del hash
    PRIMARY KEY (snapshot_id, key)
);
CREATE INDEX idx_facts_key ON facts(key);

-- Estado por checker: huella de entradas para no ejecutar si nada cambió.
CREATE TABLE checker_state (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    checker_id TEXT NOT NULL,
    input_fingerprint TEXT NOT NULL,
    snapshot_id TEXT NOT NULL REFERENCES fact_snapshots(id),
    checked_at TEXT NOT NULL,
    PRIMARY KEY (project_id, checker_id)
);

-- Estado combinado = hash de los hashes de todos los checkers.
CREATE TABLE project_states (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    state_hash TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE (project_id, state_hash)
);
CREATE TABLE project_state_snapshots (
    state_id TEXT NOT NULL REFERENCES project_states(id) ON DELETE CASCADE,
    snapshot_id TEXT NOT NULL REFERENCES fact_snapshots(id),
    PRIMARY KEY (state_id, snapshot_id)
);
CREATE TABLE project_state_head (
    project_id TEXT PRIMARY KEY NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    state_id TEXT NOT NULL REFERENCES project_states(id),
    refreshed_at TEXT NOT NULL
);

-- Capa 1: notas estáticas, con historial inmutable.
CREATE TABLE static_notes (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    origin TEXT NOT NULL CHECK (origin IN ('manual', 'repo_link')),
    source_path TEXT,                      -- repo_link: fichero del repo, solo lectura
    source_blob TEXT,                      -- blob git o blake3 del contenido leído
    revision INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'archived')),
    created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE static_note_revisions (
    note_id TEXT NOT NULL REFERENCES static_notes(id) ON DELETE CASCADE,
    revision INTEGER NOT NULL,
    title TEXT NOT NULL, body TEXT NOT NULL,
    actor_kind TEXT NOT NULL, actor_name TEXT,
    proposal_id TEXT,                      -- si vino de una propuesta aprobada
    created_at TEXT NOT NULL,
    PRIMARY KEY (note_id, revision)
);

-- Cambios propuestos (agente o deriva). Nunca se aplican solos.
CREATE TABLE note_proposals (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    note_id TEXT REFERENCES static_notes(id) ON DELETE CASCADE, -- NULL = nota nueva
    base_revision INTEGER,                 -- conflicto si la nota avanzó
    title TEXT NOT NULL, body TEXT NOT NULL,
    reason TEXT NOT NULL,
    drift_alert_id TEXT,
    state_id TEXT REFERENCES project_states(id),
    actor_kind TEXT NOT NULL, actor_name TEXT,
    status TEXT NOT NULL CHECK (status IN ('pending', 'approved', 'rejected', 'conflict')),
    created_at TEXT NOT NULL, decided_at TEXT
);

-- Anclas: qué hechos respalda una nota y con qué valor se revisó por última vez.
CREATE TABLE note_anchors (
    note_id TEXT NOT NULL REFERENCES static_notes(id) ON DELETE CASCADE,
    fact_key TEXT NOT NULL,                -- clave exacta o prefijo terminado en '*'
    reviewed_value_hash TEXT,
    reviewed_at TEXT,
    origin TEXT NOT NULL CHECK (origin IN ('explicit', 'extracted')),
    PRIMARY KEY (note_id, fact_key)
);

-- Capa 3: síntesis IA, siempre atada a un estado.
CREATE TABLE ai_notes (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    scope TEXT NOT NULL,                   -- 'project' | 'package:<name>'
    kind TEXT NOT NULL CHECK (kind IN ('state', 'architecture', 'impact')),
    body TEXT NOT NULL,
    state_id TEXT NOT NULL REFERENCES project_states(id),      -- snapshot fuente
    base_state_id TEXT REFERENCES project_states(id),          -- desde qué diff
    agent_name TEXT NOT NULL, model TEXT,
    created_at TEXT NOT NULL,
    UNIQUE (project_id, scope, kind, state_id)
);

-- Deriva: alertas, nunca ediciones.
CREATE TABLE drift_alerts (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    note_id TEXT REFERENCES static_notes(id) ON DELETE CASCADE,  -- NULL: alerta de proyecto (p. ej. `undocumented` sin notas)
    kind TEXT NOT NULL,                    -- ver §5
    fact_key TEXT NOT NULL,
    evidence TEXT NOT NULL,                -- JSON: afirmación, línea de la nota, hecho actual
    state_id TEXT NOT NULL REFERENCES project_states(id),
    status TEXT NOT NULL CHECK (status IN ('open', 'dismissed', 'resolved')),
    created_at TEXT NOT NULL, resolved_at TEXT
);
CREATE UNIQUE INDEX idx_drift_open ON drift_alerts(project_id, COALESCE(note_id, ''), kind, fact_key) WHERE status = 'open';
```

- Triggers FTS como los de `memories`: `static_notes` y `ai_notes` en `search_index`.
- Migración de datos: cada `projects.context_notes` no vacío pasa a `static_notes` (`title = 'Contexto'`, `origin = 'manual'`).
- Retención: por checker se guardan los últimos 20 snapshots más cualquiera referenciado por `ai_notes`, `drift_alerts`, propuestas o `project_state_head`. GC al final de cada refresh, nunca en segundo plano, en este orden: 1) borrar `project_states` no protegidos (ni head ni citados por síntesis, alertas o propuestas) y sus filas de `project_state_snapshots`; 2) borrar snapshots que ya no cita ningún estado y que además estén fuera de los últimos 20 de su checker.
- La capa 2 no tiene tabla de "notas": su texto se renderiza en lectura desde `facts`, siempre igual para el mismo hash.

## 3. Contrato de checker

```rust
pub trait Checker: Send + Sync {
    /// Estable y corto: forma parte de las claves y del hash.
    fn id(&self) -> &'static str;
    /// Subir cuando cambie la semántica de la salida: invalida el hash.
    fn version(&self) -> u32;
    /// Globs de entrada para la huella barata (salta la ejecución si no cambian).
    fn inputs(&self) -> &'static [&'static str];
    fn run(&self, repo: &RepoView) -> Result<CheckerOutput, CheckerError>;
}

pub struct RepoView {
    pub root: PathBuf,
    pub files: Vec<RepoFile>,       // ordenados, rutas relativas con '/'
    pub git: Option<GitInfo>,       // head, branch, dirty
    pub limits: Limits,             // max_files, max_facts, max_file_bytes, budget
}
pub struct RepoFile { pub path: String, pub blob: Option<String>, pub size: u64 }

pub struct Fact {
    pub key: String,                // "<tipo>:<scope?>:<nombre>", única por checker
    pub value: serde_json::Value,   // sin rutas absolutas, sin fechas, sin secretos
    pub sources: Vec<SourceRef>,
}
pub struct SourceRef { pub path: String, pub line: Option<u32>, pub blob: Option<String> }

pub struct CheckerOutput { pub facts: Vec<Fact>, pub truncated: bool, pub warnings: Vec<String> }
```

El framework, no el checker, garantiza el determinismo:

- Ordena `facts` por `key` y rechaza duplicados.
- `canonical_json`: ordena claves de objetos de forma recursiva. Obligatorio porque el workspace activa `serde_json/preserve_order`.
- `hash = blake3(id ‖ version ‖ truncated ‖ Σ key ‖ canonical(value))`. `sources` y `git_head` quedan fuera: mover una línea o hacer un commit sin cambio semántico no despierta a la capa 3. Las `sources` se actualizan en la fila existente.
- Guardia de secretos sobre cada `value` antes de guardar o servir: rechaza el hecho si parece credencial (prefijos `sk-`, `ghp_`, `xox`, `AKIA`, JWT, alta entropía en cadenas de 20 caracteres o más). Test obligatorio.
- `spawn_blocking` + presupuesto de tiempo por checker (2 s por defecto). Si se excede: `truncated = true`, y lo parcial no cuenta como cambio.

### Checkers del MVP

| Checker | Hechos (claves) | Por qué primero | Coste |
|---|---|---|---|
| `stack` | `dep:<eco>:<name>` → `{version, scope}`, `runtime:<name>` | El 80 % ya existe en `tech.rs`. Deriva clásica: "usamos React 18" | Bajo: envolver y añadir `scope` de workspace |
| `env` | `env:<NAME>` → `{required, scopes:[…]}` | Las variables de entorno son la deriva más común en README/specs y bloquean el arranque. Agnóstico vía regex | Bajo-medio: regex por lenguaje y guardia de secretos |
| `layout` | `path:<dir>` (2 niveles + miembros de workspace), `pkg:<name>` → `{path, kind}`, `entry:<bin>`, `cmd:<runner>:<name>` (scripts de `package.json`, targets de `Makefile`/`justfile`, `[[bin]]`), `migration:<file>` | Las rutas y comandos citados en docs se pudren con cada refactor. Base para monorepos (scopes) | Bajo: solo `git ls-files` y manifiestos ya parseados |

Fuentes de `env` (solo nombres, nunca el lado derecho):

- Plantillas: `.env.example`, `.env.sample`, `.env.template`, `.env.dist`. Nunca se abren `.env` ni `.env.local`.
- Código: `process.env.X`, `import.meta.env.X`, `std::env::var("X")`, `env!("X")`, `os.environ["X"]`, `os.getenv("X")`, `os.Getenv("X")`, `ENV["X"]`.
- Infra: claves `environment:` de `docker-compose*.yml`, `env:` en `.github/workflows/*.yml` (los nombres de `${{ secrets.X }}`).

Descartados para el MVP:

- `schema` (DDL SQL, Prisma, OpenAPI): mucho valor, pero parsers por formato. Va en la fase 5.
- AST (tree-sitter): dependencia pesada y por lenguaje.
- `git` como checker: el head entra como metadato, no como hecho. Si no, cada commit cambiaría el hash.

## 4. Flujo de refresh

Disparadores, todos perezosos y nunca por temporizador:

- `sai facts`, `sai notes …`.
- MCP `get_project_context` y `get_notes`, donde hoy ya se llama a `sync_stack`.
- Abrir la pestaña Notas en la app, sin botón de refrescar.
- Opcional, fase 5: hook `post-checkout`/`post-merge` instalado solo con `sai setup git-hooks`, que escribe en `.git/hooks` (no versionado) por acción explícita.

```
refresh_facts(project)
 1. Lock por proyecto (BEGIN IMMEDIATE sobre project_state_head) para que CLI, MCP y app no corran a la vez.
 2. RepoView una sola vez: git ls-files -s -z (+ untracked no ignorados), git status --porcelain=v2 -z.
 3. Por checker:
    a. fp = blake3(version ‖ (path, blob) de los ficheros que casan con inputs()); para los modificados
       o sin seguimiento según `git status`, el blob se sustituye por el hash de su contenido en disco,
       y los borrados sin stage entran como (path, "deleted") (el índice no ve cambios sin `git add`).
    b. fp == checker_state.input_fingerprint → reutiliza snapshot, no ejecuta.
    c. si no: run → canonicaliza → hash.
       hash == hash del snapshot actual → solo actualiza fp, checked_at y sources.
       existe (project, checker, hash) → reapunta (cambio de rama, revert).
       si no → INSERT snapshot + facts.
 4. state_hash = blake3(Σ checker_id:hash ordenado). Igual que el head → {changed:false}, fin.
 5. Si cambió: upsert project_states, mover head, drift determinista (§5), activity (EntityKind::Facts).
 6. Devuelve {state_hash, changed, checkers:[{id, hash, prev_hash, added, removed, changed}]}.
```

### Capa 3: puerta de hash y quién ejecuta la IA

saispace nunca llama a un modelo ni guarda API keys. Sintetiza el agente que ya está en sesión, y solo cuando lo pide la tarea o el usuario.

```
agente → get_synthesis_job(project, scope, kind)
  refresh_facts()                                       (barato, 0 tokens)
  ai = ai_note(scope, kind, state = head) ?? última ai_note(scope, kind)
  ai.state_id == head            → {up_to_date: true, note}     ← PUERTA: nada más (también al volver a un estado ya sintetizado)
  ai existe y su estado es otro  → {base: ai.state, target: head,
                                    diff: FactDiff por checker,
                                    commits: git log --oneline base..head (≤ 30),
                                    files: git diff --name-status -M (≤ 200),
                                    current_note: ai.body, drift_candidates, instructions}
  sin ai                         → {target: head, facts: resumen por checker (con tope), instructions}
agente → submit_synthesis(target_state_hash, scope, kind, body)
  target != head actual → error "stale", hay que pedir otro job (concurrencia optimista)
  si no → INSERT ai_notes(state_id, base_state_id, agent_name, model, created_at)
```

- Nunca se envía el repo ni el contenido de los diffs: solo hechos estructurados y nombres de fichero/commit.
- Tope del job: 8 KB. Si el diff es mayor, van conteos por checker, los N primeros cambios y `sai facts diff --checker X` para el detalle.
- Paridad CLI para agentes sin MCP: `sai notes job --json` y `sai notes submit --state <hash> --file <md>`.
- El contexto de inicio (`render_context`) añade como mucho dos líneas: notas estáticas (título · id), alertas de deriva abiertas y "síntesis desactualizada (N cambios)". Nunca pide sintetizar: evita quemar tokens en cada sesión.

```rust
pub struct FactDiff {
    pub checker_id: String,
    pub from: Option<String>, pub to: String,
    pub added: Vec<Fact>, pub removed: Vec<Fact>,
    pub changed: Vec<(Fact, Fact)>,
    pub renamed: Vec<(String, String)>,   // por blob igual en sources (path:, migration:)
}
```

## 5. Deriva

Determinista primero (0 tokens). Se recalcula solo cuando cambia el estado o la revisión de una nota.

1. Extracción de afirmaciones del Markdown de cada nota estática activa (y de `repo_link`):
   - Rutas en código inline o enlaces relativos → `path:`.
   - Tokens `UPPER_SNAKE` de 3 caracteres o más que coinciden con el patrón de variable → `env:`.
   - Nombre de dependencia conocido seguido de versión (`tauri 2`, `react@18`) → `dep:`.
   - `npm|pnpm|bun run X`, `make X`, `just X`, `cargo run --bin X` → `cmd:`/`entry:`.
2. Cada afirmación extraída se guarda como ancla `extracted`. El usuario añade anclas `explicit` (clave exacta o prefijo `env:*`).
3. Cruce con el head:

| `kind` | Regla | Severidad |
|---|---|---|
| `missing_path` | Ruta citada que no existe; si hay renombre en `FactDiff`, la evidencia lleva el destino | alerta |
| `missing_env` | Variable citada que ningún checker encuentra | alerta |
| `version_mismatch` | Versión mayor citada distinta de la detectada | alerta |
| `missing_command` | Script o target citado que ya no existe | alerta |
| `anchor_changed` | `value_hash` actual distinto de `reviewed_value_hash` | revisar |
| `undocumented` | Hecho nuevo (env o paquete) que ninguna nota ancla. Agregado en una sola alerta por checker | info |

- Las alertas nunca modifican la nota. Se resuelven de tres formas: el usuario edita la nota (se cierra sola si la afirmación ya casa), la descarta, o la marca como revisada (actualiza `reviewed_value_hash`).
- Deriva semántica (contradicciones en prosa): solo en la capa 3. El job incluye `drift_candidates` (notas con anclas cambiadas y sus hechos), y el agente puede llamar a `propose_note` con el cuerpo completo nuevo y `reason`. Se guarda como `note_proposals.pending` y la UI muestra el diff contra `base_revision`. Aprobar crea una revisión. Si la nota avanzó, la propuesta pasa a `conflict`.

## 6. Exposición

MCP (4 herramientas, en línea con el "leaner MCP" reciente):

| Tool | Hace |
|---|---|
| `get_notes(project?, layer?, id?)` | Refresh perezoso; lista o abre notas de las 3 capas con su procedencia y alertas abiertas |
| `get_synthesis_job(scope?, kind?)` | Puerta de hash + diff estructurado (§4) |
| `submit_synthesis(state_hash, scope, kind, body)` | Guarda la síntesis atada al estado |
| `propose_note(note_id?, title, body, reason)` | Única vía de escritura del agente en la capa 1 |

CLI:

- `sai facts [--checker X] [--json]`
- `sai facts diff [<from> [<to>]]`
- `sai notes list|show|add|edit|archive`
- `sai notes import <path>` (`repo_link`, solo lectura)
- `sai notes export <dir>` (única escritura en el repo, explícita)
- `sai notes drift`, `sai notes proposals [approve|reject <id>]`, `sai notes job|submit`

Desktop, pestaña "Notas":

- Tres bloques con tratamiento visual distinto: estática (editable, historial), hechos (solo lectura, hash corto y enlace a fuente) y síntesis IA (etiqueta "IA · agente · fecha · estado abc123", marca de desactualizada).
- Alertas y propuestas con aprobar/rechazar.
- Sin botón de refresh, sin KPIs y sin textos de relleno (preferencias del usuario).

Salida trazable en todas las vías: cada bloque lleva `layer`, `source` (revisión, `checker@version#hash` o `ai:<agente>@<state_hash>`) y `at`.

## 7. Riesgos y casos límite

| Caso | Tratamiento |
|---|---|
| Monorepos | `scope` = miembro de workspace (Cargo `members`, `workspaces` de npm, `pnpm-workspace.yaml`, `go.work`). Claves con scope y `ai_notes.scope = package:<name>`. Subir `MAX_MANIFESTS` (64) y hacerlo configurable |
| Repos enormes | Solo `git ls-files` (respeta `.gitignore`), `max_files` 50 k, `max_file_bytes` 512 KB para escanear `env`, presupuesto por checker. `truncated` entra en el hash para que un recorte no parezca un cambio real |
| Ficheros generados | Excluir `linguist-generated`/`-vendored` de `.gitattributes`, cabeceras `@generated`/`DO NOT EDIT`, `dist/`, `build/`, `*.min.*`. Los locks son entradas de `stack`, no hechos de `layout` |
| Secretos en env | Solo nombres. No se abren `.env` reales. Guardia de valores en hechos, job y FTS. Test que busca el valor de un fixture en los bytes del `.sqlite` |
| Cambio de rama | Snapshots direccionados por contenido: volver a una rama reutiliza snapshot, estado y síntesis IA existentes, con 0 tokens. `git_branch` es solo metadato |
| Working tree sucio | Se analiza lo que hay en disco (`dirty = 1`). El job lo indica. La UI marca la síntesis hecha sobre estado sucio |
| Renombres | `git diff -M` entre heads, más el emparejamiento por blob en `FactDiff.renamed`. La alerta `missing_path` propone el destino |
| Repo sin git | Fallback de recorrido con `IGNORED_DIRS`. Blob = blake3 del contenido y huella por `size+mtime` |
| Ruta del proyecto movida | Rutas siempre relativas: el hash no cambia |
| Concurrencia CLI/MCP/app | Lock por proyecto, `submit_synthesis` con concurrencia optimista y `base_revision` en propuestas |
| Ruido de la capa 3 | Hash solo semántico (sin líneas ni head). La IA no se ejecuta sola nunca |
| Falsos positivos de deriva | Extracción conservadora, `undocumented` como info agregada. Medir precisión con fixtures en `sai bench` antes de mostrarla en el contexto de inicio |
| Crecimiento de la DB | Retención de §2 y facts sin duplicar entre estados (se comparten por snapshot) |
| Escritura en el repo | Prohibida salvo `sai notes export` y `sai setup git-hooks`. Test de árbol intacto en todas las fases |

## 8. Fases y criterios de aceptación

Cada fase pasa `cargo fmt`, `clippy` (también `-W clippy::pedantic`), tests, `sai check` y `sai bench --compare` sin regresión.

**F0 · Contrato y `stack`**

- Crate `checkers`, trait, `canonical_json`, `blake3`, `RepoView` (git CLI más fallback). `tech.rs` se mueve a `StackChecker` y `sync_project_stack` lo usa.
- Aceptación:
  - Mismo hash en dos ejecuciones, en una copia del fixture con otra ruta absoluta y con el orden de `read_dir` alterado.
  - `get_project_context` devuelve el mismo stack que hoy (test MCP existente verde).

**F1 · Snapshots, `env`, `layout`, `sai facts`**

- Migración (tablas de hechos y estado), `refresh_facts`, GC y `sai facts [diff]`. El MCP sustituye `sync_stack` por `refresh_facts`.
- Aceptación:
  - Un segundo refresh sin cambios escribe 0 filas (contar filas antes y después).
  - Cambiar de rama y volver no crea snapshots nuevos.
  - Un commit que solo mueve líneas no cambia `state_hash`.
  - El valor de `.env` del fixture no aparece en los bytes de la DB.
  - Refresh en caliente en el repo saispace < 150 ms. En frío, el tiempo queda registrado en el resumen del PR.

**F2 · Notas estáticas**

- Tablas `static_notes`, revisiones, propuestas y FTS. Migración de `context_notes`. CLI `notes` (list/show/add/edit/import/export). Pestaña Notas (estática + hechos). Línea en `render_context`.
- Aceptación:
  - Hash del árbol del repo idéntico antes y después de todos los comandos salvo `export`.
  - El MCP no tiene forma de modificar una nota sin propuesta.
  - Cada edición crea una revisión recuperable.
  - `render_context` sigue dentro de `CONTEXT_BUDGET`.

**F3 · Deriva determinista**

- Extracción, anclas, `drift_alerts`, `sai notes drift`, alertas en la UI.
- Aceptación:
  - Fixtures de `sai bench` con ruta borrada, ruta renombrada, variable eliminada, salto de versión mayor y script borrado: todas detectadas.
  - 0 falsos positivos en los docs del propio repo (`AGENTS.md`, `README`).
  - Ninguna alerta altera `static_notes`.

**F4 · Capa IA**

- `get_synthesis_job`, `submit_synthesis`, `propose_note`, equivalentes CLI, bloque IA y propuestas con diff en la UI.
- Aceptación:
  - Con el estado sin cambios, el job devuelve `up_to_date` sin diff.
  - Un `submit` con estado viejo se rechaza.
  - El job ocupa ≤ 8 KB en el fixture grande.
  - Cada `ai_note` muestra agente, fecha y `state_hash`.
  - `grep` de API keys o clientes HTTP de modelos en el workspace: 0.

**F5 · Extensión**

- Checkers `schema` (SQL/Prisma/OpenAPI) y `ci` (workflows), scopes de monorepo en la UI, `sai setup git-hooks` opcional, cruce de `env` con el vault y promoción de memoria `decision` a nota.
- Checkers declarativos (regex más globs, en la línea del motor de `rules`) guardados en saispace y no en el repo.
- Aceptación: cada checker nuevo trae fixture de determinismo y entrada en `sai bench`.

## 9. Decisiones abiertas

- Nombre de la capa 2 en la UI: "Hechos" o "Estado detectado".
- Si `repo_link` se re-sincroniza solo al cambiar el blob (propuesta: sí, porque es lectura del repo y no escritura) o con confirmación.
- Si las anclas `extracted` se guardan o se recalculan en cada pasada (propuesta: guardarlas para conservar `reviewed_value_hash`).
