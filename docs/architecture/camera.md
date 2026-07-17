# Camera

**Carpeta objetivo:** `src/camera.rs` (presentación de cámara, GDD §11
prioridad #2).

Presentación pura (Constitución §20): lee `Transform`/`LocomotionState` de
Movement read-only, nunca escribe hacia la simulación. Vive enteramente en
`Update`, no en `FixedUpdate`.

## Datos (Components/Resources)

| Tipo | Dónde | Qué es |
|---|---|---|
| `CameraRig` | `camera.rs` | `{ current_dip, smoothed_y }` — estado propio de presentación de la cámara orbital. |
| `ControlOrientation` | `input/frame.rs` | `{ yaw, pitch }` del actor local, propiedad de Input/control. Camera solo lo lee. |
| `PointerCaptured` (Resource) | `input/mod.rs` | Input posee la captura del cursor y actualiza la orientación local. |

## Sistemas (comportamiento)

Cadena en `Update` (`camera_landing_dip`, `follow_local_actor`), ordenada
después de `InputSet::UpdateOrientation`:

1. **camera_landing_dip** — detecta transición `Fall → Walk/Sprint` en
   `LocomotionState` y agrega un dip vertical temporal (lee Movement,
   read-only).
2. **follow_local_actor** — sigue al actor controlado localmente con Y
   suavizado, spring-arm que se acerca si un `ShapeCast` (Avian
   `SpatialQuery`) detecta un choque contra geometría del mundo.

## Relaciones con otros sistemas

| Relación | Categoría | Mecanismo |
|---|---|---|
| Camera lee `Transform`/`LocomotionState` del actor local | READ | Query read-only, nunca escribe `Intents` ni estado de Movement |
| Camera lee `ControlOrientation` de Input | READ | Input/control posee yaw/pitch; Camera no es fuente de simulación |
| Camera lee geometría del mundo vía Avian `SpatialQuery` | READ | Solo para el spring-arm, no simula física propia |

Camera nunca aparece como emisor de mensajes de simulación — es hoja del
grafo, igual que UI.

## Multi-actor

En multiplayer, cada cliente sigue **solo su propio actor local**, no a los
`RemoteActor` — no es una cámara compartida ni split-screen. Migrar a
`Query<.., With<Actor>>` no aplica aquí de la misma forma que en Movement: no
hay múltiples cámaras locales que necesiten tick concurrente, solo hace falta
filtrar por "actor controlado localmente" — ver
`rationale/multi-actor-dispatch.md` para el contrato general.

## Decisiones abiertas

- **Modo apuntado — primera pasada implementada** (ticket `combat-bow`,
  2026-07-15): Camera lee `CombatState::Aiming` del player (READ Combate →
  Camera, ahora real) y hace blend (`CameraRig.aim_blend`) a boom corto +
  offset sobre el hombro derecho + mira central (UI). La geometría del
  pivot del hombro (`AIM_PIVOT_HEIGHT`/`AIM_SHOULDER_OFFSET`) es dueña de
  Combat (`combat/motors/aim.rs`) y Camera la **importa**: el rayo de la
  mira y el rayo de puntería son la misma línea sin que la simulación lea
  la cámara (§20). El disparo usa el estándar de dos fases (2026-07-17):
  un raycast desde el pivot resuelve el punto del crosshair y la flecha
  sale del socket del arco (`BOW_SOCKET_LOCAL`, compartido con el visual)
  convergiendo hacia ese punto. Fallbacks a la línea de mira: a quemarropa,
  y cuando cualquier obstáculo bloquea la línea del arco que el crosshair
  ya despejó ("si lo veo, puedo dispararle") — la vista del jugador gana
  sobre el realismo estricto del muzzle. La caída parabólica sigue
  aplicando después del lanzamiento (tiros débiles caen bajo la línea de
  mira — mecánica de carga, por diseño). Sigue abierto: ¿corte
  a primera persona en vez de over-shoulder? ¿tiempo ralentizado al tensar?
  (`Time::relative_speed` es recurso global compartido — el hitstop de
  `presentation::juice` ya lo usa puntualmente; un slow-mo sostenido
  requiere contrato propio.)
- **Lock-on (GDD §11):** selección de objetivo enemigo, cámara orbita
  centrada en él. Requiere leer posiciones de `Enemies` (READ) y decidir
  criterio de selección (más cercano, en el cono de visión, etc.) — sin
  diseñar todavía.
- Colisión de spring-arm: forma exacta del cast y comportamiento en espacios
  muy angostos (colisiona con el propio actor o geometría cóncava).
