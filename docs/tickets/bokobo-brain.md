# Ticket: `bokobo-brain`

## Sistema(s)

Enemies (nuevo plugin `src/enemies/`). Primer slice: un enemigo de graybox
("bokobo") cuyo brain de IA mueve el cuerpo **solo escribiendo `Intents`**,
por el pipeline normal de Movement. Sin combate, sin salud, sin facciones —
esos llegan con sus propios sistemas (phase gate del proyecto: movimiento
perfecto + IA moviendo por Intents, *antes* que combate).

## Lectura obligatoria, en este orden

1. `docs/CONSTITUTION.md` — completo.
2. `docs/ARCHITECTURE-MAP.md` — fila `Enemies`.
3. `docs/COUPLING-MAP.md` — Enemies↔Movement (`SHARED-CONTRACT`).
4. `docs/architecture/enemies.md`.
5. `docs/architecture/movement.md` (contrato `Intents`, multi-actor).
6. `docs/architecture/rationale/multi-actor-dispatch.md`.
7. `docs/architecture/rationale/per-entity-state-idioms.md` (por qué
   `EnemyAiState` es un enum-componente y no markers).

## Acoplamiento

- **Enemies↔Movement: SHARED-CONTRACT.** El brain escribe el `Intents`
  existente sin cambiarle una línea; el prerequisito multi-actor
  (`multi-actor-migration` + dispatcher exhaustivo) ya está en `main`.
- **Enemies↔Combat: Tight, y Combat no existe** → todo lo de combate queda
  explícitamente fuera (ver Fuera de alcance).
- **Enemies↔World: READ** — este slice ni siquiera lee `TimeOfDay`
  (no existe aún); usa `world::GameLayer` para la línea de visión.

## Alcance (File Touches)

- `src/enemies/mod.rs`, `src/enemies/state.rs`, `src/enemies/perception.rs`,
  `src/enemies/brain.rs`
- `src/main.rs` (registrar `EnemiesPlugin`)
- `src/visuals.rs` (cápsula visual del bokobo, patrón del probe)
- `docs/architecture/enemies.md` (propuesta → implementado para este slice)
- `docs/WORKING-CONTEXT.md`
- `docs/tickets/bokobo-brain.md` (este archivo)

## Fuera de alcance

- No agrega Combat, `CombatIntents`, Health, daño, aggro por daño recibido
  (`DamageAppliedMessage`), `Faction`, ni `EnemyAiState::{Combat, Flee}` —
  esos variantes entran cuando existan Combat/Health.
- No agrega pathfinding/navegación: el bokobo camina en línea recta hacia su
  objetivo; si el graybox lo bloquea, se queda empujando (aceptable para el
  primer checkpoint).
- No toca motores, servicios, hechos, arbitraje ni el formato de `Intents`.
- El brain **nunca** escribe `Transform`, `BodyVelocity` ni
  `LocomotionState` (invariante del contrato multi-actor).
- No integra `SensingLod` con la percepción (1 ray por enemigo por tick es
  barato; se integra cuando haya campamentos).

## Diseño del slice

Pipeline análogo al del jugador, tres sistemas encadenados dentro de
`MovementSet::ReadIntents` (mismo slot conceptual que `brain::read_intents`
y `probe::drive_intents`):

1. **`perception::perceive`** — para cada `Enemy`, evalúa cada `Player`
   actor: distancia ≤ `Perception::sight_range`, dentro del cono
   `fov_deg`, y ray de línea de visión enmascarado a `GameLayer::Default`
   (solo la geometría del mundo ocluye; los actores son invisibles al ray
   por capa). Escribe `AggroTarget { target, last_seen }`.
2. **`brain::decide`** — máquina de estados pura sobre `EnemyAiState`:
   ve target → `Alert`; lo pierde → `Search` (hacia `last_seen`); llega o
   expira el timeout → `Patrol`.
3. **`brain::act`** — traduce `EnemyAiState` a `Intents`:
   - `Patrol`: waypoints pseudo-aleatorios determinísticos (secuencia de
     ángulo áureo por entidad) alrededor de `Home`, con pausa entre puntos.
   - `Alert`: sprint hacia el target, frenando a `engage_distance`.
   - `Search`: camina hasta `last_seen`.

Tuning en presets const (`Perception::BOKOBO`, `EnemyBrainProfile::BOKOBO`),
mismo patrón que `GroundMovement::PLAYER`. **F7** spawnea/despawnea el bokobo
en su posición authored de mundo (nunca relativa al jugador — misma decisión
que el probe).

## Definición de terminado

- [x] `cargo fmt` limpio.
- [x] `cargo clippy` sin warnings nuevos.
- [x] `cargo check`/`cargo test` pasa.
- [x] El comportamiento coincide con `docs/architecture/enemies.md`
      (actualizado en este ticket).
- [x] Invariantes §11 testeadas: no-bleed de `AggroTarget`/`EnemyAiState`
      entre enemigos, transiciones de `decide` como función pura, `act` no
      pisa los `Intents` del jugador, waypoints determinísticos.
- [ ] Checkpoint de *feeling* (play test del usuario): patrulla legible,
      persecución que asusta un poco, búsqueda que se rinde con gracia.
- [x] Sin `unsafe`; sin `unwrap()`/`expect()` fuera de tests.
- [x] Relación Enemies↔Movement ya estaba en ambos mapas
      (`SHARED-CONTRACT`); este ticket no agrega relaciones nuevas.

## Notas para el agente que lo toma

- El usuario llama "bokobo" a su bokoblin-like; el marker genérico es
  `Enemy` (enemies.md), "bokobo" es el preset de tuning.
- `EnemyAiState` es upstream de `LocomotionState` — nunca lo espejes ni lo
  escribas desde Movement.
