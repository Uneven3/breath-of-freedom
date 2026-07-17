# Movement

**Carpeta objetivo:** `src/movement/`

## Datos (Components/Messages/Resources)

| Tipo | Dónde | Qué es |
|---|---|---|
| `Intents` | `intents.rs` | Snapshot semántico: planar, solicitudes ortogonales sprint/sneak, salto, climb, ladder, traversal y glide. Lo escribe el Brain o el redirect propiedad de Movement. |
| `LocomotionState` | `state.rs` | Enum SSoT del modo activo. Solo `arbitrate` lo escribe. |
| `ProposalBuffer` | `proposal.rs` | Type alias sobre el núcleo genérico compartido de capacidad fija `proposal::ProposalBuffer<LocomotionState, N>` (`src/proposal.rs`), drenado por `arbitrate(current)`. Ver `rationale/proposal-arbitration-core.md`. |
| `TransitionProposal` | `proposal.rs` | `{ target_state, category: Priority, override_weight, source_id }`. Los pesos de desempate de los 13 motores viven juntos en `proposal.rs` (`mod weight`), con su orden total fijado por `const` asserts — dos motores que pueden co-proponer en la misma categoría nunca empatan. El módulo de tests `arbitration_matrix` escribe la matriz completa (motor → estado → emisiones `(Priority, weight)`) como dato y verifica en el build que cada `LocomotionState` tenga exactamente un motor dueño y que ningún par co-proponente empate, con una lista explícita de excepciones mutuamente excluyentes (glide/stairs/ladder). |
| `BodyVelocity` | `mod.rs` | Velocidad del cuerpo cinemático (análogo a `CharacterBody3D.velocity`). Solo el motor activo la escribe. |
| `BodyDimensions` | `body.rs` | Perfil físico persistente de la cápsula del actor: radio y longitudes de pie/agachado. Los servicios y motores usan sus alturas semánticas; `Collider` sigue siendo la forma física de Avian. |
| `GroundDriveProfile` | `abilities.rs` | Kernel data-driven de aceleración, coast, brake, reversa, alineación, giro y pérdida de velocidad; presets Player/Horse sin branches de especie. |
| `GroundMovement`, `SprintMovement`, `SneakMovement`, `StairsMovement` | `abilities.rs` | Capacidades terrestres independientes. Presencia concede permiso y cada una lleva su tuning; el horse omite Sneak. |
| `AirborneMovement` | `abilities.rs` | Perfil base de locomoción aérea para un actor sujeto a gravedad: controla Fall con velocidad, gravedad, salto corto, giro y recuperación por actor. No representa una acción discreta del jugador. |
| `ClimbMovement` | `abilities.rs` | Capacidad persistente y tuning por actor de Climb. |
| `LadderMovement` | `abilities.rs` | Capacidad persistente y velocidad por actor de Ladder. |
| `LedgeTraversal` | `abilities.rs` | Capacidad persistente y perfiles por actor de Mantle y AutoVault. |
| `WallJumpMovement` | `abilities.rs` | Capacidad persistente y perfiles por actor de WallJump y EdgeLeap. |
| `JumpMovement` | `abilities.rs` | Capacidad persistente y tuning por actor de Jump, incluidos impulso, coyote time y buffer. |
| `GlideMovement` | `abilities.rs` | Capacidad persistente y perfil por actor de Glide. |
| `GroundSensing` | `sensing.rs` | Perfil físico de GroundService: distancia del probe y umbral de ascenso por actor. Forma parte del núcleo cinemático. |
| `LedgeSensing` | `sensing.rs` | Perfil físico opcional de LedgeService: muestras de altura, alcances y umbrales de pared, borde y vault por actor. No concede acciones. |
| `SensingLod` | `lod.rs` | Decisión de LOD de sensing por actor, reescrita cada tick por `assign_sensing_lod`: tier (`Full`/`Reduced`) y si este tick le toca castear. Los servicios de SenseWorld la consultan vía `Option<&SensingLod>` (sin el componente, se sensa siempre). |
| `SensingLodConfig` | `lod.rs` | `Resource` de tuning del LOD: radio de tasa completa alrededor del jugador e intervalo de ticks para actores lejanos. Valores por defecto documentados en el propio tipo, pensados para ajustarse. |
| `Stamina` | `stamina.rs` | Pool opt-in de esfuerzo, fuera del core. Tenerlo no concede Sprint/Jump; solo sus métodos mutan el valor. |
| `BodyContact`, `GroundFacts` | `facts.rs` | Facts del núcleo cinemático. |
| `LedgeFacts`, `StairsFacts`, `LadderFacts` | `facts.rs` | Facts opcionales instalados con sus capacidades/sensing correspondientes. |
| `LocomotionConstraintMessage`, `LocomotionConstraintFacts` | `constraints.rs` | Restricciones semánticas pedidas por otros sistemas (Combate: `ForbidSprint` mientras hay compromiso de acción). Movement es dueño del mensaje (el receptor posee el contrato) y deriva los facts por actor antes de `GatherProposals`; expiran por silencio — el emisor re-emite cada tick. `sprint::propose` se abstiene bajo `forbid_sprint`. `Sneak` nunca se restringe. |
| `BodyImpulseMessage` | `constraints.rs` | Impulso one-shot sobre `BodyVelocity` (knockback de Combate). Se suma una vez antes de que los motores tiquen; la fricción/aceleración normal del motor activo lo reabsorbe — un empujón, no un estado. |
| `LocomotionEnabled` / `KinematicAttachment` | `attachment.rs` | Gate genérico del pipeline físico y pose local respecto de un carrier; Movement suspende/sincroniza actors adjuntos. |
| `PendingSafeRecovery` | `attachment.rs` | Detach de emergencia ya desligado pero todavía sin pose validada; mantiene collider y locomoción deshabilitados mientras Movement busca sin overlap. |
| `ControlRedirect` | `control.rs` | Redirect persistente con máscara; se instala o retira junto con el attachment mediante la transacción `ActorLinkRequestMessage`. |
| `ActorLinkRequestMessage` / `ActorLinkResultMessage` | `link.rs` | Attach/detach/neutralize atómicos con aceptación o rechazo explícito; Movement valida batch, cardinalidad, chains y ciclos antes de aplicar. |
| `ClimbInputState` | `brain.rs` | Estado del toggle de escalar por actor/controlador (tecla `1`, no un hold). No es `Resource` global, porque el contrato multi-actor requiere input independiente por actor. |
| `JumpPhase`, `JumpLocal`, `GlideLocal`, `SprintLock`, `StairsLocal` | `motors/jump.rs`, `motors/glide.rs`, `motors/sprint.rs`, `motors/stairs.rs` | Estado propio de cada motor (timers, latches, cache de peldaños), por-actor. Eran `Local<T>` de sistema antes de `multi-actor-migration` — promovidos a componente para no compartir estado entre actores. |
| `MantleState`, `VaultState`, `WallJumpState`, `EdgeLeapState` | `motors/mantle.rs`, `motors/auto_vault.rs`, `motors/wall_jump.rs`, `motors/edge_leap.rs` | Máquina de fase compartida entre `propose` y `tick` de cada motor, por-actor desde su diseño original (`Local` no se puede compartir entre dos sistemas). |
| `KinematicActorBundle` | `bundles.rs` | Conveniencia de construccion para el contrato físico y de pipeline de un actor cinemático. No es una capacidad ni activa sistemas. |
| Bundles de capacidad | `bundles.rs` | Ground, Sprint, Sneak, Stairs, Stamina, Jump, Glide, Ladder, Ledge y WallJump emparejan permiso/tuning con solo su estado runtime privado. |

