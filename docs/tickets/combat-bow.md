# Ticket: `combat-bow` — IMPLEMENTADO (pendiente checkpoint jugado)

## Sistema(s)

Combat (estado `Aiming` + motor `aim`), **Projectiles** (plugin nuevo),
Camera (cámara de apuntado + mira), World (targets de práctica), Input
(acción `Aim`). Adelantado desde la fase 6 del plan por pedido del usuario
(2026-07-15); `health-core`/`enemies-combat`/`combat-defense` siguen
pendientes.

## Qué se construyó

**Apuntado (Combat):**
- `IntentAction::Aim` (mouse derecho **o Q**, held; el mouse gateado por
  `PointerCaptured`, las teclas no) y `IntentAction::Attack` ganó **F** como
  alternativa de teclado — el usuario juega con touchpad sin botones
  (2026-07-15). `CombatIntents.wants_aim`.
- `CombatState::Aiming` (quinto variant — el dispatcher y `attack::propose`
  ganaron sus brazos por exhaustividad). `commits_the_body()` → sprint
  prohibido mientras apuntas; caminar/saltar libres.
- Motor `aim` (`combat/motors/aim.rs`): `propose` mantiene `Aiming` mientras
  el botón se sostiene (soltar = silencio → `Idle`); peso `AIM` pierde
  contra un inicio de melee el mismo tick, y el buffering de combos quedó
  restringido a las fases melee (un click apuntando es el disparo, no un
  swing encolado). `shoot_drawn_arrow` convierte `attack.pressed` en
  `SpawnProjectileMessage` con dirección de `ControlOrientation` (exactamente
  donde mira la cámara de apuntado).

> **Nota (2026-07-16):** `52475eb` reemplazó el disparo por edge con un
> modelo de carga — mantener attack tensa (`DrawStrength`, drena stamina),
> soltar dispara con velocidad/daño/precisión escalados por la carga — sin
> actualizar este ticket ni `WORKING-CONTEXT.md`. Las correcciones de la
> revisión posterior (feedback de disparo, tap-fire, origen del proyectil
> sin leer la cámara, orden de Projectiles, assets cacheados) viven en
> `combat-bow-fixes.md`.

**Projectiles (`src/projectiles/`):** lo propuesto en `projectiles.md`,
ahora real: `SpawnProjectileMessage { shooter, origin, velocity, damage }`
(dueño el receptor; consumido al tick siguiente, ~16 ms, latencia aceptada),
**vuelo parabólico** (gravedad de Movement + ray sweep del paso de cada tick
— sin tunneling), impacto contra capa `Actor` → daño (bonus ×4 contra
objetivo **no alertado**, sin exigir Sneak del tirador — contrato de
`combat.md`) + `HitImpactMessage` + `DirectThreatMessage` (el enemigo
aprende de dónde vino el disparo: hacia atrás a lo largo del vuelo) +
knockback ligero; contra mundo → **se clava y se desvanece** (4 s). Daño
como cue de log hasta `health-core`.

**Cámara (`camera.rs`):** `aim_blend` easing en el `CameraRig` — boom corto
(2.2 m), **hombro derecho** (offset del pivot para despejar la línea de
tiro), spring arm respetado; **mira**: punto central UI visible solo con el
blend activo.

**Targets de práctica (`world.rs`):** 3 dianas estáticas al este del curso
(cerca/alta/lejana) sobre postes. Viven en `GameLayer::Actor` → espada y
flechas las golpean, los sensores de Movement (mask `Default`) no las ven
(no escalables). `VisualOf(self)` → flashean y muestran número de daño como
cualquier actor. Para esto las queries de target de melee soltaron
`With<Actor>`: **la capa de colisión es el filtro de verdad**, no el marker.

## Definición de terminado

- [x] fmt/clippy/test limpios (140 tests; nuevos: dirección de apuntado por
      yaw/pitch, aim cede ante melee y no interrumpe un swing, regla de daño
      de flecha, cadena de combo no bufferea desde Aiming).
- [x] Docs sincronizados: `combat.md`, `projectiles.md`, `camera.md`,
      `WORKING-CONTEXT.md`.
- [ ] Checkpoint jugado: tensar (click derecho) → zoom sobre hombro + mira;
      disparar (click izquierdo) → parábola visible a distancia; targets:
      flash + número; flecha al bokobo sin que te vea → STEALTH SHOT ×4 +
      aggro hacia tu posición aproximada.
