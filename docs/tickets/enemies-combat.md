# Ticket: `enemies-combat` — IMPLEMENTADO (pendiente checkpoint jugado)

## Sistema(s)

Enemies (brain de combate, `EnemyAiState::Combat`, arquetipo arquero),
Combat (preset `WeaponProfile::BOKOBO_CLUB`), fase 4 de `combat.md`
extendida por pedido del usuario (2026-07-16): el enemigo ataca **con
espada y con arco**.

## Diseño

El principio de carga completo: los enemigos pelean escribiendo
`CombatIntents` (y, el arquero, su propia `ControlOrientation`) — los
mismos contratos que el jugador. Cero cambios en los motores de Combat:
`attack::propose/sweep`, `aim::propose/tick_draw_strength/shoot_drawn_arrow`
ya eran multi-actor; el bokobo solo necesita los componentes y un brain que
escriba intents.

**Arquetipos por composición** (la capability ES el componente, patrón
`weapon.rs`):

- **Bokobo melee**: `WeaponProfile::BOKOBO_CLUB` + `ComboLocal` +
  `ActiveSwing` — un golpe único telegrafiado (windup 0.35 s, daño 8,
  alcance 1.9 m, sin cadena).
- **Bokobo arquero**: `DrawStrength` + `ControlOrientation` (sin
  `WeaponProfile` → `attack::propose` lo ignora por query, no por flag).
  `EnemyBrainProfile::BOKOBO_ARCHER` mantiene distancia
  (`engage_distance 9 m`).

**`EnemyAiState::Combat` (nuevo variant):** `decide` entra desde `Alert`
cuando además el target está dentro de `attack_range` del perfil
(melee 1.9 m, arquero 13 m); sale a `Alert` si se aleja y a `Search` si
pierde la vista. `act` en `Combat` camina (Walk) hacia el target frenando
en `engage_distance` — el acercamiento mantiene el facing del melee; el
arquero apunta con `ControlOrientation`, su facing corporal es cosmético.

**Brain de combate (`enemies/combat.rs`, sistemas nuevos en el mismo chain
de `MovementSet::ReadIntents`, tras `brain::act`):**

- `act_melee` — en `Combat` y a rango, un press de attack (edge de un tick)
  cada `attack_cadence_secs` (1.2 s): swings sueltos, sin encadenar
  (la cadencia supera la ventana de chain — decisión de graybox).
- `act_archer` — en `Combat`: apunta `ControlOrientation` (yaw/pitch puros
  de `yaw_pitch_toward`, testeado como inversa de `aim_direction`) al pecho
  del target (`AggroTarget.last_seen`, que en vista es su posición actual),
  `wants_aim = true`, y **carga y suelta**: `attack.held` mientras
  `DrawStrength.factor < 0.65`; al cruzar el umbral suelta (held = false) y
  el motor `aim` dispara solo — el brain lee `DrawStrength` read-only (§5)
  y jamás emite `SpawnProjectileMessage` por su cuenta. Cadencia extra
  entre disparos además del cooldown del arma.
- `EnemyCombatLocal` — bookkeeping de cadencia por enemigo (componente,
  nunca `Local` — contrato multi-actor).

**Spawns (F7):** ahora spawnea/despawnea la pareja — melee en el punto de
autor existente y arquero en un segundo punto fijo del curso. Ambos con
`Health` (`health-core`).

**Sin cambios de contrato:** el sneakstrike/stealth ya leía el `Awareness`
del *target* — el player sin `Awareness` cuenta alertado, así que el bokobo
nunca te critea. `ForbidSprint` mientras el enemigo está comprometido ya
aplicaba por query `With<Actor>`. El feedback al player (screen flash,
trauma) estaba cableado dormido en `juice.rs` y despierta solo con estos
ataques.

## Decisión abierta que este ticket NO resuelve

Aggro por daño sin visión (Search-camina vs Alert-persigue) — anotada en
`combat.md` § Decisiones abiertas; se decide jugando este checkpoint.

## Definición de terminado

- [x] Enemigos atacan solo vía `CombatIntents`/`ControlOrientation`; nada
      escribe `CombatState`, buffers de otros, `Transform` ni velocidad.
- [x] Tests: transiciones de `next_ai_state` con `Combat`,
      `yaw_pitch_toward` ↔ `aim_direction` roundtrip, cadencia melee no
      spamea edges, aislamiento entre enemigos (§11 — invariantes).
- [x] fmt/clippy/test limpios.
- [ ] Checkpoint jugado: el melee te persigue y golpea (HP baja, screen
      flash); el arquero mantiene distancia, tensa y te clava flechas en
      parábola; matarlos despawnea; el feeling de cadencias/daños se tunea
      aquí.