`brain::read_intents` no lee `ButtonInput<KeyCode>` — lee
`input::ActiveActions` para el `InputSource` enlazado por
`input::InputControlledBy` en el actor (ver `input.md`) y arma
`PlanarMoveIntent`/`ClimbIntent`/`LadderIntent` a partir de
`MoveForward/Back/Left/Right`, y los demas tipos semanticos a partir de
`Jump`/`Sprint`/etc. Los gatillos discretos usan un
`input::InputConsumeCursor` propio de Movement/actor; Movement no muta el
snapshot global de input. Qué entrada física dispara cada `IntentAction` es
una tabla rebindeable que Movement no conoce — el mismo código sirve sin
cambios para `Gamepad`/`KeyboardOnly`/`KeyboardMouse`.

`Intents` es un contrato semantico de simulacion, no un reflejo de teclado o
gamepad: modela movimiento planar, gait, salto, climb, ladder, acciones de
travesia y glide con tipos nombrados. Las acciones de control se traducen en
el brain; los motores nunca interpretan ejes crudos ni acciones de hardware.

`JumpIntent::pressed` conserva el borde de Jump para motores que no pueden
permitirse perder una pulsación entre ticks fijos (`Jump`, `WallJump`,
`EdgeLeap` y salida de `Ladder`); `JumpIntent::held` sigue representando el hold.
En un borde válido de `Climb` o `Ladder`, Jump se interpreta como Mantle
(prioridad mayor que WallJump); fuera de ese contexto, Jump conserva el
WallJump de retroceso.

