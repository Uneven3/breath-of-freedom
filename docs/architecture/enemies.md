# Enemies

**Carpeta objetivo:** `src/enemies/`

**Estado:** primer slice implementado (ticket `bokobo-brain`): un enemigo de
graybox ("bokobo", spawn/despawn con **F7**) que patrulla, persigue al verte
y busca donde te vio por última vez — moviéndose exclusivamente por
`Intents`. Combate, salud, facciones y aggro-por-daño siguen siendo
propuesta.

## Datos (Components/Messages/Resources)

| Tipo | Dónde | Qué es | Estado |
|---|---|---|---|
| `Enemy` | `enemies/mod.rs` | Marker, análogo a `Player` — junto con `Actor` (ver `rationale/multi-actor-dispatch.md`) identifica un cuerpo controlado por IA. | ✅ |
| `Home` | `enemies/mod.rs` | Punto de mundo alrededor del cual patrulla y al que vuelve. | ✅ |
| `EnemyAiState` | `enemies/state.rs` | SSoT propio de la decisión de IA: `Patrol`, `Alert`, `Search` (enum-componente, ver `rationale/per-entity-state-idioms.md`; solo `brain::decide` lo escribe). No es `LocomotionState` ni `CombatState` — es upstream de ambos. `Combat` y `Flee` entran cuando existan Combat/Health. | ✅ parcial |
| `Perception` | `enemies/perception.rs` | Tuning de sentidos por enemigo: visión (`sight_range`, `fov_deg`, `detection_secs`, `close_range_boost`, `sneak_visibility`), oído (`hearing_range`, `wall_muffle`) y decay del medidor. Preset `Perception::BOKOBO` (patrón `GroundMovement::PLAYER`). Modelo completo en `rationale/enemy-senses.md`. | ✅ |
| `DirectThreatMessage` | `enemies/perception.rs` | Amenaza inequívoca dirigida a un enemigo (recibir daño): salta el medidor a `ALERTED` y marca `last_seen`. Propiedad de Enemies; lo emitirán Health/Combat (patrón mensaje-del-receptor, como `health::DamageRequestMessage`). | ✅ mecanismo (emisores pendientes) |
| `Awareness` | `enemies/perception.rs` | **El medidor de alerta** (`0.0..=1.0`), escrito solo por `perceive`. Umbrales semánticos: `SUSPICIOUS` (investiga) y `ALERTED` (full threat). Es el contrato que Combate leerá para las reglas de sigilo: enemigo no alertado → flechas y ataques sigilosos con bonus de daño; alertado → sin sneakstrike. | ✅ |
| `AggroTarget` | `enemies/perception.rs` | Salida de Perceive: `in_sight` (el target actualmente en vista, a cualquier nivel de alerta) y `last_seen` (sobrevive a perder la vista; lo limpia el brain al rendirse). | ✅ |
| `EnemyBrainProfile` | `enemies/brain.rs` | Tuning de comportamiento: distancias de engage/arribo, radio y pausa de patrulla, timeout de búsqueda. Preset `BOKOBO`. | ✅ |
| `BrainLocal` | `enemies/brain.rs` | Bookkeeping por-enemigo del brain (waypoint, timers). Componente, nunca `Local` de sistema (contrato multi-actor). | ✅ |
| `Faction` | `enemies/mod.rs` | Para reacciones grupales (GDD §7). | propuesta |

Enemies **reutiliza** `Intents`/`LocomotionState` de Movement sin cambiarles
una línea (y hará lo mismo con `CombatIntents`/`CombatState` cuando Combate
exista) — ver decisión abajo.

## Sistemas (comportamiento)

Pipeline análogo al de un jugador, tres sistemas encadenados dentro de
`MovementSet::ReadIntents` (el mismo slot conceptual que
`movement::brain::read_intents`):

