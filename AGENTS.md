# Instrucciones del proyecto saispace

- Conversación en español; código, identificadores, commits y nombres de archivo en inglés.
- No hagas `git commit` ni `git push` salvo que se pida.
- No toques zonas del repo ajenas al encargo. Si algo no se puede verificar, dilo.
- La verificación es automática: `.githooks/pre-push` (activa con `git config core.hooksPath .githooks`) y el CI ejecutan fmt, clippy, tests, `sai check` y `sai bench --compare`. Al preparar un PR, registra la línea con `sai bench --record` y pon los deltas en el resumen.
