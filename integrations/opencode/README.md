# Plugin de saispace para OpenCode

Inyecta automáticamente el índice de memorias de `saispace` (`sai memory context`) en el prompt del sistema de OpenCode al iniciar cada sesión raíz y lo refresca tras una compactación (`--source compact`, con respaldo a `sai memory context`).

- Caché por `sessionID` para no ejecutar `sai` en cada turno.
- Ignora sesiones hijas o de subagentes (`parentID`).
- Timeout de 3 s y límite de 8 KB; ante cualquier fallo no interrumpe OpenCode.

## Instalación

Asegúrate de tener `sai` en el `PATH` y enlaza el plugin desde la raíz del repositorio:

```sh
ln -sf "$PWD/integrations/opencode/saispace.ts" ~/.config/opencode/plugins/saispace.ts
```
