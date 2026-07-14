# Movement

**Carpeta objetivo:** `src/movement/`

## Datos (Components/Messages/Resources)

| Tipo | Dónde | Qué es |
|---|---|---|
| `Intents` | `intents.rs` | Snapshot de input del frame (move_dir, wish_dir, wants_jump, wants_sprint, wants_sneak, wants_climb, wants_mantle, wants_vault, wants_glide). Solo el Brain lo escribe. |
| `LocomotionState` | `state.rs` | Enum SSoT del modo activo. Solo `arbitrate` lo escribe. |
| `ProposalBuffer` | `proposal.rs` | Type alias sobre el núcleo genérico compartido de capacidad fija `proposal::ProposalBuffer<LocomotionState, N>` (`src/proposal.rs`), drenado por `arbitrate(current)`. Ver `rationale/proposal-arbitration-core.md`. |
| `TransitionProposal` | `proposal.rs` | `{ target_state, category: Priority, override_weight, source_id }`. |
| `BodyVelocity` | `mod.rs` | Velocidad del cuerpo cinemático (análogo a `CharacterBody3D.velocity`). Solo el motor activo la escribe. |
| `BodyDimensions` | `body.rs` | Perfil físico persistente de la cápsula del actor: radio y longitudes de pie/agachado. Los servicios y motores usan sus alturas semánticas; `Collider` sigue siendo la forma física de Avian. |
| `GroundMovement` | `abilities.rs` | Capacidad persistente y perfiles de tuning de Walk, Sprint, Sneak y Stairs por actor. Esos motores solo consideran actores que llevan este componente. |
| `AirborneMovement` | `abilities.rs` | Perfil base de locomoción aérea para un actor sujeto a gravedad: controla Fall con velocidad, gravedad, salto corto, giro y recuperación por actor. No representa una acción discreta del jugador. |
| `ClimbMovement` | `abilities.rs` | Capacidad persistente y tuning por actor de Climb. |
| `LadderMovement` | `abilities.rs` | Capacidad persistente y velocidad por actor de Ladder. |
| `LedgeTraversal` | `abilities.rs` | Capacidad persistente y perfiles por actor de Mantle y AutoVault. |
| `WallJumpMovement` | `abilities.rs` | Capacidad persistente y perfiles por actor de WallJump y EdgeLeap. |
| `JumpMovement` | `abilities.rs` | Capacidad persistente y tuning por actor de Jump, incluidos impulso, coyote time y buffer. |
| `GlideMovement` | `abilities.rs` | Capacidad persistente y perfil por actor de Glide. |
| `GroundSensing` | `sensing.rs` | Perfil físico de GroundService: distancia del probe y umbral de ascenso por actor. Forma parte del núcleo cinemático. |
| `LedgeSensing` | `sensing.rs` | Perfil físico opcional de LedgeService: muestras de altura, alcances y umbrales de pared, borde y vault por actor. No concede acciones. |
| `Stamina` | `stamina.rs` | Pool de esfuerzo. Solo sus propios métodos `drain`/`recover` la mutan. |
| `BodyContact`, `GroundFacts`, `LedgeFacts`, `StairsFacts`, `LadderFacts` | `facts.rs` | Hechos del mundo que los servicios calculan y los motores leen. |
| `ClimbInputState` | `brain.rs` | Estado del toggle de escalar por actor/controlador (tecla `1`, no un hold). No es `Resource` global, porque el contrato multi-actor requiere input independiente por actor. |
| `JumpPhase`, `JumpLocal`, `GlideLocal`, `SprintLock` | `motors/jump.rs`, `motors/glide.rs`, `motors/sprint.rs` | Estado propio de cada motor (timers, latches), por-actor. Eran `Local<T>` de sistema antes de `multi-actor-migration` — promovidos a componente para no compartir estado entre actores. |
| `MantleState`, `VaultState`, `WallJumpState`, `EdgeLeapState` | `motors/mantle.rs`, `motors/auto_vault.rs`, `motors/wall_jump.rs`, `motors/edge_leap.rs` | Máquina de fase compartida entre `propose` y `tick` de cada motor, por-actor desde su diseño original (`Local` no se puede compartir entre dos sistemas). |
| `KinematicActorBundle` | `bundles.rs` | Conveniencia de construccion para el contrato físico y de pipeline de un actor cinemático. No es una capacidad ni activa sistemas. |
| `GroundMovementBundle`, `JumpMovementBundle`, `GlideMovementBundle`, `LedgeTraversalBundle`, `WallJumpMovementBundle` | `bundles.rs` | Conveniencias de construccion: emparejan cada capacidad con el estado runtime privado de sus propios motores. |

