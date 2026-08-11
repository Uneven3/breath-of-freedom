# Arquitectura y rationale

El código documenta lo hecho; este archivo (≤200 líneas) fija las **leyes** y el
**por qué**. El detalle vive en módulos y tests; historial: `git log -- docs/`.

## Leyes (la Constitución — el código las cita por §)

Código que viole estas leyes no se implementa ni mergea.

- **§1** Responsabilidad única por plugin/sistema/componente.
- **§2** Extender agregando sistemas/componentes, no editando lógica ajena.
- **§3** Un trait implementado honra lo que el trait promete.
- **§4** APIs públicas mínimas: lo `pub` es lo que cruza la frontera de su
  crate. Dentro, `pub(crate)` — Cargo lo cobra.
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
- **§14** Un plugin por sistema, carpeta propia en su crate.
- **§15** Comentarios sólo para invariantes/restricciones/workarounds, nunca el
  *qué*. Techo testeado: 30% por archivo, contando `//`, `///` y `//!`; ningún
  bloque más largo que su ítem. El rationale largo va a `docs/`.
- **§16** ~300 líneas es señal de dividir, no bloqueo.
- **§17** Dependencia nueva en `Cargo.toml` requiere OK humano previo.
- **§18** Sin allocations en el hot path de `FixedUpdate`.
- **§19** Datos separados de sistemas: el dato compartido vive en `bof_domain`,
  que no declara `bevy` ni Avian.
- **§20** Simulación nunca depende de visuales: `bof_simulation` no declara
  `bevy_render`, `bevy_input` ni `bevy_window`. Cámara/HUD/interpolación/cues
  viven en `Update` y solo **leen**.
- **§21** **No construir lo que el motor va a dar — ni planear sobre lo que
  todavía no da.** `bsn!` es la dirección para componer entidades; el formato
  `.bsn` no existe y un `Scene` no se serializa (verificado en las fuentes de
  0.19), así que no puede ser el archivo de nivel. Detalle en `MAP_EDITOR.md`.

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
            EmitConstraints → Projectiles::Simulate → Health::Apply, y el
            desgaste de armas después de los tres emisores de impacto
Update      Presentación: visuals, camera, HUD/debug, juice, sfx — solo READ
```

**Regla de schedules:** Bevy corre `FixedUpdate` antes que `Update`; todo
hardware que lee simulación se resuelve en `PreUpdate` — en `Update` llega tarde
(audit 2026-07-17). Dos sistemas que tocan el mismo dato sin orden declarado
dejan la decisión al ejecutor: `scheduling_audit` congela cuántos pares así
quedan y sólo permite que bajen.

## Por qué (rationale destilado)

- **Multi-actor por `ActorId` + `Actor` + `Intents`.** Todo cuerpo tiene
  identidad estable; IA y red mueven **sólo** `Intents`/`CombatIntents`, nunca
  `Transform`, `BodyVelocity`, `LocomotionState` ni estado privado de motores.
  Por eso agregar animales, NPCs o co-op es un Brain nuevo y cero motores. Los
  motores despachan por **capacidad**, no por identidad: el horse es un actor
  con otro set, no un caso especial — y cada capacidad exige su estado por
  `#[require]`, así que no se puede spawnear a medias.
- **Árbitro central por sistema.** Motores *proponen* a un `ProposalBuffer` de
  capacidad fija (`bof_domain::proposal`, prioridad → peso); un solo sistema
  arbitra y es el único escritor de `LocomotionState`/`CombatState`. Los trece
  `propose` corren en paralelo y **el orden no puede cambiar el ganador**:
  `arbitration_matrix` prohíbe los empates de `(Priority, weight)` entre motores
  que co-proponen, con su lista de excepciones mutuamente excluyentes.
- **El receptor posee el contrato.** El mensaje lo define quien lo consume
  (`LocomotionConstraintMessage` es de Movement aunque lo emita Combat). Las
  restricciones expiran por silencio: el emisor re-emite mientras la condición
  dure. `ForbidSprint` llega 1 tick tarde por el orden Movement→Combat —
  aceptado y fijado con test.
