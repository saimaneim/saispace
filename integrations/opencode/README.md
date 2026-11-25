# Plugin de saispace para OpenCode

Para OpenCode 2.4 o posterior (API de plugins v2: export por defecto con `id` y `setup`).

En la primera ejecución de cada sesión principal guarda la salida de `sai memory context` como instrucción de sesión (`saispace-memory`). El agente ve el índice de memorias del proyecto sin que se toque el prompt base de OpenCode.

- Tras compactar, durante una ejecución, muestra la variante `--source compact`, que pide guardar un `session_summary`. Después vuelve al índice normal.
- Ignora las sesiones hijas (subagentes).
- `sai` corre con stdin cerrado y se mata a los 3 s. El índice se limita a 8 KB. Ante cualquier fallo no inyecta nada y lo reintenta en la siguiente ejecución.

## Instalación

Necesitas `sai` en el `PATH`. Desde la raíz del repositorio:

```sh
ln -sf "$PWD/integrations/opencode/saispace.ts" ~/.config/opencode/plugins/saispace.ts
```

Reinicia OpenCode (o su servicio en segundo plano) para que cargue el plugin.
