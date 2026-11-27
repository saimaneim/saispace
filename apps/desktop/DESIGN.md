# Sistema de diseño de saispace

Herramienta de escritorio oscura, densa, sobria y rápida para orquestar agentes e información de proyectos (referencias de alta densidad técnica: Linear, Raycast, T3 Code). Sin ornamentos superfluos ni estética de dashboard genérico. Menos controles manuales en UI, permitiendo a la IA configurar todo vía MCP y CLI.

## Identidad visual y paleta de color

La interfaz se estructura sobre capas de luminosidad decreciente en profundidad, eliminando bordes de 1px en favor del contraste directo entre superficies y el espacio intencional. Paleta de grises neutros sin tinte azulado (saturación 0).

### Tokens de superficie
- `void` (`#141414`): Fondo base de la ventana y el área principal de trabajo.
- `carbon` (`#1a1a1a`): Superficie para paneles laterales y contenedores estructurales (barra lateral, feed de actividad, lector de memoria y contexto).
- `obsidian` (`#212121`): Bloques de contenido interactivo, inputs, filas, bloques de código y secciones estructuradas.
- `elevated` (`#2a2a2a`): Estado activo de elementos seleccionados, pestañas abiertas y menús flotantes.
- `graphite` (`#333333`): Superficie de hover o foco reforzado sobre superficies elevadas.
- `smoke` (`#444444`): Delimitación sutil, barras de acento secundarias y separadores en casos estrictamente estructurales.

### Tokens de texto y contraste
- `paper` (`#f5f5f5`): Texto primario de máxima jerarquía, títulos y valores activos.
- `bone` (`#e8e8e8`): Texto principal estándar en filas y contenido activo.
- `mist` (`#d4d4d4`): Texto secundario y cuerpo de párrafos legibles.
- `fog` (`#9a9a9a`): Metadatos, etiquetas secundarias, atajos de teclado e iconos inactivos.
- `ash` (`#6b6b6b`): Separadores visuales, texto deshabilitado e indicadores vacíos.

### Acento de marca y semántica
- `acid-lime` (`#e2f928`): Acento eléctrico único de saispace. Aplica a acción primaria, indicador de selección activa en navegación/listas (`w-0.5 bg-acid-lime`), acento de logotipo y anillos de foco accesible (`ring-2 ring-acid-lime/40`).
- `pulse-green` (`#22c55e`): Estados activos, hallazgos y confirmaciones exitosas.
- `status-amber` (`#f59e0b`): Tareas en progreso, configuración, estados en pausa y advertencias de cambios sin guardar.
- `coral-red` (`#ef4444`): Acciones destructivas, correcciones, errores y estados bloqueados.
- `signal-teal` (`#06b6d4`): Referencias, patrones y bloques `Why`.
- `iris` (`#6366f1`): Arquitectura, metadatos de agentes y bloques `Where`.

### Chips semánticos de memoria
- `decision`: acento `acid-lime` (`bg-acid-lime/12 text-acid-lime`).
- `architecture`: acento `iris` (`bg-iris/15 text-iris`).
- `bugfix`: acento `coral-red` (`bg-coral-red/12 text-coral-red`).
- `pattern`: acento `signal-teal` (`bg-signal-teal/12 text-signal-teal`).
- `config`: acento `status-amber` (`bg-status-amber/12 text-status-amber`).
- `discovery`: acento `pulse-green` (`bg-pulse-green/12 text-pulse-green`).
- `preference`, `session_summary`, `note`: tonos neutros sobre `obsidian` y `elevated` (`text-bone` / `text-mist`).

### Sombras controladas
- `--shadow-popover`: `0 10px 30px -4px #00000080` para menús desplegables y selectores.
- `--shadow-modal`: `0 24px 48px -12px #000000b3` para modales y diálogos de búsqueda.

## Tipografía y densidad

- **Fuente única:** Plus Jakarta Sans Variable cargada localmente mediante `@fontsource-variable/plus-jakarta-sans`.
- **Monoespaciado:** Exclusivamente JetBrains Mono para el editor de contexto, claves temáticas, rutas técnicas y bloques de código.
- **Idioma y mayúsculas:** Mayúsculas de frase en español para todos los textos de la interfaz. Prohibido el uso de `uppercase` o versalitas decorativas.
- **Escala de texto:** Base de 15px. Títulos de detalle en `text-lg` (18px) o `text-base` (16px), contenido en `text-sm` (14px), metadatos y etiquetas en `text-xs` (12px), indicadores numéricos y tiempos relativos con `tabular-nums`.

## Dimensiones, controles y radios

- **Filas estándar:** Altura fija de `h-10` (40px) para proyectos, tareas y tecnologías.
- **Controles de formulario e inputs:** Altura de `h-9` (36px).
- **Acciones compactas e iconos:** Botones de acción secundaria en `h-8` o `h-7`.
- **Iconografía:** Lucide React unificada a `w-4 h-4` (o `w-3.5 h-3.5` en árboles anidados y chips).
- **Radios consistentes:**
  - `rounded-md` (6px) para botones, inputs y filas interactivas.
  - `rounded-lg` (8px) para popovers, menús, bloques estructurados y grupos de tabs.
  - `rounded-xl` (12px) para diálogos modales y contenedores de lectura principal.
- **Transiciones y estabilidad:** Respuesta inmediata `≤ 150ms` (`120ms` estándar para hover, foco, popovers y modales; `150ms` máximo para el ancho de la barra lateral). Respeto estricto de `prefers-reduced-motion`, `overscroll-contain` en contenedores modales y reserva de alto en cargas iniciales para evitar saltos de layout.

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
- Bloques estructurados (`What:`, `Why:`, `Where:`, `Learned:` con o sin negrita Markdown, y `**Clave**:`) agrupados con su contenido en secciones sobre `obsidian/60` con barra lateral sutil de rol, etiqueta legible en `fog` (sin `uppercase`) y texto en `bone`/`mist`.
- Ancho de lectura controlado (~70 caracteres en prosa, contenedores `max-w-[76ch]`).

## Lista de prohibiciones

- Prohibido el uso de bordes de 1px (`border`, `border-b`, etc.) para división de bloques.
- Prohibidos degradados, brillos, neones, glassmorphism o `backdrop-blur`.
- Prohibidas sombras pesadas con valores RGBA directos en clases (`shadow-[...rgba(...)]`).
- Prohibidas animaciones lentas (`> 150ms`) o elásticas y `transition-all`.
- Prohibidos textos de relleno decorativos y tarjetas infladas con espacio muerto.
