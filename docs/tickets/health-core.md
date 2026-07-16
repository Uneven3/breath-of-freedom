# Ticket: `health-core` — IMPLEMENTADO (pendiente checkpoint jugado)

## Sistema(s)

Health (`src/health/`, plugin nuevo — fase 3 de `combat.md`), con cableado
en Combat (melee), Projectiles (flechas), Player (respawn), Enemies
(despawn al morir), World (targets destructibles) y Debug (HUD).

## Qué se construyó

**El plugin (`src/health/`)**, siguiendo `docs/architecture/health.md`:

- `data.rs` — datos puros (Constitución §19):
  - `Health { current, max }`, campos privados; solo sus métodos
    (`apply_damage`, `heal_full`) la mutan — mismo patrón que
    `movement::Stamina`.
  - `DamageRequestMessage { target, amount }` — cualquier sistema lo
    emite; Health valida que el target tenga `Health`.
    (`source` para kill credit y `DamageKind` siguen fuera: un campo se
    agrega cuando un sistema lo lee, no antes.)
  - `DamageAppliedMessage` — **diferido**: aterriza con su primer
    consumidor (`Staggered` en `combat-defense` / `Flee`), ningún mensaje
    antes de que un sistema lo lea. El cue de daño aplicado es hoy el log
    de Health.
  - `DeathMessage { entity }` — emitido al cruzar `current <= 0.0`, una sola
    vez (un target ya muerto no re-emite). Health **no** decide qué pasa al
    morir — cada sistema dueño del actor reacciona.
- `mod.rs` — `apply_damage` en `FixedUpdate`, set `HealthSet::Apply`,
  después de `ProjectilesSet::Simulate` (que ya corre después de
  `CombatSet::EmitConstraints`): ambos emisores del tick ya escribieron.
  Sin Broker: un pool no tiene estados exclusivos que arbitrar
  (`rationale/when-not-broker-pattern.md`).

**Emisores reales** (reemplazan los cues de log "placeholder until
health-core"):

- `combat::motors::attack::resolve_melee_hits` emite
  `DamageRequestMessage`.
- `projectiles::resolve_arrow_hit` emite `DamageRequestMessage`.
  `HitImpactMessage`/`DirectThreatMessage`/knockback no cambian — siguen
  siendo canales paralelos.
- El cue de log vive ahora en Health (`[health] ...`), al aplicar.

**Reacciones a `DeathMessage`** (cada dueño, no Health):

- **Player** (`player.rs`): respawn — teleport al spawn de autor, velocidad
  a cero, `heal_full`. Escribir `Transform`/`BodyVelocity` aquí es la misma
  operación discreta que el spawn inicial (regla de juego del dueño del
  actor), no un bypass del pipeline de IA/control — la invariante de
  `WORKING-CONTEXT.md` aplica a *control de movimiento*, no a
  spawn/respawn.
- **Enemies** (`enemies/mod.rs`): despawn + cue. El visual huérfano ya lo
  limpia `visuals::despawn_orphaned_enemy_visual`.
- **World** (`world.rs`): los targets de práctica ganan `PracticeTarget` +
  `Health(30)` y despawnean al morir — destructibles, validan la ruta de
  muerte en un no-enemigo.

**HP asignados:** Player 100, bokobo melee 30, bokobo arquero 20,
targets 30. Primera pasada — se tunean en el checkpoint.

**HUD (`debug.rs`):** línea `hp: X/Y` del player.

## Hallazgo del checkpoint (2026-07-16): crash al matar al bokobo

Play-test: matar al bokobo panickeaba al aplicar buffers de comandos.
Causa: en el mismo frame de la muerte, `visuals::despawn_orphaned_enemy_visual`
encola el despawn del visual huérfano mientras `juice::flash_on_hit`
(sin orden relativo, mismo `Update`) encola `insert(HitFlash)` sobre ese
mismo visual — si el despawn aplica primero, el insert es sobre una entidad
inválida y el error handler por defecto de Bevy 0.19 panickea.

**Fix:** la presentación que toca entidades que otro sistema puede
despawnear el mismo frame usa los comandos tolerantes:
`flash_on_hit` → `try_insert`, `expire_hit_flash` → `try_remove`
(`juice.rs`), y el tracker de `sfx` → `try_insert` (carrera equivalente con
el toggle F7). Regla para el futuro: **un sistema de presentación nunca
asume que la entidad de simulación (o su visual) sigue viva al aplicar su
buffer** — comandos `try_*` o consumo por `Query::get`, jamás `insert`/
`remove`/`despawn` a ciegas sobre entidades ajenas.

- [x] `Health` solo mutada por sus métodos; requests a entidades sin
      `Health` o ya muertas se ignoran (sin panic, §9).
- [x] `DeathMessage` se emite exactamente una vez por muerte.
- [x] Tests de invariantes: aplicar/clamp, muerte única, request inválido
      ignorado (§11 — contratos, no feeling).
- [x] fmt/clippy/test limpios.
- [ ] Checkpoint jugado: matar al bokobo a espadazos/flechazos (despawn),
      destruir un target, morir a manos del bokobo y respawnear.
