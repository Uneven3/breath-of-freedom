# Camera

**Carpeta objetivo:** `src/camera.rs` (presentación de cámara, GDD §11
prioridad #2).

Presentación pura (Constitución §20): lee `Transform`/`LocomotionState` de
Movement read-only, nunca escribe hacia la simulación. Vive enteramente en
`Update`, no en `FixedUpdate`.

## Datos (Components/Resources)

| Tipo | Dónde | Qué es |
|---|---|---|
| `CameraRig` | `camera.rs` | `{ yaw, pitch, current_dip, smoothed_y }` — estado propio de presentación de la cámara orbital. |
| `MouseCaptured` (Resource) | `camera.rs` | Si el cursor del SO está capturado; gatea `mouse_look`. |

## Sistemas (comportamiento)

Cadena objetivo en `Update` (`cursor_control`, `mouse_look`/`keyboard_look`,
`camera_landing_dip`, `follow_local_actor`):

1. **cursor_control** — ESC libera el cursor (segundo ESC sale de la app),
   click izquierdo lo recaptura. Solo aplica bajo `InputScheme::KeyboardMouse`
   (`input.md`) — sin mouse que capturar, no corre bajo `KeyboardOnly`.
2. **mouse_look** (`run_if(KeyboardMouse)`) — acumula `AccumulatedMouseMotion`
   en yaw/pitch del rig. **`keyboard_look`** (`run_if(KeyboardOnly)`) — lee
   `input::ActiveActions` del `InputSource` local para
   `LookUp/Down/Left/Right` (no `KeyCode` directo, ver `input.md`) y gira
   yaw/pitch a velocidad angular constante mientras cada acción está activa,
   mismo clamp de pitch. Ambos son intercambiables según `input::InputScheme`;
   Camera tampoco sabe qué entrada física dispara cada
   `LookUp/Down/Left/Right`, solo lee la acción ya resuelta.
3. **camera_landing_dip** — detecta transición `Fall → Walk/Sprint` en
   `LocomotionState` y agrega un dip vertical temporal (lee Movement,
   read-only).
4. **follow_local_actor** — sigue al actor controlado localmente con Y
   suavizado, spring-arm que se acerca si un `ShapeCast` (Avian
   `SpatialQuery`) detecta un choque contra geometría del mundo.

## Relaciones con otros sistemas

| Relación | Categoría | Mecanismo |
|---|---|---|
| Camera lee `Transform`/`LocomotionState` del actor local | READ | Query read-only, nunca escribe `Intents` ni estado de Movement |
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

- **Modo apuntado (GDD §8, §11):** cámara en primera persona + tiempo
  ralentizado al tensar el arco. Disparador: Combate entra en
  `CombatState::Aiming` (`combat.md`) — Camera necesita leerlo (READ nuevo,
  Combate → Camera) para decidir el corte a primera persona; el ralentizado
  de tiempo (`Time::relative_speed`) es un recurso global compartido, no
  exclusivo de Camera; requiere contrato propio si otro sistema también lo
  necesita.
- **Lock-on (GDD §11):** selección de objetivo enemigo, cámara orbita
  centrada en él. Requiere leer posiciones de `Enemies` (READ) y decidir
  criterio de selección (más cercano, en el cono de visión, etc.) — sin
  diseñar todavía.
- Colisión de spring-arm: forma exacta del cast y comportamiento en espacios
  muy angostos (colisiona con el propio actor o geometría cóncava).
