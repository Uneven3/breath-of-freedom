# Arquitectura y rationale

El código es la documentación de lo que se hizo; este archivo (≤200 líneas)
documenta las **leyes** que todo código debe cumplir y **por qué** la
arquitectura es la que es. El detalle vive en los módulos y sus tests.
Historial de diseño: `git log -- docs/`.

## Leyes (la Constitución — el código las cita por §)

Código que viole estas leyes no se implementa ni mergea.

- **§1** Responsabilidad única por plugin/sistema/componente.
- **§2** Extender agregando sistemas/componentes, no editando lógica ajena.
- **§3** Un trait implementado honra lo que el trait promete.
- **§4** APIs públicas mínimas: solo lo que el llamador necesita.
- **§5** Depender de componentes/mensajes expuestos, jamás de internals.
- **§6** Components/Resources/Messages son datos puros; la lógica va en
  sistemas (helpers puros ok).
- **§7** Cada sistema muta solo lo que posee. Comunicación cruzada diferida =
  `Message` (0.19: `MessageReader`/`Writer`); `Event`/observer solo si la
  inmediatez es explícitamente necesaria.
- **§8** Evitar `unwrap()`/`expect()`: estados inválidos irrepresentables por
  tipos, no panics en runtime. (Tests exentos.)
- **§9** Panic = bug de programador. Todo lo que el juego puede producir
  (asset faltante, input raro, red) se modela con `Result`/`Option`.
- **§10** *Checkpoint* = comportamiento validado **jugándolo**.
- **§11** Tests después del checkpoint (no fijar feeling no validado).
  Excepción obligatoria: invariantes de arquitectura/ECS se testean desde el
  diseño (no-bleed entre actores, ordering, overflow, contratos multi-actor).
- **§12** Sin `unsafe` en el proyecto.
- **§13** `cargo fmt` + `clippy -D warnings` antes de terminar; `#[allow]`
  solo con justificación puntual.
- **§14** Un plugin por sistema, carpeta propia bajo `src/`.
- **§15** Comentarios solo para lo que el código no puede decir (invariantes,
  restricciones, workarounds). Nunca el *qué*.
- **§16** ~300 líneas es señal de dividir, no bloqueo.
- **§17** Dependencia nueva en `Cargo.toml` requiere OK humano previo.
- **§18** Sin allocations en el hot path de `FixedUpdate`.
- **§19** Datos separados de sistemas en archivos distintos
  (`state.rs`/`intents.rs` vs `mod.rs`/motores).
- **§20** Simulación (FixedUpdate) nunca depende de nada visual. Cámara, HUD,
  interpolación y cues viven en `Update` y solo **leen**.

## El pipeline seleccionado

```text
PreUpdate   Input: hardware → ActiveActions + ControlOrientation
            (encadenado tras bevy InputSystems; ver regla de schedules)
FixedUpdate Brain → Intents → [Sense → Propose → Arbitrate → Tick motor] → Body
            Movement sets:  ApplyExternal → ReadIntents → ControlRedirect →
                            SenseWorld → GatherProposals → Arbitrate →
                            TickActiveMotor → SyncAttachments
            Mounts sets:    Request → Lifecycle (antes de ApplyExternal),
                            Confirm (antes de ReadIntents), PostMove,
                            Charge, DeathCleanup (tras Health)
            Combat sets:    ApplyContext → ReadIntents → GatherProposals →
                            Arbitrate → TickActiveMotor → EmitConstraints
                            (todo tras Movement::TickActiveMotor)
            Projectiles → Health::Apply → death cleanup
Update      Presentación: visuals, camera, HUD/debug, juice, sfx — solo READ
```

**Regla de schedules:** Bevy corre `FixedUpdate` *antes* que `Update` en cada
frame. Por eso todo lo que la simulación lee del hardware se resuelve en
`PreUpdate`; un escritor en `Update` llega un frame tarde (fue el hallazgo
crítico del audit 2026-07-17).

## Por qué (rationale destilado)

- **Multi-actor por `Actor` + `Intents`.** Todo cuerpo (player, enemigo,
  horse, probe, futuro jugador remoto) es una entidad `Actor`; IA y red se
  mueven **solo** escribiendo `Intents`/`CombatIntents` — nunca `Transform`,
  `BodyVelocity`, `LocomotionState`, facts ni estado privado de motores.
  Esto es lo que hace barato agregar animales/NPCs/co-op: un Brain nuevo,
  cero motores nuevos. Los motores despachan por capacidad
  (`GroundMovement`, `JumpMovement`, …), no por identidad: el horse es "un
  actor con otro set de capacidades", no un caso especial.
- **Árbitro central por sistema.** Motores *proponen* transiciones a un
  `ProposalBuffer` de capacidad fija (núcleo compartido `src/proposal.rs`,
  prioridad → peso → orden); un solo sistema arbitra y es el único escritor
  de `LocomotionState`/`CombatState`. Nada de estados concurrentes ni
  motores escribiéndose entre sí. Tests `arbitration_matrix` fijan que cada
  estado tiene exactamente un motor dueño.