`brain::read_intents` no lee `ButtonInput<KeyCode>` — lee
`input::ActiveActions` para el `InputSource` enlazado por
`input::InputControlledBy` en el actor (ver `input.md`) y arma
`move_dir`/`wish_dir` a partir de
`MoveForward/Back/Left/Right`, y `wants_jump`/`wants_sprint`/etc. a partir
de `Jump`/`Sprint`/etc. Los gatillos discretos usan un
`input::InputConsumeCursor` propio de Movement/actor; Movement no muta el
snapshot global de input. Qué entrada física dispara cada `IntentAction` es
una tabla rebindeable que Movement no conoce — el mismo código sirve sin
cambios para `Gamepad`/`KeyboardOnly`/`KeyboardMouse`.

`Intents::jump_pressed` conserva el borde de Jump para motores que no pueden
permitirse perder una pulsación entre ticks fijos (`Jump`, `WallJump`,
`EdgeLeap` y salida de `Ladder`); `wants_jump` sigue representando el hold.
En un borde válido de `Climb` o `Ladder`, Jump se interpreta como Mantle
(prioridad mayor que WallJump); fuera de ese contexto, Jump conserva el
WallJump de retroceso.

La composicion de capacidades persistentes y la frontera entre `Climb`,
`Mantle`, `AutoVault`, `WallJump` y `Ladder` esta definida en
[`rationale/movement-capability-composition.md`](rationale/movement-capability-composition.md).
Las capacidades no son estados activos ni activan sistemas: los motores se
seleccionan por `Query` y `LocomotionState` permanece exclusivo.

Los bundles de `bundles.rs` son un limite de construccion, no de ejecucion:
`KinematicActorBundle` instala el contrato común de simulación y cada bundle
de capacidad instala su tuning más sus fases/latches privados. Un sistema no
consulta bundles; sigue consultando los componentes concretos. Input local,
cámara, IA y red se componen por fuera para que el mismo actor físico pueda
tener controladores distintos.

Los servicios también seleccionan perfiles explícitos: GroundService requiere
`GroundSensing`, incluido por `KinematicActorBundle`; LedgeService requiere
`LedgeSensing`, que se agrega solo a actores que deben publicar hechos de
pared/borde. Los perfiles no reemplazan `BodyDimensions`: describen cómo se
sondea el mundo, mientras que las dimensiones describen el cuerpo.

## Estados (`LocomotionState`)

`Walk`, `Sprint`, `Fall`, `Jump`, `AutoVault`, `Climb`, `Mantle`,
`Stairs`, `Ladder`, `Glide`, `Sneak`, `WallJump`, `EdgeLeap`. Default: `Fall`.
(No hay `Idle`: quieto en el suelo es `Walk` con input cero.)

## Sistemas (comportamiento)

Pipeline en `FixedUpdate` a 60Hz, `SystemSet`s encadenados (`MovementSet`):

1. **ReadIntents** — brains de hardware, IA o red escriben `Intents` por actor.
2. **SenseWorld** — 4 servicios (`ground`, `ledge`, `stairs`, `ladder`) escriben los `*Facts`.
3. **GatherProposals** — los 13 motores corren su `propose` cada frame, sin
   condición — cada uno decide si empuja un `TransitionProposal` al buffer.
4. **Arbitrate** — `arbitrate()` (`mod.rs`) elige el ganador por
   `(category, override_weight)` y escribe `LocomotionState`.
