# Arquitectura y rationale

El código documenta lo hecho; este archivo (≤200 líneas) fija las **leyes** y el
**por qué** arquitectónico. El detalle vive en módulos/tests; historial: `git log -- docs/`.

## Leyes (la Constitución — el código las cita por §)

Código que viole estas leyes no se implementa ni mergea.

- **§1** Responsabilidad única por plugin/sistema/componente.
- **§2** Extender agregando sistemas/componentes, no editando lógica ajena.
- **§3** Un trait implementado honra lo que el trait promete.
- **§4** APIs públicas mínimas: solo lo que el llamador necesita.
- **§5** Depender de componentes/mensajes expuestos, jamás de internals.
- **§6** Components/Resources/Messages son datos puros; la lógica va en
  sistemas (helpers puros ok).
- **§7** Cada sistema muta solo lo que posee. Comunicación diferida = `Message`
  (0.19: `MessageReader`/`Writer`); `Event`/observer solo si exige inmediatez.
- **§8** Los `[lints]` prohíben `unwrap()`/`expect()` en producción. Tests exentos.
- **§9** Panic = bug de programador. Todo lo que el juego puede producir
  (asset faltante, input raro, red) se modela con `Result`/`Option`.
- **§10** *Checkpoint* = comportamiento validado **jugándolo**.
- **§11** Tests después del checkpoint; invariantes arquitectura/ECS sí se testean
  desde diseño (no-bleed, ordering, overflow, contratos multi-actor).
- **§12** `unsafe_code = "forbid"` en cada crate.
- **§13** `[lints]` en `deny` + `cargo fmt` y Clippy antes de terminar; `#[allow]`
  solo con justificación puntual.
- **§14** Un plugin por sistema, carpeta propia bajo `src/`.
- **§15** Comentarios solo para invariantes/restricciones/workarounds. Nunca el *qué*.
- **§16** ~300 líneas es señal de dividir, no bloqueo.
- **§17** Dependencia nueva en `Cargo.toml` requiere OK humano previo.
- **§18** Sin allocations en el hot path de `FixedUpdate`.
- **§19** Datos separados de sistemas (`state.rs`/`intents.rs` vs `mod.rs`/motores).
- **§20** Simulación nunca depende de visuales; cámara/HUD/interpolación/cues
  viven en `Update` y solo **leen**.