1. **`perception::perceive`** ✅ — escribe `AggroTarget` y `Awareness`
   evaluando cada `Player` actor contra cono de visión (distancia + FOV
   planar) y línea de visión: un ray enmascarado a
   `world::GameLayer::Default`, así solo la geometría del mundo ocluye (las
   cápsulas de actores son invisibles al ray por capa). La detección es
   **gradual**: en vista, el medidor llena a `1/detection_secs` modulado por
   cercanía (`close_range_boost`) y por el `LocomotionState` del target
   (`Sneak` multiplica por `sneak_visibility` — para eso existe el sigilo);
   sin vista, decae en `awareness_decay_secs`. "Por la espalda" no hace
   falta como regla: fuera del cono el medidor nunca llena. **Oído** ✅: sin
   visión, un radio omnidireccional (`hearing_range × loudness` del target,
   derivada read-only de su gait y velocidad; paredes atenúan con
   `wall_muffle`) llena el mismo medidor **con techo en `SUSPICIOUS`** — un
   ruido te hace girar e investigar, solo la vista completa la detección.
   **Aggro por daño** ✅ mecanismo: `receive_direct_threats` (entre perceive
   y decide) procesa `DirectThreatMessage` → `ALERTED` instantáneo. Ver
   `rationale/enemy-senses.md`. Propuesto: ruidos discretos posicionales,
   `TimeOfDay` (GDD §10), alerta grupal por `Faction`.
2. **`brain::decide`** ✅ — único escritor de `EnemyAiState`. Transiciones
   (función pura `next_ai_state`, testeada): en vista **y** `ALERTED` →
   `Alert` (full threat); `SUSPICIOUS` → `Search` para investigar el
   estímulo; perder al target alertado → `Search` hacia `last_seen`; llegar
   ya calmado o expirar `search_timeout_secs` → `Patrol` (olvida el target).
3. **`brain::act`** ✅ — traduce `EnemyAiState` a `Intents` (overwrite
   completo por tick, como todo brain): `Patrol` deambula por waypoints
   pseudo-aleatorios determinísticos (secuencia de ángulo áureo por entidad —
   orgánico pero reproducible y testeable) alrededor de `Home` con pausas;
   `Alert` sprinta hacia el target y frena en `engage_distance` (ahí lo
   tomará Combate); `Search` camina hasta `last_seen`.

El brain nunca escribe `Transform`, `BodyVelocity` ni `LocomotionState`, y
no hay pathfinding todavía: el bokobo va en línea recta y puede quedar
empujando un obstáculo (aceptable para el checkpoint de graybox).

## Relaciones con otros sistemas

| Relación | Categoría | Mecanismo | Estado |
|---|---|---|---|
| Enemies escribe `Intents` de Movement | SHARED-CONTRACT | Mismo tipo de dato, Brain distinto — cero cambios en Movement | ✅ |
| Enemies escribe `CombatIntents` de Combate | SHARED-CONTRACT | Igual que arriba | propuesta (Combat no existe) |
| Combate lee `enemies::Awareness` del objetivo para las reglas de sigilo | READ (contrato fijado) | Query read-only sobre `Awareness::is_alerted()`: no alertado → bonus de flechas/sneakstrike; alertado → full threat, sin sneakstrike. Ver `combat.md` § Relaciones. | contrato ✅, Combat no existe |
| Enemies usa `world::GameLayer` para línea de visión | READ | `SpatialQueryFilter::from_mask(Default)` | ✅ |
| Enemies lee `World::TimeOfDay` | READ | Query read-only | propuesta |
| Enemies lee `Movement::LocomotionState` del objetivo (¿está en sigilo?) | READ | Query read-only en `perceive` (modula la velocidad de detección); nunca asume un único jugador | ✅ |
| Enemies lee su propio `Health` para `Flee` (GDD §7) | READ | Query read-only — ver `health.md` | propuesta |
| Health/Combat emiten `enemies::DirectThreatMessage` al dañar a un enemigo | MESSAGE (Enemies es dueño del tipo) | Aggro inmediato ante ataques sorpresa: `ALERTED` + `last_seen`, saltando el medidor. Reemplaza la propuesta anterior de escuchar `DamageAppliedMessage` directamente — el receptor es dueño del contrato. | mecanismo ✅, emisores pendientes |
| Combate: flanqueo grupal (GDD §7) | decisión abierta | — | — |

## Decisiones abiertas

- Diseño de `EnemyAiState::{Combat, Flee}` cuando existan Combat/Health.
- Reacciones grupales por `Faction`.
- Pathfinding/navegación (hoy: línea recta).
- Integrar la percepción con `SensingLod` cuando haya campamentos (hoy es
  1 ray por enemigo por tick — barato).
- Enemies montando criaturas — hereda la dependencia de Monturas si se
  construye.
