# Enemies

**Carpeta objetivo:** `src/enemies/`

## Datos (Components/Messages/Resources) — propuesta

| Tipo | Dónde | Qué es |
|---|---|---|
| `Enemy` | `enemies/mod.rs` | Marker, análogo a `Player` — junto con `Actor` (ver `rationale/multi-actor-dispatch.md`) identifica un cuerpo controlado por IA. |
| `EnemyAiState` | `enemies/state.rs` | SSoT propio de la decisión de IA: `Patrol`, `Alert`, `Search`, `Combat`, `Flee`. No es `LocomotionState` ni `CombatState` — es upstream de ambos, el análogo de "hardware" para un actor de IA. |
| `Perception` | `enemies/perception.rs` | Rango de visión/oído, `AggroTarget(Option<Entity>)`. |
| `Faction` | `enemies/mod.rs` | Para reacciones grupales (GDD §7). |

Enemies **reutiliza** `Intents`/`CombatIntents`/`LocomotionState`/
`CombatState` de Movement/Combate sin cambiarles una línea — ver decisión
abajo.

## Sistemas (comportamiento) — propuesta

Pipeline análogo al de un jugador, pero el "Brain" es IA:

1. **Perceive** — llena `Perception` leyendo transforms/`World` (line of sight), `TimeOfDay` (GDD §10: comportamiento día/noche), y escuchando `DamageAppliedMessage` dirigidos a sí mismo para detectar ataques de fuentes no visibles y transferir el aggro inmediatamente al atacante.
2. **Decide** — máquina de estados que escribe `EnemyAiState` a partir de `Perception` + `Faction`.
3. **Act** (el `EnemyBrain`) — traduce `EnemyAiState` + posición del target a `Intents`/`CombatIntents`, en el mismo slot conceptual que `brain::read_intents` ocupa para el jugador (ver `movement.md`).

## Relaciones con otros sistemas

| Relación | Categoría | Mecanismo |
|---|---|---|
| Enemies escribe `Intents`/`CombatIntents` de Movement/Combate | SHARED-CONTRACT | Mismo tipo de dato, Brain distinto — cero cambios en Movement/Combate |
| Enemies lee `World::TimeOfDay` | READ | Query read-only |
| Enemies lee `Movement::LocomotionState` del `AggroTarget`/objetivo percibido (¿está en sigilo?) | READ | Query read-only, mismo mecanismo que el bonus de sigilo en `combat.md`; nunca asume un único jugador |
| Enemies lee su propio `Health` para decidir `EnemyAiState::Flee` al estar herido (GDD §7) | READ | Query read-only — ver `health.md` |
| Enemies escucha `DamageAppliedMessage` dirigido a sí mismo | MESSAGE | Para reaccionar con aggro inmediato ante ataques sorpresa (fuera de línea de visión) |
| Combate: flanqueo grupal (GDD §7) | decisión abierta | — |

## Decisiones abiertas

- **Prerequisito:** Movement y Combate operan sobre `Query<.., With<Actor>>`
  para que `EnemyBrain` escriba `Intents`/`CombatIntents` en cualquier actor
  controlado por IA. Ver `rationale/multi-actor-dispatch.md`.
- Diseño concreto de la máquina de `EnemyAiState` (transiciones exactas).
- Reacciones grupales por `Faction`.
- Enemies montando criaturas — hereda la dependencia de Monturas si se
  construye.