- **El receptor posee el contrato.** El mensaje lo define quien lo consume
  (`LocomotionConstraintMessage` es de Movement aunque lo emita Combat;
  `DamageRequestMessage` es de Health). Las restricciones expiran por
  silencio: el emisor re-emite cada tick mientras la condición dure. El veto
  `ForbidSprint` llega 1 tick tarde por el orden Movement→Combat — aceptado
  y fijado con test de regresión.
- **Mounts vía ActorLink transaccional.** Mounts pide
  `Attach`/`Detach`/`Neutralize` por mensaje; Movement instala/retira
  atómicamente attachment, redirect de control, collider y gate, y responde
  con ack. Mounts solo confirma su relación (`MountedOn`/`RiddenBy`) desde
  un ack aceptado. Los requests aplican el mismo tick (el workspace se
  dimensiona en el propio tick — no existe rechazo por capacidad). Detach
  sin pose segura = collider off + suspensión hasta que Movement encuentre
  pose válida (`PendingSafeRecovery`, candidatos fijos por tick).
- **Salud y hostilidad.** Combat/Projectiles/Charge consultan
  `HostileInteractionImmunity` antes de toda consecuencia y emiten
  `DamageRequestMessage`; Health re-valida, aplica y emite `DeathMessage`.
  La reacción a la muerte vive con el dueño del actor (Player respawnea en
  `player.rs`, enemigos en `enemies/`, targets en `world/`). No existe
  `DamageAppliedMessage`: se diseñará con su primer consumidor real.
- **Percepción por marcador.** Los enemigos perciben actores `Perceivable`
  (marcador de Perception; hoy solo el player). Cuando la hostilidad
  necesite más de un bit (animales, aliados), se reemplaza por facción.
- **Presentación desechable.** Cada actor tiene un visual separado que
  interpola hacia el cuerpo (`VisualOf` lo enlaza para efectos
  transversales). La simulación no porta meshes ni handles. Sistemas de
  presentación que tocan entidades despawneables el mismo frame usan
  comandos tolerantes (`try_insert`). El GLB del player trae rig+clips en un
  archivo: el loader auto-inserta `AnimationPlayer`/targets, sin linking
  manual de huesos.
- **Combate apuntado en dos fases.** El rayo del crosshair nace del pivote a
  altura de ojos (`AIM_PIVOT_HEIGHT`, simulación pura — §20; la cámara lo
  importa y se alinea al apuntar) y resuelve el target; la flecha sale del
  socket del arco (`BOW_SOCKET_LOCAL`, compartido con el visual)
  convergiendo. Fallbacks a la línea de mira: a quemarropa, y cuando
  cualquier obstáculo bloquea la línea del arco que el crosshair despejó —
  "si lo veo, puedo dispararle". La carga estilo Bannerlord (velocidad/daño
  escalan, spread castiga soltar rápido) y la caída parabólica son diseño.
- **Capas de física.** `GameLayer::{Default, Actor}`: los contactos físicos
  cruzan capas; lo que compran es *sensing selectivo* (el sensing de ledges
  enmascara a `Default` para no trepar cápsulas ajenas; espada/flechas
  seleccionan `Actor`).
- **Mundo en tres capas.** `world/mod.rs` (tipos y reglas propias),
  `world/spawn.rs` (mecanismo agnóstico del nivel), `world/layout.rs` (el
  nivel como tablas de datos + geometría derivada). Agrandar el mapa es
  editar `layout.rs`; esa es la costura donde un loader de assets
  (RON/escena GLTF) se enchufa sin tocar el mecanismo.
- **Checkpoint jugado, luego tests.** El feeling se valida jugando
  (§10-§11); el loop operativo es: implementar → `fmt`+`clippy`+`test` →
  lanzar el juego para el usuario → leer el log de la sesión
  (`[health] X took N`, `[debug] animation clip …`) antes de reportar.

## Mapa de módulos (los que existen)

| Módulo | Posee | Frontera |
|---|---|---|
| `input` | `ActiveActions`, `ControlOrientation`, bindings | Nadie lee hardware salvo él; resuelve en PreUpdate |
| `movement` | `Intents`, `LocomotionState`, motores, facts, attachment/link | Brains escriben Intents; Combat pide por mensaje |
| `proposal` | Núcleo genérico de arbitración | Type-aliases por sistema |
| `combat` | `CombatIntents`, `CombatState`, motores, perfiles montados | Tras Movement; emite constraints/daño por mensaje |
| `projectiles` | Flechas: vuelo parabólico, impacto | Spawn por mensaje de Combat |
| `health` | `Health`, inmunidad, aplicación autoritativa de daño | Único que resta HP; muerte por mensaje |
| `enemies` | Percepción, `Awareness`, brains melee/arquero | Escribe solo sus `*Intents`/`ControlOrientation` |
| `mounts` | `Horse`, relación, owner, carga | Todo cambio físico vía ActorLink a Movement |
| `player` | Spawn y respawn del jugador | Dueño de la reacción a su muerte |
| `world` | Geometría, capas, nivel, targets | Sustrato: no lee a nadie |
| `visuals`, `camera`, `presentation`, `sfx`, `debug` | Presentación | Solo READ sobre simulación (§20); debug F1-F7 |

Sistemas futuros (inventario, crafteo, swim, snowboard, clima, NPCs,
multiplayer, persistencia) se diseñan cuando toquen, como consumidores
aditivos de estos contratos — sus borradores viejos están en git.
