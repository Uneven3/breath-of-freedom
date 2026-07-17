# Rationale: revisión de diseño ECS y contratos lógicos (codex)

## Contexto

Esta revisión buscó errores de diseño, malas prácticas ECS y supuestos
lógicos peligrosos en la documentación objetivo. No revisa estado de
implementación; revisa si los contratos documentados son sanos para construir
el juego.

## Decisiones aplicadas

1. **Estado de input por actor, no global.** `ClimbToggle` como `Resource`
   global contradice el contrato multi-actor: dos actores no pueden compartir
   el mismo toggle de escalada. Se reemplaza por `ClimbInputState` por
   actor/controlador. (codex)

2. **Montura es un `Actor` compuesto cuando comparte locomoción.** El horse
   usa el pipeline Movement con sólo sus capacidades compatibles; `Horse`
   conserva la relación y comportamientos propios de Mounts. (codex)

3. **Combate no fuerza estados locomotores concretos.** Un evento como
   "apuntar impide sprint" o "stagger interrumpe" no debe elegir `Walk`/`Idle`
   directamente, porque Movement es el único sistema que conoce suelo, agua,
   caída, escalada y otros facts físicos. Combate emite restricciones
   semánticas; Movement decide un `LocomotionState` válido. (codex)

4. **Cliente multiplayer captura input, no escribe simulación autoritativa.**
   En un modelo host-autoritativo, el cliente produce `LocalInputFrame` y lo
   envía. En el host, Multiplayer entrega ese frame a Input mediante
   `ApplyRemoteActionsMessage`; Input valida fuente/secuencia y muta
   `ActiveActions` del `InputSource` remoto; después los Brains genéricos
   producen `Intents`/`CombatIntents` de actores simulados. Esto evita un
   mundo cliente parcialmente autoritativo y parcialmente replicado. (codex)

5. **Cues discretos no dependen solo de `Changed<T>` en `Update`.** Bevy puede
   ejecutar varios ticks de `FixedUpdate` antes de un `Update`; mirar solo el
   estado final pierde transiciones intermedias. Los sucesos discretos deben
   emitirse desde simulación o conservarse en una cola de transiciones para
   presentación. (codex)

6. **Arbitración sin allocations ni estado vacío ambiguo.** El core de
   `ProposalBuffer` usa capacidad fija y `arbitrate(current)`. No usa `Vec` en
   el hot path ni requiere `unwrap`/panic si no hay propuestas. (codex)

7. **Tests de invariantes antes del checkpoint.** La regla de "tests después
   del checkpoint" aplica al feeling jugable, no a seguridad de arquitectura.
   Invariantes ECS, schedule, multi-actor, overflow y no-alloc deben poder
   testearse desde el diseño. (codex)

8. **Bevy 0.19 usa Messages para comunicación diferida.** Los contratos entre
   plugins no deben documentarse como `EventReader`/`EventWriter`; el patrón
   objetivo es `#[derive(Message)]`, `MessageReader` y `MessageWriter`.
   `Event`/observers se reservan para reacciones inmediatas justificadas.
   Ver `bevy-0-19-messages-vs-events.md`. (codex)

9. **Los pedidos no son resultados.** Si un sistema puede rechazar o validar
   un pedido, el documento debe separar mensajes de request/result
   (`DamageRequestMessage` vs. `DamageAppliedMessage`,
   `InventoryTransactionMessage` vs. resultado aplicado/rechazado). Esto
   evita que otros sistemas reaccionen a efectos que nunca ocurrieron.
   (codex)

10. **Evitar polling de input de hardware directo en FixedUpdate.** Para evitar
    gatillados dobles ante ticks de FixedUpdate múltiples y pérdida de input
    cuando FixedUpdate no corre, Input resuelve hardware en `PreUpdate` (una
    vez por frame de render) escribiendo a `ActiveActions`. Los Brains leen
    acciones resueltas, no `ButtonInput<KeyCode>`, y los gatillos discretos
    se consumen con `InputConsumeCursor` por consumidor/actor, no mutando un
    `ActiveActions` global. Ver `fixed-update-input-gating.md` y
    `data-driven-keybindings.md`. (codex)

11. **UI no escribe tablas de Input.** El menú de rebinding puede leer
    `Keybindings` y emitir `RebindRequestMessage`, pero solo Input valida
    conflictos/capacidad y muta su propio `Keybindings`. Esto mantiene
    ownership ECS y evita que la UI se vuelva dueña accidental de reglas de
    input. (codex)

12. **Multiplayer no escribe `ActiveActions` directamente.** Red recibe wire
    data y emite `ApplyRemoteActionsMessage`; Input valida el `InputSource`
    remoto y muta su propio snapshot. Esto mantiene la frontera de ownership:
    Multiplayer posee transporte/protocolo, Input posee resolución y estado
    de acciones. (codex)

13. **Input remoto requiere secuencia monotónica.** `ActionFrame` incluye
    `frame_seq`; Input descarta frames remotos duplicados o viejos antes de
    tocar `ActiveActions`. Sin esta validación, un paquete reordenado podría
    reactivar gatillos discretos o sobrescribir estados sostenidos con datos
    antiguos. (codex)

12. **Percepción de enemigos ante daño sorpresa.** Para evitar que los enemigos
    atacados desde puntos ciegos no reaccionen, el sistema `Perceive` de
    `Enemies` escucha `DamageAppliedMessage`. Al detectar un impacto dirigido
    a sí mismo, establece al atacante como `AggroTarget` y transiciona
    inmediatamente a `EnemyAiState::Combat`. Ver `enemy-damage-aggro.md`.
    (antigravity)

## Regla resultante

Cuando un documento proponga comunicación entre sistemas, debe decir:

- quién posee el dato;
- quién puede escribirlo;
- si la relación es input, simulación, presentación o red;
- qué sistema valida la transición final;
- cómo se evita asignar memoria en `FixedUpdate`;
- qué pasa ante el caso vacío o inválido.

Si alguno de esos puntos queda implícito, el diseño todavía no está listo para
implementarse. (codex)