5. **TickActiveMotor** — los motores corren sobre `Query<.., With<Actor>>` y
   se auto-filtran con un guard interno por entidad
   (`if *state != LocomotionState::X { continue }`) en vez de un `run_if`
   global — esto es lo que permite simulación concurrente e independiente de
   jugadores locales, remotos, enemigos y otros cuerpos controlables. Ver
   `rationale/multi-actor-dispatch.md`.

`motors::sneak::sync_sneak_collider` corre en `FixedUpdate`, justo después
de `Arbitrate` y antes de `TickActiveMotor` (swap de collider, declarativo
sobre `Changed<LocomotionState>` + cruce del límite Sneak vía `Crouched`),
para que el motor activo tique con la cápsula correcta en el mismo frame.
Es forma física derivada; no decide locomoción. Antes de arbitrar,
`motors::sneak::update_stand_clearance` prueba la cápsula de pie precalculada.
Si hay techo, Sneak se mantiene aunque se libere el botón; solo vuelve a crecer
cuando la cápsula completa cabe.

`Ladder` es un motor de anclaje: `LadderService` publica base, cima, normal y
una línea authored para el centro del cuerpo. Ladder entra solo mediante el
toggle de climb, fija el eje horizontal y acepta solo velocidad vertical, sin
consumir stamina. En el borde superior no aplica un impulso: `Mantle` propone
la salida solo con su acción manual; Jump propone `WallJump`. World puede
marcar la pared con
`NonClimbable`; LedgeService bloquea entonces solo el motor `Climb`, sin
ocultar esa geometría a Mantle/Vault.

`Stairs` modela un tramo recto uniforme: base, cima, cantidad, huella y
contrahuella. Su trigger es un oriented box authored; una escalera curva se
compone de tramos adyacentes de un peldaño. `StairsService` publica el tramo
que contiene al actor y el motor limita su muestra de snap a la huella del
tramo, por lo que una huella corta no salta varias contrahuellas. La salida a
una pendiente pertenece a `GroundService`: el trigger termina en la última
huella y ambas superficies deben coincidir en altura.

## Relaciones con otros sistemas

Movement expone datos de simulación para lectura read-only y mensajes de
interrupción; otros sistemas no mutan su estado interno directamente.

**Punto de extensión:** mensajes semánticos como
`LocomotionConstraintMessage::ForbidSprint` o
`LocomotionConstraintMessage::Interrupt` permiten que sistemas como Combate o
Salud pidan restricciones/interrupciones sin escribir directamente el estado
de Movement. Movement valida esos pedidos contra sus facts físicos y decide
el `LocomotionState` final — ver `docs/architecture/combat.md` § Relaciones.

**Monturas** (`docs/architecture/mounts.md`) no modifica Movement ni obliga a
`brain::read_intents` a conocer `MountIntents`. El input montado se resuelve
con un sistema de traducción propio de Mounts que lee `Intents` del jinete y
escribe `MountIntents` en la montura — ver
`rationale/mounts-intent-redirect.md`.

**Enemies** y **Multiplayer** no modifican Movement — reutilizan el mismo
`Intents`/`LocomotionState` con un Brain distinto (IA o red), pero ambos
requieren el contrato multi-actor descrito abajo.

## Decisiones abiertas

- Interpolación visual >60Hz (Avian transform interpolation).
- Cámara: colisión de spring-arm y landing dip.
- Regla de sprint en escaleras/muros escalables.
- Nadar/Bucear y Snowboard (GDD §9) se diseñan como motores nuevos de este
  mismo plugin — ver `swim.md`, `snowboard.md` y
  `rationale/traversal-extensions-in-movement.md`.
- **Contrato multi-actor:** `Query<.., With<Actor>>` + guard interno por
  entidad — implementado (ticket `multi-actor-migration`), ya no es un
  prerequisito pendiente de Enemies y Multiplayer. Ver
  `rationale/multi-actor-dispatch.md` y `docs/ARCHITECTURE-MAP.md`.
