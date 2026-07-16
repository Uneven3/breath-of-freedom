# Ticket: `combat-bow-fixes` — correcciones post-revisión del arco

## Contexto

Revisión de código 2026-07-16 sobre HEAD (`52475eb`). La nota **KNOWN BUG**
de `WORKING-CONTEXT.md` ("la flecha nunca dispara") describía el código de
`1eebe9a`, donde `shoot_drawn_arrow` disparaba con el edge `attack.pressed`.
`52475eb` reescribió el modelo completo — mantener attack **carga**
(`tick_draw_strength`), soltar **dispara** — sin actualizar la nota. El trace
tick-a-tick del pipeline actual indica que el disparo al soltar funciona;
queda confirmarlo jugando. La revisión sí encontró los siguientes bugs
reales en HEAD, que este ticket corrige.

## Hallazgos y cambios

### 1. `just_fired` nunca es observable (feedback de disparo muerto)

`shoot_drawn_arrow` (`CombatSet::GatherProposals`) ponía
`DrawStrength.just_fired = true`, pero `tick_draw_strength`
(`CombatSet::TickActiveMotor`, **mismo tick, después**) lo reseteaba a
`false`. Ningún sistema de `Update` (p. ej. `juice::bow_fire_feedback`) lo
vio jamás: camera kick e hitstop de carga completa muertos.

**Cambio:** el flag se reemplaza por `BowFiredMessage { shooter, charge }`,
emitido por `shoot_drawn_arrow` y consumido por presentación con
`MessageReader` — mismo patrón de propiedad que `HitImpactMessage`
(Constitución §7). Un flag booleano de un tick además entregaba el edge
más de una vez a 144 fps de render (varios frames de `Update` por tick
fijo); el mensaje entrega exactamente una vez por lector.

### 2. Tap rápido pierde el disparo

Con press y release del ataque entre dos ticks de `FixedUpdate` (fácil con
render > 64 fps), al tick siguiente `held == false` pero `factor == 0.0`:
ni `released` ni carga — el edge `pressed` (por generaciones, sí llega) se
ignoraba y el disparo se perdía. El "instant tap-fire" del comentario en
`ARROW_SPEED_MIN` no existía para taps rápidos.

**Cambio:** `pressed && !held` dispara a carga mínima.

### 3. Violación §20: el origen de la flecha dependía de la cámara

`shoot_drawn_arrow` (simulación, `FixedUpdate`) leía `CameraRig` y
`cam_tf.translation` para el origen del proyectil: la trayectoria (gameplay)
variaba con el estado visual (`aim_blend` suavizado en `Update`, spring arm
comprimido por paredes). Además el fallback `body_muzzle` para actores sin
cámara hacía que un AI dispare distinto que el player por definición.

**Cambio:** el origen se computa solo de simulación:
`translation + Y·AIM_MUZZLE_HEIGHT + hombro(yaw)·AIM_SHOULDER_OFFSET`,
igual para todo actor. La alineación con la mira se conserva porque el rayo
de la mira pasa exactamente por el pivot del hombro de la cámara: Combat
(`aim.rs`) pasa a ser dueño de `AIM_MUZZLE_HEIGHT`/`AIM_SHOULDER_OFFSET` y
`camera.rs` los importa (presentación lee simulación — dirección permitida).

### 4. Latencia no determinista del spawn de flechas

`(spawn_arrows, fly_arrows)` no tenía orden respecto a `CombatSet`: el
scheduler podía colocarlos antes o después del emisor del mensaje, así que
la flecha aparecía con 0 o 1 tick de retraso según la corrida.

**Cambio:** `.after(CombatSet::EmitConstraints)` — el mensaje se consume el
mismo tick que se emite y la flecha vuela al tick siguiente (los `Commands`
aplican al final del schedule). Latencia estable de 1 tick.

### 5. Churn de assets en el hot path (Constitución §18)

`fly_arrows` hacía `meshes.add(...)` + `materials.add(...)` **por partícula
de estela por tick** (60+/s por flecha), y `spawn_arrows` por flecha: assets
nuevos en `FixedUpdate`, y cada material único rompe el batching del
renderer.

**Cambio:** recurso `ArrowAssets` creado una vez en `Startup`; los spawns
clonan handles.

### 6. Salto seco de FOV al disparar (menor)

`follow_player` asignaba `persp.fov = target_fov` sin suavizado;
`draw_factor` cae a 0 en un tick al disparar y el FOV pegaba un salto.

**Cambio:** lerp exponencial hacia el target, misma técnica que
`aim_blend`.

## Fuera de alcance (decisión abierta, anotada en `combat.md`)

Un bokobo flechado sin visión del atacante queda `ALERTED` por
`DirectThreatMessage` pero `next_ai_state` exige `visible && alerted` para
`Alert`: va a `Search` y **camina** hacia el atacante en vez de perseguir.
Puede ser diseño ("investiga") o no — se decide jugando en
`enemies-combat`, no aquí.

## Definición de terminado

- [x] `BowFiredMessage` reemplaza `just_fired`; `juice::bow_fire_feedback`
      lo consume.
- [x] Tap-fire a carga mínima.
- [x] Origen del proyectil 100 % simulación; constantes compartidas con la
      cámara con Combat como dueño.
- [x] Orden de Projectiles pinneado tras `CombatSet::EmitConstraints`.
- [x] `ArrowAssets` en `Startup`; cero `Assets::add` en `FixedUpdate`.
- [x] FOV suavizado.
- [x] fmt/clippy/test limpios.
- [ ] Checkpoint jugado: tensar → disparar al soltar; tap rápido dispara;
      kick de cámara al disparar; hitstop a carga completa; parábola y
      targets como en `combat-bow`.
