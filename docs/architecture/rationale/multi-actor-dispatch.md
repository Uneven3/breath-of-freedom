# Rationale: dispatch multi-actor (`Actor` genérico)

Monturas, Enemies y Multiplayer necesitan N actores corriendo el mismo
pipeline (Brain → Intents → Broker → Motors → Body) simultáneamente e
independientemente. El contrato objetivo es un marker genérico `Actor`:
`Player`, `Enemy`, jugadores remotos y otros cuerpos controlables son
especializaciones de ese actor.

## Decisión

El pipeline de simulación no debe depender de `Single<..., With<Player>>`.
Los sistemas de Movement y Combat operan sobre `Query<..., With<Actor>>`,
iterando todos los actores relevantes.

La fase `TickActiveMotor` encadena sistemas `tick_body` con queries exactas
por capacidad. Todos comparten un `MotorCore` pequeño; cada motor agrega solo
su perfil, facts, pool y estado privado. Cada sistema comprueba su
`LocomotionState` dueño antes de mover el cuerpo. El encadenamiento fija un
único writer temporal y la SSoT de estado hace que un actor solo pueda ser
movido por uno de ellos.

Todo estado temporal que pueda variar por actor vive en componentes por
entidad, no en `Local<T>` compartido por el sistema (`JumpLocal`,
`GlideLocal`, `SprintLock`, `StairsLocal`, …).

## Por qué queries exactas encadenadas

La unión anterior forzaba a cada actor a participar en una query con alrededor
de veinte `Option`, aunque careciera de esas capacidades. Las queries exactas
conservan la composición ECS: un horse no carga ni expone datos de climb,
mantle, ladder, glide, sneak o wall-jump. El costo es una pasada acotada por
capacidad, compensada por filtros arquetípicos que excluyen actores no
compatibles. El `.chain()` hace explícito el orden de writers.

Los `propose` siguen siendo 13 sistemas independientes a propósito: ahí la
gracia del Broker es que cada motor decida por sí mismo, y el orden de
ejecución no importa porque el arbitraje resuelve por `(Priority, weight)`
(ver los tests `*_regardless_of_propose_order`).

## Por qué

- **Enemies:** `EnemyBrain` necesita escribir `Intents`/`CombatIntents` en
  entidades que no son el jugador local.
- **Multiplayer:** un actor remoto es un `Actor` cuyo `InputSource` viene de
  red; los mismos Brains genéricos traducen `ActiveActions` a
  `Intents`/`CombatIntents`.
- **Monturas:** el pipeline de Mounts es separado, pero jinetes no-jugador
  heredan el mismo modelo de actor genérico.

Ver `docs/architecture/movement.md` y `docs/ARCHITECTURE-MAP.md`
(categoría `BLOCKING-PREREQUISITE`).
