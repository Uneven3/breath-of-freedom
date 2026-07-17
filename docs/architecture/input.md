# Input

**Carpeta:** `src/input/`
**Estado:** snapshot local implementado; rebinding/gamepad/red pendientes.

Input muestrea hardware una vez por render frame (`PreUpdate`) y publica
acciones resueltas por `InputSource`. Los Brains leen `ActiveActions`; ningún
Brain de Movement, Combat o Mounts conoce teclas concretas.

## Datos

| Tipo | Dónde | Función |
|---|---|---|
| `IntentAction` | `input/action.rs` | 18 acciones actuales: movimiento/look, Jump, Sprint, Sneak, ClimbToggle, Mantle, Vault, Glide, Attack, Aim e Interact. Parry no existe aún. |
| `ActiveActions` / `ActionFrame` | `input/frame.rs` | Slots fijos por fuente: sostenidos + generaciones de triggers. |
| `InputControlledBy(InputSource)` | `input/frame.rs` | Enlace actor–fuente. |
| `InputConsumeCursor` | `input/frame.rs` | Cursor por consumidor; evita doble consumo entre fixed ticks. |
| `ControlOrientation` | `input/frame.rs` | Yaw/pitch semánticos usados por Movement, Combat y Camera. |

Movement, Combat y Mounts tienen cursores separados en el mismo Player. Por
eso E puede compartir `IntentAction::Interact` sin consumir triggers de salto
o ataque pertenecientes a otro dominio.

## Mapping local actual

- WASD: movimiento; IJKL: look.
- Space: Jump/Glide; Shift: Sprint; Ctrl: Sneak.
- 1/2/3: ClimbToggle/Mantle/Vault.
- Mouse izquierdo o F: Attack; mouse derecho o Q: Aim.
- E: Interact (Mounts).
- F8 no es gameplay rebindeable: Debug captura únicamente un request de
  toggle; la simulación de Mounts lo procesa en `FixedUpdate`.

## Pendiente

- `Keybindings`, chords, UI/persistencia de rebinding y gamepad.
- `ApplyRemoteActionsMessage` y validación de frames de red.
- Agregar Guard/Parry solo junto al motor Combat que realmente los consuma.
