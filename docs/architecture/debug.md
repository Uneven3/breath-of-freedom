# Debug

**Carpeta objetivo:** `src/debug.rs` (ya existe con un HUD básico, ver
Decisiones abiertas para su migración)

Observabilidad en dos capas: logs estructurados (post-mortem, terminal) y
overlay en vivo dentro del propio juego (tiempo real, sin salir a un
debugger externo). Presentación pura (Constitución §20) salvo los comandos
de debug explícitamente marcados como excepción — ver Relaciones.

## Logs estructurados

Bevy ya trae `tracing`/`bevy_log` vía `LogPlugin` (parte de `DefaultPlugins`,
sin dependencia nueva — Constitución §17 no aplica). Convención:

- Cada sistema usa `bevy::log::{trace, debug, info, warn, error}` con
  `target` explícito igual al nombre del sistema (ej.
  `debug!(target: "combat", ?state, "transition")`), nunca `println!`.
  Filtrable con `RUST_LOG=combat=debug,movement=trace` en runtime, sin
  recompilar.
- Niveles: `trace` = por-frame (arbitración, motor activo — ruidoso,
  apagado por defecto); `debug` = transiciones de estado y decisiones
  discretas (`LocomotionState` cambia, `DamageRequestMessage` llega);
  `info` = eventos de ciclo de vida (sesión host/client iniciada, jugador
  conectado); `warn` = condición recuperable rara (buffer casi lleno,
  frame de red descartado por viejo); `error` = reservado para lo que ya
  se decidió loguear antes de un panic de bug real (Constitución §9), no
  para condiciones esperables del juego.
- El placeholder de audio (`sfx.md`: `log::debug!("[audio] cue: {id}")`) es
  un caso particular de esta convención, no un mecanismo aparte.

## Overlay en vivo (dentro del juego)

| Tipo | Dónde | Qué es |
|---|---|---|
| `DebugOverlay` (Resource) | `debug.rs` | `{ visible: bool }`. Toggle con F3. Nunca gatea si el juego corre, solo si se dibuja. |
| `DebugInspectTarget` (Resource) | `debug.rs` | `Option<Entity>` del `Actor` bajo inspección. Default: el actor local. Cicla con una tecla entre `Actor`s vivos — clave para depurar `EnemyAiState`/`RemoteActor` sin ser ese jugador. |

## Sistemas (comportamiento) — propuesta

- `toggle_debug_overlay` — `Update`, F3 invierte `DebugOverlay.visible`.
- `cycle_inspect_target` — `Update`, tecla dedicada cicla `DebugInspectTarget`
  entre entidades `With<Actor>` vivas.
- `update_debug_text` (ya existe, migra de `Single<.., With<Player>>` a leer
  `DebugInspectTarget` — mismo prerequisito multi-actor que el resto,
  `rationale/multi-actor-dispatch.md`) — muestra `LocomotionState`,
  `CombatState`, `Health`, `Stamina`, velocidad, grounded del actor
  inspeccionado, más `NetworkRole` e `input::InputScheme`/`ActiveActions`
  globales (no dependen del target).
- Todo corre en `Update`, lee componentes/resources de todos los sistemas
  read-only — no necesita `CueMessage` ni ningún bus, es lectura directa
  igual que Camera/UI (Constitución §20).

## Relaciones con otros sistemas

| Relación | Categoría | Mecanismo |
|---|---|---|
| Debug lee cualquier `Component`/`Resource` público de cualquier sistema | READ | Mismo trato que UI/Camera — nunca escribe simulación |
| Debug lee `NetworkRole`, latencia de replicación | READ | Útil específicamente para depurar el modelo host-autoritativo |
| Debug lee `input::InputScheme`/`ActiveActions` | READ | Ver en vivo qué `IntentAction` están activas ahora mismo — imprescindible mientras se ajusta el default de `KeyboardOnly` a mano (`input.md`) |
| **Excepción explícita:** comandos de debug (god mode, noclip, teletransportar, forzar clima) | WRITE, marcado | Único lugar del proyecto donde presentación puede escribir simulación — cada comando debe loguearse (`warn!(target: "debug", ...)`) y quedar detrás de un gate que no compile en build de release (ver Decisiones abiertas) |

## Decisiones abiertas

- ¿Overlay/comandos siempre compilados detrás de F3, o detrás de
  `#[cfg(debug_assertions)]`/feature flag para que un build de release no
  los incluya en absoluto?
- ¿Consola de comandos de texto (requiere un crate de UI de texto — crate
  nuevo, Constitución §17, pedir aprobación explícita) o solo hotkeys
  discretas (más simple, ya alcanza para god-mode/noclip/teleport)?
- `bevy_inspector_egui` o herramienta de inspección visual equivalente —
  **no agregar sin aprobación explícita** (Constitución §17); el overlay de
  arriba cubre el caso de uso mínimo sin dependencias nuevas.
- Métricas de rendimiento (FPS, conteo de entidades): `bevy::diagnostic` ya
  viene con Bevy sin costo de dependencia nueva — falta decidir si se
  agrega al mismo overlay o uno separado.
- Grabación/replay de una sesión para reproducir un bug — no evaluado.
