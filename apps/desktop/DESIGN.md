# Sistema de diseño de saispace

Herramienta de escritorio oscura, densa, sobria y rápida para orquestar agentes e información de proyectos (referencias de alta densidad técnica: Linear, Raycast, T3 Code). Sin ornamentos superfluos ni estética de dashboard genérico. Menos controles manuales en UI, permitiendo a la IA configurar todo vía MCP y CLI.

## Identidad visual y paleta de color

La interfaz se estructura sobre capas de luminosidad decreciente en profundidad, eliminando bordes de 1px en favor del contraste directo entre superficies y el espacio intencional. Paleta de grises neutros sin tinte azulado (saturación 0).

### Tokens de superficie
- `void` (`#141414`): Fondo base de la ventana y el área principal de trabajo.
- `carbon` (`#1a1a1a`): Superficie para paneles laterales y contenedores estructurales (barra lateral, feed de actividad).
- `obsidian` (`#212121`): Bloques de contenido interactivo, inputs, filas y cabeceras de sección.
- `elevated` (`#2a2a2a`): Estado activo de elementos seleccionados, pestañas abiertas y menús flotantes.
- `graphite` (`#333333`): Superficie de hover o foco reforzado.
- `smoke` (`#444444`): Delimitación sutil y separadores en casos estrictamente estructurales.

### Tokens de texto y contraste
- `paper` (`#f5f5f5`): Texto primario de máxima jerarquía, títulos y valores activos.
- `bone` (`#e8e8e8`): Texto principal estándar.
- `mist` (`#d4d4d4`): Texto secundario y cuerpo de párrafos legibles.
- `fog` (`#9a9a9a`): Metadatos, etiquetas secundarias, atajos de teclado e iconos inactivos.
- `ash` (`#6b6b6b`): Separadores visuales, texto deshabilitado e indicadores vacíos.

### Acento de marca y semántica
- `acid-lime` (`#e2f928`): Acento eléctrico único de saispace. Aplica a acción primaria, selección destacada, acento de logotipo y anillos de foco accesible (`ring-2 ring-acid-lime/40`).
- `pulse-green` (`#22c55e`): Estados activos y confirmaciones exitosas.
- `status-amber` (`#f59e0b`): Tareas en progreso, estados en pausa y advertencias de cambios sin guardar.
- `coral-red` (`#ef4444`): Acciones destructivas, errores y estados bloqueados.
- `signal-teal` (`#06b6d4`): Referencias y tecnologías secundarias.
- `iris` (`#6366f1`): Metadatos especiales de agentes.

### Sombras controladas
- `--shadow-popover`: `0 10px 30px -4px #00000080` para menús desplegables y selectores.
- `--shadow-modal`: `0 24px 48px -12px #000000b3` para modales y diálogos de búsqueda.

## Tipografía y densidad

- **Fuente única:** Plus Jakarta Sans Variable cargada localmente mediante `@fontsource-variable/plus-jakarta-sans`.
- **Monoespaciado:** Exclusivamente JetBrains Mono para el editor de contexto y bloques de código de previsualización.
- **Idioma y mayúsculas:** Mayúsculas de frase en español para todos los textos de la interfaz. Prohibido el uso de `uppercase` o versalitas decorativas.
- **Escala de texto:** Base de 15px. Contenido en `text-sm` (14px), metadatos y etiquetas en `text-xs` (12px), indicadores numéricos con `tabular-nums`.

## Dimensiones, controles y radios

- **Filas estándar:** Altura fija de `h-10` (40px) para proyectos, tareas y tecnologías.
- **Controles de formulario e inputs:** Altura de `h-9` (36px).
- **Acciones compactas e iconos:** Botones de acción secundaria en `h-8` o `h-7`.
- **Iconografía:** Lucide React unificada a `w-4 h-4` (o `w-3.5 h-3.5` en árboles anidados).
- **Radios consistentes:**
  - `rounded-md` (6px) para botones, inputs y filas interactivas.
  - `rounded-lg` (8px) para popovers, menús y grupos de tabs.
  - `rounded-xl` (12px) para diálogos modales.
- **Transiciones:** Respuesta inmediata de `120ms` (`duration-[120ms]`) para hover y foco.

## Jerarquía de la pestaña Contexto

Solo ficheros estándar del repo: saispace no guarda contexto propio fuera de ellos.

1. **Directrices:** Ficheros en raíz (`AGENTS.md`, `CLAUDE.md`, `DESIGN.md`), desplegado por defecto. Se abre `AGENTS.md` al entrar.
2. **Skills:** Carpeta de skills (`.agents/skills/*`, `.claude/skills/*`), colapsada por defecto. El contador cuenta skills (carpetas), no ficheros. Cada skill es un subgrupo con sus ficheros (`SKILL.md`, referencias).
3. **Reglas:** Reglas operativas (`.agents/rules/*`, `.cursor/rules/*`), colapsada por defecto.
4. **Otros:** Archivos de contexto restantes, colapsada por defecto.
5. **Filtro reactivo:** Campo de búsqueda superior que expande de inmediato los grupos y skills que coincidan.
6. **Rutas e información:** Nombres limpios con tooltip nativo que muestra la ruta completa en hover.
7. **Vista previa por defecto:** Markdown (`.md`, `.mdc`) se abre renderizado; JSON, TOML y YAML se abren en el editor.

## Renderizado de memoria y Markdown

- Presentación limpia sin marcas crudas ni `dangerouslySetInnerHTML`.
- Bloques de clave/valor (`**What**:`, `**Why**:`, `**Where**:`, `**Learned**:`) presentados con etiquetas en `fog` y texto legible en `mist`.
- Ancho de lectura controlado a ~70 caracteres (`max-w-[70ch]`).

## Lista de prohibiciones

- Prohibido el uso de bordes de 1px (`border`, `border-b`, etc.) para división de bloques.
- Prohibidos degradados, brillos, neones, glassmorphism o `backdrop-blur`.
- Prohibidas sombras pesadas con valores RGBA directos en clases (`shadow-[...rgba(...)]`).
- Prohibidas animaciones lentas o elásticas.
- Prohibidas tarjetas infladas con espacio muerto.
