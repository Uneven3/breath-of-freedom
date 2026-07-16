# Health

**Carpeta objetivo:** `src/health/`

**Estado:** implementado (ticket `health-core`, 2026-07-16); pendiente
checkpoint jugado.

Sistema hermano de Movement/Combat/Mounts — cualquier actor con vida
(jugador, Enemies, y potencialmente Monturas) usa el mismo `Health`, sin que
Combate necesite poseer el dato. Ver `rationale/health-ownership-boundary.md`.

## Datos (Components/Messages/Resources)

| Tipo | Dónde | Qué es |
|---|---|---|
| `Health` | `health/data.rs` | `{ current: f32, max: f32 }`, campos privados. Solo sus propios métodos (`apply_damage`, `heal_full`) la mutan — mismo patrón que `movement::Stamina`. |
| `DamageRequestMessage` | `health/data.rs` | `{ target: Entity, amount: f32 }`. Cualquier sistema (Combat, Projectiles, futuros hazards) lo emite; Health valida que el target tenga `Health` vivo y aplica el daño. (`source` para kill credit y `DamageKind` se agregan cuando un sistema los lea — ningún campo antes de su consumidor.) |
| `DamageAppliedMessage` | `health/data.rs` (diferido) | Emitido solo cuando Health aplicó daño a un target válido; los sistemas que reaccionan a daño recibido lo escucharán, no el pedido crudo. **Aterriza con su primer consumidor** (`Staggered` en `combat-defense` / `Flee` cuando el brain lea su `Health`) — ningún mensaje antes de que un sistema lo lea. |
| `DeathMessage` | `health/data.rs` | Emitido cuando `Health::apply_damage` deja `current <= 0.0`, exactamente una vez (un target ya muerto no re-emite). Health no decide qué pasa al morir (loot, respawn, despawn) — eso es de cada sistema dueño del actor. |

## Sistemas (comportamiento) — implementado

1. **`apply_damage`** (`HealthSet::Apply`, `FixedUpdate`, después de
   `ProjectilesSet::Simulate`, que ya corre tras
   `CombatSet::EmitConstraints` — ambos emisores del tick ya escribieron) —
   `MessageReader<DamageRequestMessage>`, llama `Health::apply_damage` en
   `target`; si cruza a `<= 0.0`, emite `DeathMessage(target)` exactamente
   una vez. (`DamageAppliedMessage` se emitirá aquí cuando exista su
   consumidor.)
2. Sin motor propio ni `ProposalBuffer`: Health no tiene estados exclusivos
   que arbitrar, es un pool simple — no aplica el patrón Broker. Ver
   `rationale/when-not-broker-pattern.md`.

**Reacciones a `DeathMessage` (cada dueño):** Player → respawn
(`player.rs`: teleport al spawn + `heal_full`); Enemy → despawn + cue
(`enemies/mod.rs`); target de práctica → despawn (`world.rs`).

## Relaciones con otros sistemas

| Relación | Categoría | Mecanismo |
|---|---|---|
| Combate calcula el monto de daño (incluye bonus de sigilo leyendo `Movement::LocomotionState::Sneak`) y emite `DamageRequestMessage` | MESSAGE | Health no conoce reglas de combate, solo aplica pedidos válidos |
| Combate escucha `DamageAppliedMessage`/`DeathMessage` dirigido a su propio actor y decide la transición `CombatState::Staggered` | MESSAGE | Health no elige estados de Combate — Combate reacciona al resultado aplicado |
| Enemies lee su propio `Health` para decidir `EnemyAiState::Flee` al estar herido (GDD §7) | READ | Query read-only |
| UI lee `Health` para HUD (vida) | READ | Query read-only |
| Proyectiles emiten `DamageRequestMessage` al impactar | MESSAGE | Ver `docs/architecture/projectiles.md` |
| Durabilidad de armas es un pool *estructuralmente similar* pero **no es `Health`** | ninguna | Vive en Inventory/Equipment — un arma no muere, se rompe y dejas de poder usarla |

## Decisiones abiertas

- `DamageKind` — enumerar tipos (físico, frío/calor, caída, eléctrico) para
  soportar resistencias/debilidades por raza o criatura (GDD §10).
- ~~Qué pasa al `DeathMessage` del jugador~~ — decidido graybox: respawn en
  el spawn de autor con vida completa (game over real llega con
  Persistence).
- ~~Qué pasa al `DeathMessage` de un Enemy~~ — decidido graybox: despawn +
  cue (loot llega con Inventory).
- Regeneración pasiva de `Health` (¿existe, como en BotW con comida?) — GDD
  no lo especifica.
