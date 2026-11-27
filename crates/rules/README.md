# saispace-rules

Motor de reglas verificables de saispace. Cada regla es un fichero TOML en
`.agents/checks/<id>.toml` (o el antiguo `.saispace/rules/`). El motor decide qué reglas aplican a cada fichero
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

#### Opciones de `forbid`

| Campo | Efecto |
| --- | --- |
| `multiline = true` | La regex se aplica al texto entero (puede cruzar líneas). El hallazgo cae en la línea donde empieza la coincidencia. `'\A'` prohíbe un fichero entero. |
| `unless_line` | Salta las coincidencias cuya línea cumpla esta regex. |
| `except` | Descarta las coincidencias cuyo propio texto cumpla esta regex. |
| `within` | Solo busca dentro de los tramos que encaje esta regex (su grupo 1 si lo tiene, si no toda la coincidencia). Sirve para acotar a «el `className` de un `<DialogTitle>`». |
| `unless_span` | Con `within`: salta el tramo si la coincidencia completa de `within` (no solo el grupo 1) cumple esta regex. Sirve para marcas de exención en el mismo elemento. |
| `unless_before` + `before_chars` | Salta la coincidencia si esta regex encaja en los `before_chars` caracteres (80 por defecto) justo antes de ella: por ejemplo un `Object.entries(` que abre la llamada. |

