# Ticket: `mounts-core` — POR IMPLEMENTAR

Montura terrestre de graybox, montarla y cabalgar. Diseño ya fijado en
`docs/architecture/mounts.md` + `rationale/mounts-intent-redirect.md` —
**leer ambos antes de empezar**; este ticket solo fija el alcance MVP y las
decisiones que faltaban. Regla general del repo: `docs/CONSTITUTION.md`.

## Alcance MVP (decidido 2026-07-16)

- **Una especie terrestre** de graybox ("kelpo", caja/cápsula, sin IP de
  Zelda). Spawn/despawn con **F8** en punto fijo del curso (patrón F7 del
  bokobo en `enemies/mod.rs`). Sin voladoras, sin `Rear`, sin `Bond`, sin
  doma, sin Health de montura (queda abierto).
- Estados MVP: `Idle`, `Gallop` (movimiento), `Sprint` (con `wants_boost` =
  Sprint del jinete). Enum completo del doc puede declararse ya; solo estos
  tres tienen motor.
- Input: nueva `IntentAction::Interact` (tecla **E**, edge por
  `InputConsumeCursor`) para montar/desmontar a ≤ 2.5 m.

## Arquitectura (resumen de lo ya decidido)

- `src/mounts/` plugin propio (§14), datos separados de sistemas (§19):
  `intents.rs` (`MountIntents`), `state.rs` (`MountLocomotionState`),
  `proposal.rs` (alias sobre `crate::proposal::ProposalBuffer`, pesos
  centralizados como `combat/proposal.rs`), `mod.rs` (plugin + `MountSet`).
- Pipeline hermano en `FixedUpdate`:
  `TranslateIntent .after(MovementSet::ReadIntents)` → `GatherProposals` →
  `Arbitrate` (único escritor de `MountLocomotionState`) →
  `TickActiveMotor` (dispatcher con `match` exhaustivo, patrón
  `combat/motors/mod.rs`).
- `translate_mount_intents`: query jinetes con `MountedOn(mount)`, copia su
  `Intents` (el `planar` ya viene resuelto por el brain de Movement) a
  `MountIntents` de la montura y **neutraliza el `Intents` del jinete**
  (`*intents = default()`) — el redirect consume el input: los motores del
  jinete ven quietud. Movement no conoce Mounts (rationale).

## Decisiones nuevas de este ticket

- **Jinete montado:** al montar, insertar `MountedOn(mount)`, desactivar su
  colisión con el mecanismo de Avian (verificar API: `ColliderDisabled` o
  equivalente en 0.7), `BodyVelocity = 0`. Cada tick, en
  `MountSet::TickActiveMotor` **después** del motor de la montura, Mounts
  fija el `Transform` del jinete al anclaje de silla (offset constante
  sobre el centro de la montura) y su rotación a la de la montura.
  Excepción documentada tipo respawn: Mounts es dueño de la colocación del
  jinete mientras `MountedOn` existe — no es el pipeline de control.
  La cámara sigue al Player por transform → funciona sola.
- **Desmontar:** E de nuevo → quitar `MountedOn`, reactivar colisión,
  colocar al costado de la montura (lado libre por ray corto), heredar la
  velocidad planar de la montura en `BodyVelocity` (rationale § física).
- **Cuerpo de la montura:** `RigidBody::Kinematic` + cápsula propia en
  `GameLayer::Actor`, `MountBody` marker. **No** usar
  `KinematicActorBundle` (eso la haría `Actor` del pipeline de Movement).
  Motor `gallop`: aceleración/fricción planar hacia `move_dir`, rotación
  hacia la dirección de movimiento, snap al suelo por ray + `GRAVITY` si no
  hay piso (referencia conceptual: `motor_common.rs`, sin reutilizarlo).
  Tuning primera pasada: Gallop ~9 m/s, Sprint ~13 m/s; se tunea jugando.
- **Combate montado:** sin gate en MVP — los `CombatIntents` del jinete
  siguen vivos (arco a caballo puede simplemente funcionar). Se evalúa en
  el checkpoint; anotado como decisión abierta en `mounts.md`.

## Tests (§11 — invariantes, no feeling)

- Traducción aislada: dos jinetes/monturas no se cruzan intents; un actor
  sin `MountedOn` no escribe `MountIntents` de nadie.
- El redirect neutraliza el `Intents` del jinete el mismo tick.
- Arbitraje: `Idle` por silencio; `Gallop` con input; matriz de pesos sin
  empates (patrón `arbitration_matrix`).
- Montar/desmontar es simétrico (componentes restaurados).

## Definición de terminado

- [ ] fmt / clippy `-D warnings` / tests limpios.
- [ ] Docs sincronizados: `mounts.md` (estado implementado + decisiones de
      arriba), `input.md` si existe la acción, `WORKING-CONTEXT.md`.
- [ ] Checkpoint jugado (lo valida el usuario): E monta, WASD cabalga con
      cámara normal, Shift galopa más rápido, E desmonta al costado con
      inercia, F8 spawn/despawn. Sin crash al desmontar en movimiento.

## Advertencias al implementador

- AI/red escriben solo `Intents` — la montura se mueve únicamente por
  `MountIntents` traducidos; jamás escribir `Transform`/velocity de la
  montura desde un brain (invariantes en `WORKING-CONTEXT.md`).
- Presentación que toque entidades despawneables el mismo frame:
  `try_insert`/`try_remove` (lección en `health-core.md`).
- Sin allocations en `FixedUpdate` (§18); assets cacheados en `Startup`
  (patrón `ArrowAssets` en `projectiles/mod.rs`).
- Crate nuevo = prohibido sin aprobación (§17).
