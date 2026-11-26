# saispace-rules

Motor de reglas verificables de saispace. Cada regla es un fichero TOML en
`.saispace/rules/<id>.toml`. El motor decide qué reglas aplican a cada fichero
(por globs) y, si la regla tiene `check`, busca infracciones línea a línea.

Lo usa el CLI: `sai check`, `sai rule add|list|rm` y `sai context <ficheros>`.

## Formato de una regla

```toml
id = "rust-no-unwrap"                 # obligatorio: a-z, 0-9 y '-'; igual al nombre del fichero
description = "Sin .unwrap() en producción"  # obligatorio: el mensaje de cada hallazgo
severity = "error"                    # "error" (por defecto) o "warn"
files = ["crates/*/src/**/*.rs"]      # obligatorio: globs relativos a la raíz del repo
exclude = ["crates/*/src/bin/**"]     # opcional
fix = "Propaga el error con ?"        # opcional: se copia en cada hallazgo
guide = "Lee docs/errores.md"         # opcional: texto para agentes (sai context)

[check]                               # opcional si hay guide
kind = "forbid"
pattern = '\.unwrap\(\)'
stop_at = '^\s*#\[cfg\(test\)\]'
```

Los campos desconocidos son un error. Una errata como `severty = "warn"` ya no se
ignora en silencio.

## Tipos de regla

### `forbid`: regex prohibida por línea

Recorre las líneas del fichero y reporta la **primera** coincidencia de cada línea.
Si hay `stop_at`, deja de mirar desde la primera línea que cumpla esa regex
(útil para saltarse `#[cfg(test)]`).

Entrada (`crates/core/src/lib.rs`):

```rust
fn a() {
    let v = x.unwrap();
    let w = y.unwrap(); // sai-ignore: rust-no-unwrap
}
#[cfg(test)]
mod tests { fn t() { q.unwrap(); } }
```

Salida (`sai check --json`):

```json
{
  "files_scanned": 1,
  "rules": 1,
  "findings": [
    {
      "rule": "rust-no-unwrap",
      "severity": "error",
      "path": "crates/core/src/lib.rs",
      "line": 2,
      "column": 14,
      "message": "Sin .unwrap() en producción",
      "fix": "Propaga el error con ?",
      "snippet": "let v = x.unwrap();"
    }
  ]
}
```

`line` y `column` empiezan en 1. `column` cuenta bytes, no caracteres.
`snippet` es la línea sin espacios a los lados, cortada a 160 caracteres.

### `require`: regex que debe aparecer en el fichero

Se evalúa sobre el texto completo. Para anclar a inicio de línea usa `(?m)^...`.

```toml
id = "license-header"
description = "Cabecera SPDX obligatoria"
severity = "warn"
files = ["**/*.ts"]

[check]
kind = "require"
pattern = '(?m)^// SPDX-License-Identifier: '
```

Si falta, da **un** hallazgo en la línea 1, columna 1, con la primera línea como
`snippet`. Un fichero vacío también falla.

### `max_lines`: tope de líneas por fichero

```toml
[check]
kind = "max_lines"
max = 300
```

Con 412 líneas da un hallazgo en la línea 301 con `snippet = "412 lines (max 300)"`.

### Solo guía (sin `check`)

Una regla con `guide` y sin `check` nunca da hallazgos. Sirve para que
`sai context <fichero>` muestre al agente qué leer antes de tocar esos ficheros.

## Silenciar un hallazgo

`sai-ignore: <id>` en la misma línea o en la anterior silencia ese `forbid`.
Admite varios ids separados por comas o espacios: `// sai-ignore: a, b`.
No afecta a `require` ni a `max_lines`.

## Varias reglas a la vez (no hay prioridad)

No existe prioridad entre reglas. Todas las reglas cuyos globs encajan con un
fichero se ejecutan y reportan por separado, aunque coincidan en la misma línea.
Una regla `error` no oculta una `warn`.

`check_tree` ordena los hallazgos por `path`, luego `line` y luego `rule` (id).
`Report::errors()` y `Report::warnings()` cuentan por severidad. El CLI falla con
errores; con `--strict`, también con avisos.

## Recorrido del repositorio

`Engine::check_tree(root, only)`:

- Respeta `.gitignore` e incluye ficheros ocultos, salvo `.git/`.
- `only` limita a rutas absolutas (ficheros o carpetas). Vacío = todo.
- Solo lee ficheros a los que aplica alguna regla.
- Salta sin avisar los ficheros de más de 2 MiB y los que no son UTF-8.
  Aun así cuentan en `files_scanned`.
- Analiza en paralelo (rayon).

## Errores (`RulesError`)

| Variante | Cuándo |
| --- | --- |
| `Parse { path }` | TOML mal formado, campo desconocido, `kind` o `severity` inválidos |
| `Invalid(id, motivo)` | id con caracteres no válidos, `description` vacía, `files` vacío o con un glob vacío, sin `check` ni `guide`, patrón o `stop_at` vacío, `max = 0`, id repetido en `Engine::new`, fichero cuyo nombre no coincide con el id |
| `Regex { rule }` / `Glob { rule }` | patrón o glob que no compila |
| `DuplicateDefinition` | dos ficheros con el mismo id (en la práctica lo para antes la comprobación de nombre) |
| `Duplicate(id)` | `write_rule` sin `force` sobre un id existente |
| `NotFound(id)` | `remove_rule` de un id que no existe o no es válido |
| `Io` / `Serialize` | fallos de disco o de escritura del TOML |

## API

```rust
use saispace_rules::{Engine, find_root, load_rules};

let root = find_root(&std::env::current_dir()?);
let engine = Engine::new(&load_rules(&root)?)?;
let report = engine.check_tree(&root, &[]);
let findings = engine.check_text("src/lib.rs", "fn a() { x.unwrap(); }");
let applicable = engine.rules_for("src/lib.rs");
```

Tests: `cargo test -p saispace-rules`. Casos reales en `tests/scenarios.rs`.