- **Mounts vía ActorLink transaccional.** Mounts pide `Attach`/`Detach`/
  `Neutralize` por mensaje; Movement instala o retira atómicamente attachment,
  redirect, collider y gate, y responde con ack — Mounts confirma su relación
  sólo desde un ack aceptado. Mismo tick, sin allocation en `FixedUpdate`
  (workspace dimensionado en `PreUpdate`). Detach sin pose segura = collider off
  y suspensión.
- **Salud y hostilidad.** Combat/Projectiles/Charge consultan
  `HostileInteractionImmunity` y emiten `DamageRequestMessage`; Health
  re-valida, aplica y emite `DeathMessage`. La reacción a la muerte vive con el
  dueño del actor. No existe `DamageAppliedMessage`: se diseñará con su primer
  consumidor. Los enemigos perciben actores `Perceivable`; con más de un bit de
  hostilidad, ese marcador pasa a ser facción.
- **Presentación desechable.** Cada actor tiene un visual separado que interpola
  hacia el cuerpo (`VisualOf` los enlaza); la simulación no porta meshes ni
  handles. `AppearanceBinding` selecciona una receta de `VisualCatalog` por
  clave+slot: la identidad de gameplay jamás es una ruta de asset. LOD, culling
  e instancing sólo cambian entidades visuales, nunca el collider ni el estado.
  Toda UI que actúa lee en `Update` y emite comandos que su dueño valida en
  `FixedUpdate`.
- **Combate apuntado en dos fases.** El rayo del crosshair nace del pivote a
  altura de ojos y resuelve el target; la flecha sale del socket del arco
  convergiendo. Ambas constantes viven en domain porque la cámara **tiene** que
  alinearse con el origen del proyectil. Fallback a la línea de mira a
  quemarropa y cuando un obstáculo tapa la línea del arco: "si lo veo, puedo
  dispararle".
- **Capas de física.** `GameLayer::{Default, Actor}`: los contactos cruzan
  capas; compran *sensing selectivo* (ledges enmascara `Default` para no trepar
  cápsulas ajenas; espada y flechas seleccionan `Actor`).
- **Colisiones independientes del asset (2026-07-19).** Cuerpo sólido para
  locomoción, hurtboxes sensoras y hitboxes barridas, separados. El mesh nunca
  es collider ni autoridad. Contrato completo en `AHORA.md`.
- **Mundo en tres capas.** Dato (heightfield y semántica, en simulación),
  mecanismo agnóstico del nivel (`world/spawn.rs`) y autoría (`layout`,
  `forest`). Agrandar el mapa toca sólo autoría, que es donde un loader de
  assets se enchufa sin tocar el mecanismo.
- **El costo es propiedad de la representación, no de la identidad.** Una
  entidad semántica (`TreeKind`, un actor) carga *tiers* en `VisualCatalog`
  —proxy barato, malla detallada, impostor futuro— elegidos por presupuesto. El
  graybox usa el tier barato para no mentir sobre el costo: un placeholder caro
  que no shipea invalida toda medición hecha contra él. `visuals/budget.rs`
  avisa por log de cualquier malla pasada, y `perf/budget.rs` suma lo que cada
  escena *declara*, que es lo que el contador de runtime no puede ser. Baseline
  PBR de Bevy; shaders fullscreen son opt-in.
- **Debug: un snapshot, dos sinks.** El HUD sirve para juzgar *feeling* y el log
  es lo único que sobrevive al playtest, y no pueden contradecirse: sólo
  `debug/collect.rs` convierte valores en texto, hacia un `DebugSnapshot` de
  datos puros (§6, §19). El *trace* por tick va aparte: es un flujo de eventos,
  no un estado presente.
- **La instrumentación tiene puntos ciegos declarados.** El total `gpu:` suma
  los spans que Bevy *registra*, no el costo real: las sombras usan
  `info_span!` y no aportan timestamps. "El gpu medido no cambió" **no**
  significa "el GPU no es el cuello" — ese error ya desvió un diagnóstico hacia
  el prepass cuando el costo eran las sombras. Lo no instrumentado se mide por
  A/B (perilla + frame time), nunca por ausencia en la tabla.
