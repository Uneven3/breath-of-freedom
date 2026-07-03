# Health

**Carpeta objetivo:** `src/health/`

Sistema hermano de Movement/Combat/Mounts — cualquier actor con vida
(jugador, Enemies, y potencialmente Monturas) usa el mismo `Health`, sin que
Combate necesite poseer el dato. Ver `rationale/health-ownership-boundary.md`.

## Datos (Components/Messages/Resources) — propuesta

| Tipo | Dónde | Qué es |
|---|---|---|
| `Health` | `health/mod.rs` | `{ current: f32, max: f32 }`, campos privados. Solo sus propios métodos (`apply_damage`, `heal`) la mutan — mismo patrón que `movement::Stamina`. |
| `DamageRequestMessage` | `health/messages.rs` | `{ target: Entity, amount: f32, source: Entity, kind: DamageKind }`. Cualquier sistema (Combat, hazards de clima, caída) lo emite; Health valida que el target tenga `Health` y aplica el daño. |
| `DamageAppliedMessage` | `health/messages.rs` | Emitido solo cuando Health aplicó daño a un target válido. Los sistemas que reaccionan a daño recibido escuchan este mensaje, no el pedido crudo. |
| `DeathMessage` | `health/messages.rs` | Emitido cuando `Health::apply_damage` deja `current <= 0.0`. Health no decide qué pasa al morir (loot, respawn, despawn) — eso es de cada sistema dueño del actor. |

## Sistemas (comportamiento) — propuesta

1. **ApplyDamage** — `MessageReader<DamageRequestMessage>`, llama
   `Health::apply_damage` en `target`; si el daño se aplicó, emite
   `DamageAppliedMessage`; si cruza a `<= 0.0`, emite
   `DeathMessage(target)`. Corre en `FixedUpdate` (es simulación, no
   presentación).
2. Sin motor propio ni `ProposalBuffer`: Health no tiene estados exclusivos
   que arbitrar, es un pool simple — no aplica el patrón Broker. Ver
   `rationale/when-not-broker-pattern.md`.

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
- Qué pasa al `DeathMessage` del jugador (respawn, game over).
- Qué pasa al `DeathMessage` de un Enemy (loot, despawn).
- Regeneración pasiva de `Health` (¿existe, como en BotW con comida?) — GDD
  no lo especifica.