- **§21** **No construir lo que el motor va a dar — ni planear sobre lo que
  todavía no da.** `bevy_scene` 0.19 trae el macro `bsn!` con parches por campo:
  la dirección es migrar hacia ahí, no ampliar andamiaje que lo duplique, y
  `world/layout.rs` es un BSN artesanal (candidato #1). Pero **verificado en las
  fuentes el 2026-07-26: el formato `.bsn` no existe y un `Scene` no se
  serializa**, así que BSN no puede ser un formato de archivo. Nuestro: estados y
  ciclo de vida, contenido procedural, y datos numéricos (heightfield, semántica).

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

**Regla de schedules:** Bevy corre `FixedUpdate` antes que `Update`; todo hardware
que lee simulación se resuelve en `PreUpdate`. Escribirlo en `Update` llega tarde
(hallazgo crítico del audit 2026-07-17).

## Por qué (rationale destilado)

- **Multi-actor por `ActorId` + `Actor` + `Intents`.** Todo cuerpo (player,
  enemigo, horse, futuro remoto) tiene identidad estable; IA y red mueven **solo**
  `Intents`/`CombatIntents` — nunca `Transform`, `BodyVelocity`,
  `LocomotionState` ni estado privado de motores. Por eso agregar animales/NPCs/
  co-op es un Brain nuevo y cero motores. Los motores despachan por **capacidad**
  (`GroundMovement`, `JumpMovement`, …), no por identidad: el horse es un actor
  con otro set de capacidades, no un caso especial.
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
- **Mounts vía ActorLink transaccional.** Mounts pide `Attach`/`Detach`/
  `Neutralize` por mensaje; Movement instala/retira atómicamente attachment,
  redirect de control, collider y gate, y responde con ack. Mounts confirma su
  relación solo desde un ack aceptado. Aplican el mismo tick (workspace
  dimensionado en `PreUpdate`, sin allocation en `FixedUpdate`). Detach sin pose
  segura = collider off + suspensión (`PendingSafeRecovery`).
- **Salud y hostilidad.** Combat/Projectiles/Charge consultan
  `HostileInteractionImmunity` antes de toda consecuencia y emiten
  `DamageRequestMessage`; Health re-valida, aplica y emite `DeathMessage`.
  La reacción a la muerte vive con el dueño del actor (Player respawnea en
  `player/`, enemigos en `enemies/`, targets en `world/`). No existe
  `DamageAppliedMessage`: se diseñará con su primer consumidor real.
- **Percepción por marcador.** Los enemigos perciben actores `Perceivable`
  (marcador de Perception; hoy solo el player). Cuando la hostilidad
  necesite más de un bit (animales, aliados), se reemplaza por facción.
- **Presentación desechable.** Cada actor tiene un visual separado que interpola
  hacia el cuerpo (`VisualOf` lo enlaza para efectos transversales); la simulación
  no porta meshes ni handles, y lo que toca entidades despawneables el mismo frame
  usa comandos tolerantes (`try_insert`). `AppearanceBinding` vive en esa raíz y
  selecciona por clave+slot una receta de `VisualCatalog` — la identidad de
  gameplay jamás es una ruta de asset; Body/MainHand/OffHand/World separan por
  dueño. LOD, culling e instancing solo cambian entidades/recetas visuales: nunca
  el collider ni el estado de simulación. Toda UI que actúa lee en `Update` y
  emite comandos que su dueño valida en `FixedUpdate`; el foco modal es de Input.
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
- **Colisiones independientes del asset (decisión 2026-07-19).** La
  migración separará tres geometrías: cuerpo sólido simple para locomoción,
  hurtboxes sensoras para recibir daño y hitboxes barridas para atacar. El
  mesh renderizado nunca es collider ni autoridad. Hurtboxes/hitboxes viven
  en `FixedUpdate`, enlazan a su `Actor` dueño y usan primitivas/perfiles de
  capacidad fija. Un GLTF puede aportar nodos/socket espaciales que el loader
  convierte a datos puros; simulación nunca lee huesos/`AnimationPlayer` de
  `Update`. Cambiar solo el visual no cambia resultados de simulación.
- **Mundo en tres capas.** `world/mod.rs` (tipos y reglas propias),
  `world/spawn.rs` (mecanismo agnóstico del nivel), `world/layout.rs` +
  submódulos authored como `world/forest.rs` (tablas/diseño y geometría
  derivada). Agrandar el mapa toca solo autoría; esa es la costura donde un
  loader de assets (RON/escena GLTF) se enchufa sin tocar el mecanismo.
- **El costo es una propiedad de la representación, no de la identidad.** Una
  entidad semántica (`TreeKind`, un actor) carga *tiers* de representación en
  `VisualCatalog` — proxy procedural barato, malla detallada, y en el futuro
  impostor/LOD — elegidos por presupuesto, nunca una receta fija. El graybox usa
  el tier barato para no mentir sobre el costo: un placeholder caro que la
  versión final no shipeará invalida toda medición hecha contra él. El watchdog
  de triángulos (`visuals/budget.rs`) hace visible en el log cualquier malla que
  exceda el presupuesto. El baseline usa PBR de Bevy: `StandardMaterial` y el
  único `ExtendedMaterial` del terreno; shaders fullscreen son opt-in.
- **Debug: un snapshot, dos sinks.** Consola y pantalla responden preguntas
  distintas y no pueden contradecirse: el jugador mira el HUD para juzgar
  *feeling*, y el log es lo único que sobrevive al playtest para armar la tabla
  antes/después. Por eso `debug/collect.rs` es el único que convierte valores en
  texto, hacia un `DebugSnapshot` de datos puros (§6, §19); `hud` y `console`
  solo lo acomodan. Las secciones tienen slots fijos, así el orden del reporte
  no depende del orden de los sistemas. La consola emite periódico (serie
  temporal del A/B), por cambio (secciones no volátiles) y a demanda con **P**.
  El *trace* por tick (transiciones, flips, casts) queda aparte: es un flujo de
  eventos, no un estado presente, y un snapshot solo mostraría el último.
- **La instrumentación tiene puntos ciegos declarados.** El total `gpu:` es la
  suma de los spans que Bevy *registra*, no el costo real de GPU. Los pases de
  sombra quedan afuera: Bevy los marca con `info_span!` y no con el grabador de
  diagnostics (`bevy_pbr/render/light.rs`), así que no aportan timestamps. Una
  lectura de "el gpu medido no cambió" no significa "el GPU no es el cuello" —
  ese error ya se cometió una vez y desvió el diagnóstico hacia el prepass
  cuando el costo dominante eran las sombras. Lo no instrumentado se mide por
  A/B (perilla + frame time), nunca por ausencia en la tabla de pases.
- **Suavizado invariante al framerate.** Toda interpolación de presentación usa
  `StableInterpolate::smooth_nudge` (decaimiento exponencial), nunca
  `(rate * dt)` como factor de lerp: esa forma llega a 1.0 a ~20 fps y elimina
  el suavizado justo cuando más se nota, cambiando el comportamiento de cámara
  y visuales entre configuraciones de un mismo A/B. Los `lerp` que quedan
  mezclan por un factor de estado (p. ej. `aim_blend`), no por tiempo.
- **Checkpoint jugado, luego tests** (§10-§11). El loop: implementar →
  `fmt`+`clippy`+`test` → lanzar el juego para el usuario → leer el log antes de
  reportar. Rendimiento exige además escena/build/resolución repetibles y frame
  time antes/después: no se optimiza por intuición ni se acepta porque corre.

## Mapa de módulos (los que existen)

| Módulo | Posee | Frontera |
|---|---|---|
| `input` | `ActiveActions`, `ControlOrientation`, bindings, foco modal | Nadie lee hardware salvo él (ley hoy violada en 13 archivos: C2); resuelve en PreUpdate |
| `scene` | `AppState`, tabla `SCENES`, `SceneBuild`, ciclo de vida | Decide *qué existe y cuándo*; `DespawnOnExit` en todo contenido de escena |
| `editor` | Autoría in-engine: pinceles de relieve, pintura semántica, historial, persistencia | Decide *dónde y cuándo*; el **cómo** cambia el dato es de `world` |
| `asset_pipeline` | Manifiesto build-time, `MaterialPalette`, `SpatialCatalog`, `schema.rs` | Única autoridad espacial de lo authored; SoT compartida con `build.rs` |
| `movement` | `Intents`, `LocomotionState`, motores, facts, attachment/link | Brains escriben Intents; Combat pide por mensaje |
| `proposal` | Núcleo genérico de arbitración | Type-aliases por sistema |
| `combat` | `CombatIntents`, `CombatState`, motores, perfiles montados | Tras Movement; emite constraints/daño por mensaje |
| `projectiles` | Pool fijo de flechas: vuelo/impacto | Simulación sin visuales; Update sincroniza representación |
| `health` | `Health`, inmunidad, aplicación autoritativa de daño | Único que resta HP; muerte por mensaje |
| `inventory` | `Inventory`, equipo/durabilidad, pickups | Equipar inserta/retira `WeaponProfile`; lee `HitImpactMessage` (melee); pide heal a Health |
| `enemies` | Percepción, `Awareness`, brains melee/arquero | Escribe solo sus `*Intents`/`ControlOrientation` |
| `interaction` | Árbitro de `Interact`, `Interactable`, prioridad | Único consumidor de la tecla; emite la decisión por mensaje |
| `mounts` | `Horse`, relación, owner, carga | Todo cambio físico vía ActorLink a Movement |
| `player` | Spawn y respawn del jugador | Dueño de la reacción a su muerte |
| `world` | Geometría, capas, nivel, targets | `TerrainAccess` enruta toda lectura; no lee a nadie |
| `visuals`, `camera`, `presentation`, `sfx` | Presentación + UI | Solo READ; las acciones UI vuelven por mensajes (§20) |
| `debug` | `DebugSnapshot` (datos puros) + trace por tick | Un snapshot, dos sinks: HUD y consola. Nadie más formatea |
| `perf` | Perillas de benchmark, costo GPU por pase | Solo escribe sus perillas; cada dueño las aplica a lo suyo |
| `time_control` | Hitstop y `Time<Virtual>` | Único escritor del reloj virtual de simulación |

Sistemas futuros (crafteo, swim, clima, NPCs, multiplayer, persistencia) se
diseñan al tocar, como consumidores aditivos; borradores en git.