La composicion de capacidades persistentes y la frontera entre `Climb`,
`Mantle`, `AutoVault`, `WallJump` y `Ladder` esta definida en
[`rationale/movement-capability-composition.md`](rationale/movement-capability-composition.md).
Las capacidades no son estados activos ni activan sistemas: los motores se
seleccionan por `Query` y `LocomotionState` permanece exclusivo.

### Capacidades terrestres implementadas

Walk, Sprint, Sneak y Stairs son opt-ins separados. Un mismo
`ground_drive_step` consume `GroundDriveProfile` y distingue input alineado,
coast, brake, reversa y giro de alta velocidad sin conocer Player/Horse. El
Player compone las cuatro capacidades; el horse compone Ground/Sprint/Stairs
y usa perfiles de mayor inercia y steering limitado. La decisión y sus
invariantes viven en
[`rationale/movement-capability-composition.md`](rationale/movement-capability-composition.md).

El curso gris puede incluir un `TraversalProbe`: un segundo `Actor` sin
`Player` ni `InputControlledBy` cuyo brain de integracion escribe solo sus
propios `Intents` dentro de `ReadIntents`. F6 lo spawnea/despawnea en su
posición authored de mundo (frente al muro de prueba del graybox), nunca
relativa al jugador — el escenario debe ser reproducible corrida tras
corrida. No es un enemigo ni una excepcion
de Debug; demuestra que un controlador AI puede reutilizar capacidades,
sensores, propuestas y motores sin que Movement conozca su implementacion.
El escenario es la vuelta completa de travesía (ticket `probe-mantle-glide`):
avanza hasta que los sensores publican una pared escalable, solicita `Climb`,
asciende, **se sostiene bajo el borde sin mantle accidental** (el checkpoint
original, preservado como settle), hace `Mantle` al tope, se asienta, gira
180° (los motores rotan el cuerpo; el script solo observa la rotación),
salta, y baja en `Glide` hasta aterrizar — el aterrizaje solo cuenta si
`Glide` fue observado. Una maniobra solo se considera cubierta si sensores y
arbitraje normales alcanzan su condicion observada; el brain no reescribe la
posicion ni la velocidad para pasar una estacion.

Los bundles de `bundles.rs` son un limite de construccion, no de ejecucion:
`KinematicActorBundle` instala el contrato común de simulación (incluido el
perfil `GroundSensing`, que recibe como parámetro) y cada bundle de capacidad
instala su tuning más sus fases/latches privados. Un sistema no consulta
bundles; sigue consultando los componentes concretos. Input local, cámara,
IA y red se componen por fuera para que el mismo actor físico pueda tener
controladores distintos — el jugador local se arma en `src/player.rs`
(`PlayerPlugin`), no dentro de `MovementPlugin`.

Los servicios también seleccionan perfiles explícitos: GroundService requiere
`GroundSensing`, incluido por `KinematicActorBundle`; LedgeService requiere
`LedgeSensing`, que se agrega solo a actores que deben publicar hechos de
pared/borde. Los perfiles no reemplazan `BodyDimensions`: describen cómo se
sondea el mundo, mientras que las dimensiones describen el cuerpo.

**Los actores no son geometría escalable.** Todo actor cinemático declara
membresía en `world::GameLayer::Actor` (vía `KinematicActorBundle`), y los
casts de LedgeService enmascaran a `GameLayer::Default` (geometría de mundo):
ninguna cápsula — jugador, probe, enemigos futuros — se lee como pared de
climb, borde de mantle ni obstáculo de vault. Las capas no cambian los
contactos físicos (los cuerpos siguen chocando y se puede estar parado sobre
otro actor: GroundService sí ve actores a propósito); solo eligen qué ve cada
query espacial.

## Estados (`LocomotionState`)

`Walk`, `Sprint`, `Fall`, `Jump`, `AutoVault`, `Climb`, `Mantle`,
`Stairs`, `Ladder`, `Glide`, `Sneak`, `WallJump`, `EdgeLeap`. Default: `Fall`.
(No hay `Idle`: quieto en el suelo es `Walk` con input cero.)