```toml
[check]
kind = "forbid"
within = '''<DialogTitle\b(?:[^>"']|"[^"]*"|'[^']*')*?className\s*=\s*("[^"]*"|\{[^}]*\})'''
pattern = '''(?:^|[\s"'`{(,])text-[^\s"'`{}(),]*'''
except = '''(?:^|[\s"'`{(,])text-(?:base|lg)$'''
```

`stop_at` y `sai-ignore` también valen con `multiline` y `within`.

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

El patrón puede nombrar el propio fichero con `{path}`, `{dir}`, `{parent}`, `{file}`,
`{stem}` y `{ext}` (se escapan como texto literal). Así un fichero compara su
contenido con su ubicación: `name: {parent}` exige que el frontmatter de
`.agents/skills/bun/SKILL.md` diga `name: bun`.

### `max_lines`: tope de líneas por fichero

```toml
[check]
kind = "max_lines"
max = 300
```

Con 412 líneas da un hallazgo en la línea 301 con `snippet = "412 lines (max 300)"`.

### `refs_exist`: referencias que deben resolverse

Cada referencia sale de una coincidencia de `pattern` (grupo 1, o toda la
coincidencia si no hay grupo), opcionalmente limitada a los tramos de `within`,
o de las claves de un objeto JSON (`json_keys = "skills"`, ruta con puntos). Falla
si la ruta resultante no existe tal cual ni con alguno de los `suffixes`. Solo se
evalúa con `check_tree` o `check_text_at(Some(root), ..)`.

```toml
[check]
kind = "refs_exist"
pattern = '`(src/[^`]+)`'
suffixes = [".ts", ".tsx"]
```

`template` convierte la referencia en la ruta a comprobar. Marcadores: `{0}` la
referencia entera, `{1}`.. los grupos, `{nombre}` los grupos con nombre, y las
partes del fichero que se está revisando: `{path}`, `{dir}`, `{parent}` (carpeta
contenedora), `{file}`, `{stem}` (nombre sin extensión) y `{ext}`.

```toml
# skills-lock.json: cada clave de "skills" debe tener su carpeta
files = ["skills-lock.json"]
[check]
kind = "refs_exist"
json_keys = "skills"
template = ".agents/skills/{0}/SKILL.md"
```

Con `glob = true` la referencia es un glob y debe encajar con al menos un fichero
real del repositorio (el recorrido respeta `.gitignore`). Detecta rutas
renombradas o borradas en los `globs:` de una regla:

```toml
files = [".agents/rules/*.md"]
[check]
kind = "refs_exist"
within = '(?m)^globs:\s*\[([^\]]*)\]'   # el tramo entre corchetes
pattern = '"([^"]+)"'                     # cada glob entre comillas
glob = true
```

`pattern` y `json_keys` son excluyentes: hace falta exactamente uno.

### `block`: buscar dentro de bloques balanceados

Cada coincidencia de `open` abre un bloque. Termina en la primera coincidencia de
`end` tras ella o, sin `end`, en el delimitador que cierra el último carácter de la
coincidencia de `open` (`{`, `(` o `[`; ignora cadenas, plantillas y comentarios).
En los bloques donde `when` encaja (todos si falta), el cuerpo debe cumplir
`require` y no debe cumplir `forbid`. El hallazgo cae en el `open` (si falta
`require`) o en la coincidencia de `forbid`.

```toml
[check]
kind = "block"
open = 'function\s+\w*Trigger\s*\([\s\S]*?\)\s*(?::[^{]*)?\{'
when = '\wPrimitive\.\w*Trigger\b'      # solo triggers de overlay
require = 'cursor-pointer'
```

### `unused_symbols`: declarado aquí, usado en otro sitio

Cada símbolo declarado en este fichero (grupo 1 de `declare`, dentro de los tramos
de `within` si se indica) debe usarse en algún fichero de `corpus` (globs). Los
usos se recogen de una pasada con las regex de `uses`; cada una construye el
símbolo con sus grupos (`symbol = "--{1}"`, por defecto `{1}`). Con
`own_file = true` la regex se aplica al propio fichero que declara. El corpus se
lee una sola vez por ejecución.

```toml
[check]
kind = "unused_symbols"
declare = '(?m)^\s{2}(--[\w-]+):'
within = '(?s):root \{(.*?)@theme'
corpus = ["src/**/*.{ts,tsx}"]
uses = [
  { pattern = 'var\(\s*(--[\w-]+)' },
  { pattern = 'var\(\s*(--[\w-]+)', own_file = true },
  { pattern = '\b(?:bg|text|border)-([a-z][\w-]*)', symbol = "--{1}" },
]
```

### `set_covered`: un conjunto debe cubrir a otro

Cada valor capturado por `pattern` en este fichero debe aparecer entre los que
captura `in_pattern` en el primer fichero existente de `in_files`, salvo los de
`ignore`. Cada valor se informa una vez. Si ninguno de `in_files` existe no
informa nada.

```toml
files = ["src/lib/scraper/html-extractor.ts"]
[check]
kind = "set_covered"
pattern = '''platform:\s*["']([a-z0-9_-]+)["']'''
in_files = ["src/components/social-helpers.tsx", "src/components/social-dialog.tsx"]
in_pattern = '''id:\s*["']([a-z0-9_-]+)["']'''
ignore = ["github", "discord"]
```

### `element`: elementos JSX/TSX (ancestros, hijos, props)

Un escáner ligero de JSX (sin AST completo: ignora cadenas, plantillas, comentarios,
genéricos `Foo<Bar>` y comparaciones `a < b`) da a cada elemento su etiqueta,
atributos, hijos y ancestros. `element` selecciona elementos y, según el campo
de aserción que se use, informa del propio elemento (línea de su `<`), de cada
coincidencia dentro de él o de una discrepancia con un pariente. Solo se admite
una aserción: `pattern`, `distinct`, `compare` o `child_values`; sin ninguna, se
informa de cada elemento seleccionado.

Selección (todas las condiciones a la vez):

| Campo | Efecto |
| --- | --- |
| `tag` | Regex sobre el nombre de la etiqueta (entero: `p|span` no encaja con `pre`). |
| `open` / `open_not` | Listas de regex que la etiqueta de apertura debe / no debe cumplir. |
| `content` / `content_except` | Regex sobre el texto interior; `content_except` se aplica a su grupo 1. |
| `ancestor` | `{ tag, open, open_not, nearest, within_chars }`: algún ancestro (o solo el padre con `nearest`) cumple las condiciones; `within_chars` limita a ancestros cuya apertura empieza como mucho a esos bytes del elemento. |
| `class_conditional` | `false` solo elementos cuyo `className` no es condicional (ternario, `&&`, `\|\|`, `??`, o llamada con un argumento así); `true` solo los condicionales. |
| `resolve_consts` | Añade a la apertura el inicializador de cada `const nombre = ...;` al que apunte un atributo `attr={nombre}`, para ver clases guardadas en variables. |

Aserciones:

| Campo | Efecto |
| --- | --- |
| `pattern` (+ `scope`, `except`) | Informa de cada coincidencia. `scope`: `open` (por defecto), `class` (valor del `className`/`class` tal cual), `class_strings` (solo los literales: cadenas, parte de texto de las plantillas, ambas ramas de ternarios y `&&`; informa en la línea del elemento) o `content`. |
| `distinct = { pattern, max }` | Informa si el grupo 1 de `pattern` toma más de `max` valores distintos en la apertura. |
| `child_values = { tag, pattern, default, max }` | Los hijos directos `tag` no pueden tener más de `max` valores distintos del grupo 1 de `pattern` (`default` si falta). |
| `compare` | Compara un valor del elemento con el de un pariente (`to = "parent"` o `"sibling"`, este último con `related_tag`): `own` y `related` son regex con grupo 1 (con `own_default` / `related_default`); `single` salta elementos con varios valores propios; `allow[valor_pariente] = [valores propios aceptados]` y `deny[valor_pariente] = [valores rechazados]`. |

```toml
# icono de un tamaño que el slot del botón ya reserva
[check]
kind = "element"
tag = "HugeiconsIcon"
resolve_consts = true

[check.compare]
to = "parent"
own = '\bsize-([\d.]+)'
single = true
related = '(?s)(?:\A<Button\b|render=\{\s*<Button\b).*?\bsize="([a-z-]+)"'

[check.compare.allow]
icon-sm = ["3.5"]
lg = ["4"]
```

Un elemento dentro de un atributo (`render={<Button />}`) hereda los ancestros del
elemento que lo contiene pero no cuenta como su hijo.

### `contrast`: contraste WCAG de clases de texto y fondo

Calcula la razón de contraste WCAG 2 entre las clases `text-<color>` y
`bg-<color>`/`from-<color>` (con `/NN` de opacidad y `[#hex]`). El color
`<nombre>` se lee de la propiedad CSS `--color-<nombre>` o `--<nombre>` de los
ficheros de `colors` (siguiendo cadenas `var()`; hex, `hsl()`, `rgb()`). Un nombre
sin color (`text-xs`, `bg-transparent`) se ignora. Si solo una de las dos clases
aparece se usa `default_fg`/`default_bg` para la otra. Los umbrales son `min`
(4.5), `min_large` (3) y `min_state` (3, para hover, active e iconos
`[&_svg]:`). Es grande el texto que cumple `large`, o `large_size` junto con
`bold`. Los prefijos `focus:`, `disabled:`, `aria-*`, `data-*`, `peer`, `has`, `in`
(configurables con `skip_prefixes`) y `dark:` no se evalúan.

- `units = "jsx"`: cada elemento con texto directo. Las clases se acumulan desde
  los ancestros (hasta `max_variants` combinaciones de ramas de ternarios) y se
  informa de la peor. `skip_open` salta elementos (o descendientes de) cuya
  apertura encaja (`aria-hidden`, `sr-only`). `sibling_background` aplica una clase
  de fondo cuando un hermano pinta el fondo (un indicador de selección).
- `units = "variants"`: cada entrada del grupo `group` (`variant`) de
  `call(base, { variants: { group: { nombre: "clases" } } })`, mezclada con la
  base. `emphasis` + `min_emphasis` bajan el mínimo para controles con peso.

El hallazgo cae en el elemento (o en la llamada) y su `snippet` lleva la razón,
el mínimo y las dos clases con su color: `<p> 2.54:1 (min 4.5:1) | text text-gray-400
(#9ca3af) on bg bg-white (#ffffff)`.

```toml
[check]
kind = "contrast"
units = "jsx"
colors = ["src/app/globals.css", ".agents/checks/tailwind-palette.css"]
default_fg = "#eaedf0"
default_bg = "#0b0c0f"
large = '^text-(?:[2-9]?xl)$'
```

### Tokens CSS (`css_length`, `css_line_height`, `css_scale`, `css_contrast`, `css_refs`)

Leen las propiedades `--nombre: valor;` del fichero revisado (cualquier bloque, sin
comentarios; la última declaración gana) y evalúan `px`, `rem` (16px), `calc()` con
`+ - * /` y `var()`. Los nombres de `names` van sin `--`. El hallazgo cae en la
línea de la declaración.

| Tipo | Comprueba |
| --- | --- |
| `css_length` | `names` resuelven a ≥ `min_px`. |
| `css_line_height` | Para cada token de `names`, `<token><suffix>` resuelve a una razón ≥ `min_ratio`, salvo tamaños ≥ `skip_from_px`. |
| `css_scale` | Los tokens de `names` (menos `exclude`) son distintos entre sí y están al menos `min_step_px` separados al ordenarlos; los que no se resuelven también se informan. |
| `css_contrast` | `pairs = [{ fg, bg, min, label }]` con tokens de color (alfa mezclado sobre el fondo). |
| `css_refs` | Cada `var(--x)` de los tokens de `names` apunta a un token declarado. |

```toml
[check]
kind = "css_scale"
names = '^radius-[a-z0-9]+$'
min_step_px = 2.0
```

### Guardas de fichero (`only_if` / `unless`)

Campos de la regla (valen para cualquier `check`): `only_if` aplica la regla solo
a ficheros cuyo texto cumple la regex; `unless` la salta si la cumple.

```toml
only_if = '''["']use client["']'''
```

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
| `Invalid(id, motivo)` | id con caracteres no válidos, `description` vacía, `files` vacío o con un glob vacío, sin `check` ni `guide`, patrón o `stop_at` vacío, `max = 0`, id repetido en `Engine::new`, fichero cuyo nombre no coincide con el id, `refs_exist` sin (o con ambos) `pattern`/`json_keys`, `element` con más de una aserción, `contrast` sin `colors` ni colores por defecto válidos |
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

Tests: `cargo test -p saispace-rules`. Casos reales en `tests/scenarios.rs`; las
capacidades genéricas (plantillas, bloques, `element`, `contrast`, tokens CSS) en
`tests/capabilities.rs`, y los escáneres (JSX, expresiones, CSS) con pruebas
unitarias en `src/`.
