# Instrucciones del proyecto saispace

## Idioma

Responde **siempre en español**, incluidos los resúmenes de trabajo, las explicaciones y las preguntas al usuario. El código, los identificadores, los mensajes de commit y los nombres de archivo van en inglés; todo lo que sea conversación va en español.

## Flujo de trabajo

- No hagas `git commit` ni `git push` salvo que se pida explícitamente.
- Antes de dar algo por terminado, ejecuta lo que corresponda: `cargo test --workspace` y, en `apps/desktop`, `bun run check` y `bun run lint`.
- Antes de dar un cambio por terminado, ejecuta `sai bench --compare` y pon los deltas en el resumen. Si una métrica empeora, explícalo o arréglalo. Al preparar el PR, registra la línea con `sai bench --record`.
- No toques zonas del repo que no sean del encargo.
- Si algo no se puede verificar, dilo en vez de suponerlo.