El enum garantiza la exclusividad mutua; los estados **ortogonales**
(`Crouched`, latches de stamina) viven en componentes aparte. Cuándo usar
enum vs componente-presencia está fijado en
[`rationale/per-entity-state-idioms.md`](rationale/per-entity-state-idioms.md).

## Sistemas (comportamiento)

Pipeline en `FixedUpdate` a 60Hz, `SystemSet`s encadenados (`MovementSet`):

1. **ReadIntents** — brains de hardware, IA o red escriben `Intents` por actor.
2. **SenseWorld** — 4 servicios (`ground`, `ledge`, `stairs`, `ladder`) escriben
   los `*Facts`. Antes de este set corre `lod::assign_sensing_lod`: clasifica
   cada actor por distancia al jugador local y decide si este tick le toca
   castear. Los actores lejanos sensan cada `reduced_interval` ticks,
   escalonados por índice de entidad; sus `*Facts` quedan acotadamente
   desactualizados (el jugador siempre sensa a tasa completa). Ver
   `rationale/sensing-lod.md`.
3. **GatherProposals** — los 13 motores corren su `propose` cada frame, sin
   condición — cada uno decide si empuja un `TransitionProposal` al buffer.
4. **Arbitrate** — `arbitrate()` (`mod.rs`) elige el ganador por
   `(category, override_weight)` y escribe `LocomotionState`.
5. **TickActiveMotor** — sistemas `tick_body` encadenados consultan un
   `MotorCore` mínimo más la capacidad/facts/estado exactos de cada motor.
   Cada uno se activa solo para su `LocomotionState`; la SSoT de estado y el
   orden total garantizan un único writer corporal por actor y tick, sin una
   mega-query de capacidades opcionales. Ver
   `rationale/multi-actor-dispatch.md`.

`motors::sneak::sync_crouch_collider` corre en `FixedUpdate`, justo después
de `Arbitrate` y antes de `TickActiveMotor`, para que el motor activo tique con
la cápsula correcta en el mismo frame. El crouch es un **modificador ortogonal**
al estado: la cápsula agachada sigue la intención de crouch (o un stand-up
bloqueado) durante *cualquier* locomoción de suelo — Sneak plano **o** Stairs —,
no el estado `Sneak`. Por eso `Stairs` también agacha: `stairs::tick_body` usa la
media-altura agachada para el snap por peldaño y el `sneak_multiplier` del perfil
para la velocidad. Es forma física derivada; no decide locomoción. Antes de
arbitrar, `motors::sneak::update_stand_clearance` prueba la cápsula de pie
precalculada. Si hay techo, el actor se mantiene agachado aunque se libere el
botón; solo vuelve a crecer cuando la cápsula completa cabe.

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

**Monturas** (`docs/architecture/mounts.md`) usan el mismo pipeline `Actor`.
Mounts emite un `ActorLinkRequestMessage` atómico y solo confirma su relación
al recibir `ActorLinkResultMessage::Accepted`. Movement, sin conocer Mounts,
instala o retira attachment, redirect, collider y gate como una sola
transacción, suspende al rider mediante
`LocomotionEnabled`, sincroniza su `KinematicAttachment`, copia sólo planar,
sprint y salto al controlled actor y neutraliza al controller en
`MovementSet::ControlRedirect`. El orden es `ApplyExternal -> ReadIntents ->
ControlRedirect -> SenseWorld -> GatherProposals -> Arbitrate ->
TickActiveMotor -> SyncAttachments`; ver
`rationale/mounts-intent-redirect.md`.

`Rejected(CapacityPending)` reencola el request exacto en orden de lectura para
el tick posterior a `prepare_actor_link_workspace`. Un detach no validado o un
carrier perdido quita el link pero conserva `ColliderDisabled`, retira
`LocomotionEnabled` e instala `PendingSafeRecovery`; el recovery prueba un
número fijo de candidatos por tick y solo restaura al hallar uno sin overlap.

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
- **Contrato multi-actor:** `Query<.., With<Actor>>` + ticks encadenados con
  queries exactas por capacidad — implementado (ticket
  `multi-actor-migration`, luego reducido a `MotorCore`), ya no es un
  prerequisito pendiente de Enemies y Multiplayer. Ver
  `rationale/multi-actor-dispatch.md` y `docs/ARCHITECTURE-MAP.md`.