- **Un test que afirma ausencia necesita un canario.** "Nadie lee hardware", "no
  hay ambigüedades": esos tests dan la misma salida verde si la ley se cumple
  que si el detector está ciego, y las dos veces que se midieron ambigüedades
  sin canario el número era falso. Cada regla de `tests/architecture.rs` tiene
  el suyo, y el canario debe ser **diferencial** cuando ya hay un baseline.
- **Suavizado invariante al framerate.** Presentación interpola con
  `StableInterpolate::smooth_nudge`, nunca con `(rate * dt)` como factor: esa
  forma llega a 1.0 a ~20 fps y borra el suavizado justo cuando más se nota,
  cambiando el comportamiento entre configuraciones de un mismo A/B. Los `lerp`
  que quedan mezclan por estado, no por tiempo.
- **Checkpoint jugado, luego tests** (§10-§11): implementar →
  `fmt`+`clippy`+`test` → lanzar el juego → leer el log antes de reportar.
  Rendimiento exige además escena/build repetibles y frame time antes/después.

## Las tres capas, y quién ve a quién

```text
breath-of-freedom (bin)   composición: main.rs, scene, world::layout/spawn, input
   ├── bof_simulation     gameplay autoritativo, sin ventana ni render
   └── bof_domain         datos puros: tipos, unidades, Intents, estados, facts
```

**Hermanas, no una pila** (decisión del usuario, 2026-08-03): presentación vive
en el binario y sólo puede nombrar `bof_domain`, que es dato puro, así que
**leer es lo único que puede hacer**. El binario es la única capa que ve las
dos, y por eso el único lugar donde armar collider **y** malla en la misma
función es legal (`world::spawn`).

La frontera la cobra Cargo —`bof_domain` sin `bevy` ni Avian, `bof_simulation`
sin `bevy_render`/`bevy_input`/`bevy_window`— y lo que Cargo no alcanza está en
`tests/architecture.rs`: presentación no nombra `bof_simulation`, C2, `unsafe` y
el registro único de plugins.

Cuando simulación quiere saber el estado de la app, la pregunta correcta es qué
mensaje debería recibir: terreno, player, enemigos, caballo y reloj declaran su
vida con `SceneScoped` y `scene` decide cuándo nacen.

## Mapa de módulos (contratos reexportados desde `bof_domain`)

| Módulo | Posee | Frontera |
|---|---|---|
| `input` (app) | bindings, foco modal, cursor | Nadie lee hardware salvo él (deuda C2: 12 archivos, congelada por `tests/architecture.rs`); simulación no *puede*: no declara `bevy_input` |
| `scene` (app) | `AppState`, tabla `SCENES`, `SceneBuild`, ciclo de vida | Decide *qué existe y cuándo*; simulación declara `SceneScoped` y esto lo bindea a `DespawnOnExit` |
| `editor` | Autoría in-engine: pinceles de relieve, pintura semántica, historial, persistencia | Decide *dónde y cuándo*; el **cómo** cambia el dato es de `world` |
| `asset_pipeline` | Manifiesto build-time, `MaterialPalette`, `SpatialCatalog`, `schema.rs` | Única autoridad espacial de lo authored; SoT compartida con `build.rs` |
| `inventory` | `Inventory`, equipo/durabilidad, pickups | Equipar inserta/retira `WeaponProfile`; lee `HitImpactMessage` (melee); pide heal a Health |
| `world` | Heightfield, semántica por celda, marcadores authored, reloj | `TerrainAccess` enruta toda lectura; `layout`/`spawn` se quedan en el binario porque arman collider **y** malla |
| `visuals`, `camera`, `presentation`, `sfx` | Presentación + UI | Solo READ; las acciones UI vuelven por mensajes (§20) |
| `debug` | `DebugSnapshot` (datos puros) + trace por tick | Un snapshot, dos sinks: HUD y consola. Nadie más formatea |
| `perf` | Perillas de benchmark, costo GPU por pase | Solo escribe sus perillas; cada dueño las aplica a lo suyo |

Lo no listado sigue la regla general: posee su dato, lo publica por mensaje y
nadie más lo escribe. Sistemas futuros se diseñan al tocar, como consumidores
aditivos.
