# Rationale: Messages sobre Events para contratos entre plugins en Bevy 0.19 (codex)

## Contexto

Bevy 0.19 separa con más claridad dos mecanismos que antes se confundían en
la documentación: `Message` para comunicación diferida y `Event`/observers
para reacciones inmediatas. La arquitectura de este proyecto depende de
schedule explícito, host autoritativo y sistemas desacoplados; por eso el
contrato por defecto entre plugins debe ser `Message`. (codex)

## Decisión

Los documentos de arquitectura usan `#[derive(Message)]`,
`MessageReader<T>` y `MessageWriter<T>` para daño, cues de presentación,
spawn de proyectiles, restricciones locomotoras, inventario, crafting,
quests y persistencia. La categoría del mapa es `MESSAGE`, no `EVENT`.
(codex)

`Event`/observers quedan reservados para casos donde la reacción inmediata
sea parte explícita del diseño. Si un documento quiere usar observers, debe
justificar por qué la ejecución inmediata es necesaria y cómo no rompe el
orden determinista de `FixedUpdate`. (codex)

## Consecuencias

- Un sistema que pide algo a otro emite un mensaje semántico; el dueño valida
  y muta su propio dato.
- Los mensajes de pedido y resultado se separan cuando importa la validación:
  `DamageRequestMessage` no equivale a `DamageAppliedMessage`.
- UI/SFX/VFX pueden consumir mensajes discretos en `Update`, pero los valores
  continuos se leen read-only desde componentes/resources de simulación.
- Los nombres heredados con sufijo `Event` no son el patrón objetivo para
  nuevos contratos del proyecto. (codex)
