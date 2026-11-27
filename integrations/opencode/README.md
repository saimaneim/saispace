# Plugin de saispace para OpenCode

Para OpenCode 2.4 o posterior (API de plugins v2: export por defecto con `id` y `setup`).

En la primera ejecución de cada sesión principal guarda la salida de `sai memory context` como instrucción de sesión (`saispace-memory`). El agente ve el índice de memorias del proyecto sin que se toque el prompt base de OpenCode.

- Tras compactar, durante una ejecución, muestra la variante `--source compact`, que pide guardar un `session_summary`. Después vuelve al índice normal.
- Ignora las sesiones hijas (subagentes).
- `sai` corre con stdin cerrado y se mata a los 3 s. El índice se limita a 8 KB. Ante cualquier fallo no inyecta nada y lo reintenta en la siguiente ejecución.

## Aprendizaje automático de errores

Con cada mensaje del usuario (evento `session.inbox.enqueued`), el plugin envía el texto como JSON a `sai memory nudge` por stdin:

```json
{"hook_event_name":"UserPromptSubmit","prompt":"<texto del usuario>"}
```

Si el CLI detecta que el prompt es una corrección, devuelve un recordatorio corto (≤ 300 caracteres). El plugin lo guarda como entrada de instrucciones `saispace-nudge` y la quita al terminar ese turno. Si no hay corrección, no se inyecta nada. Errores y timeouts (1.5 s) se ignoran silenciosamente.

Cada llamada a herramienta que termina (`session.tool.success` / `session.tool.failed`) se envía a `sai memory nudge` como `PostToolUse` / `PostToolUseFailure` (nombre y entrada de la herramienta, salida y error). Si su texto coincide con los disparadores de una lección guardada (`sai memory save --trigger "…"`), el plugin la guarda como entrada `saispace-recall` hasta que termina la ejecución. `sai` corre con `SAISPACE_HOST=opencode`, así que las lecciones con `env:opencode` salen en el índice al empezar.

## Instalación

Necesitas `sai` en el `PATH`:

```sh
sai setup opencode
```

Copia el plugin incluido en `sai` a `~/.config/opencode/plugins/saispace.ts` y su módulo a `~/.config/opencode/saispace-plugin/`. Repite el comando tras actualizar `sai`; `--uninstall` lo quita. Si había el enlace antiguo al repo, lo sustituye.

Reinicia OpenCode (o su servicio en segundo plano) para que cargue el plugin.
