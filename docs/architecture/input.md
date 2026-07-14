# Input

**Carpeta objetivo:** `src/input.rs` (o `src/input/` si crece)

Motor de *keybinding* genérico y rebindeable — el jugador decide qué combo
de teclas dispara qué acción (estilo WoW: `1` o `Alt+1` para atacar, a
elección). El *feeling* de qué mapa por defecto se siente bien solo se
valida jugando; lo que la arquitectura debe garantizar es que **ningún**
Brain de gameplay tenga una tecla concreta hardcodeada, para que cualquier
remapeo sea posible sin tocar Movement/Combat/NPCs/Camera.

## Estado de implementación

El núcleo local está implementado en `src/input/`: `IntentAction`,
`InputSource`, `InputControlledBy`, `ActiveActions`, `ActionFrame`,
`InputConsumeCursor` y `ControlOrientation`. `resolve_local_actions` corre en
`PreUpdate`, publica el mapping local actual y preserva generaciones de
gatillos para `FixedUpdate`. Movement ya consume ese contrato; Camera solo lee
`ControlOrientation`.

Quedan pendientes los chords configurables, gamepad, persistencia/UI de
rebinding y la aplicación de frames remotos. Esos añadidos deben extender el
mismo snapshot, nunca volver a introducir hardware crudo en gameplay.

## Datos (Components/Messages/Resources)

| Tipo | Dónde | Qué es |
|---|---|---|
| `InputScheme` | `input.rs` (Resource) | Pendiente: `Gamepad`, `KeyboardOnly`, `KeyboardMouse`. Seleccionará qué tabla de `Keybindings` por defecto se carga y qué sistemas de hardware corren. |
| `IntentAction` | `input/action.rs` | Enum plano de **todas** las acciones discretas rebindeables de todos los dominios: `MoveForward/Back/Left/Right`, `LookUp/Down/Left/Right`, `Jump`, `Sprint`, `Sneak`, `ClimbToggle`, `Mantle`, `Vault`, `Glide`, `Attack`, `Parry`, `Aim`, `Interact`. Input es dueño de este enum — es la única "forma compartida" que expone, sin conocer *por qué* cada dominio la usa. |
| `HardwareTrigger` | `input/binding.rs` | Entrada física abstracta: `Key(KeyCode)`, `Mouse(MouseButton)`, `GamepadButton(GamepadButton)`, etc. `KeyCode` no se filtra a los sistemas de gameplay. |
| `InputChord` | `input/binding.rs` | `{ modifiers: [HardwareTrigger; MAX_MODIFIERS], trigger: HardwareTrigger }`. `{}` de modifiers + trigger = tecla/botón solo; `{Period, Space}` + `KeyS` = el combo de 3 teclas que uno de nuestros ejemplos usa para Attack. |
| `Keybindings` | `input.rs` (Resource) | Pendiente: tabla rebindeable `(InputChord → [IntentAction; MAX_ACTIONS_PER_CHORD])` de capacidad fija. El resolver local actual conserva los defaults de teclado como una tabla interna fija. |
| `InputSource` | `input/source.rs` | Identifica una fuente local de input (`LocalPlayer`, `LocalDebug`, etc.). Permite que un snapshot de acciones tenga dueño explícito y evita asumir un único actor global. |
| `InputControlledBy(InputSource)` | `input/source.rs` | Componente en el actor local controlado por una fuente de input. Movement/Combat/NPCs usan este enlace para elegir qué slice de `ActiveActions` leer. |
| `ActiveActions` | `input.rs` (Resource) | Snapshot resuelto por `InputSource`, almacenado en slots de capacidad fija: bitset de acciones sostenidas + contador/generación por acción disparada. Los Brains lo leen inmutablemente — nunca `ButtonInput<KeyCode>` directamente. |
| `ActionFrame` | `input/frame.rs` | Snapshot serializable de acciones resueltas para una fuente: `{ frame_seq, sustained_bitset, trigger_generations }`. Input lo produce para red y tests de contrato; no contiene hardware crudo ni `Keybindings`. |
| `InputConsumeCursor` | `input/cursor.rs` | Componente/cursor por consumidor (`Movement`, `Combat`, `NPCs`, etc.) que guarda la última generación consumida por acción. Consumir un trigger muta el cursor del dueño, no el `ActiveActions` global. |
| `RebindRequestMessage` / `RebindResultMessage` | `input/messages.rs` | Pendiente: UI pedirá cambiar un binding; Input validará y será su único writer. |
| `ApplyRemoteActionsMessage` | `input/messages.rs` | Pendiente: Multiplayer entregará un `ActionFrame` a Input, que validará fuente/secuencia antes de escribir su slot. |

## Sistemas (comportamiento) — propuesta

- `resolve_bindings` — Corre en `PreUpdate` (justo después del sistema interno de entrada de Bevy, ejecutándose exactamente una vez por frame de render). Primero calcula el chord ganador por cada `trigger` presionado: entre los `InputChord` cuyo `trigger` coincide y cuyos `modifiers` están **todos** presionados, gana el de **más modifiers** (más específico). Después marca como consumidos los `HardwareTrigger` usados como modifier por esos ganadores y descarta cualquier ganador cuyo propio `trigger` haya sido consumido como modifier de otro ganador. Los ganadores restantes activan sus listas fijas de `IntentAction`s y escriben en `ActiveActions` por `InputSource`: estados sostenidos y generaciones de gatillos. Los bindings ambiguos se rechazan al grabar el bind, no en runtime. Esta segunda fase evita que `Period+Space+S` dispare `Attack` **y** el binding de `Space` solo (`Jump`+`Glide`) a la vez — ver `rationale/data-driven-keybindings.md` § Supresión de modifiers.
- Movement/Combat/NPCs arman sus propios contratos semanticos a partir de `ActiveActions` en `FixedUpdate` (ej. Movement produce `PlanarMoveIntent`, `JumpIntent` y `GlideIntent`). Para gatillos discretos, cada Brain compara la generación de la acción con su propio `InputConsumeCursor`; si es nueva, la procesa y actualiza su cursor. Si `FixedUpdate` corre múltiples veces en un frame de render, el cursor evita double trigger; si no corre, la generación queda disponible para el siguiente tick.
- Camera lee `LookUp/Down/Left/Right` de `ActiveActions` en `Update` bajo `KeyboardOnly` para sintetizar `LookAxis`, o el delta de mouse directamente bajo `KeyboardMouse` (los deltas son continuos y no pasan por la tabla `Keybindings`).
- `apply_rebind_request` — `MessageReader<RebindRequestMessage>`; Input valida que el chord no cree conflicto ambiguo, que haya capacidad en la tabla y que el hardware pertenezca al esquema activo. Luego muta `Keybindings` y emite `RebindResultMessage`. UI nunca escribe `Keybindings` directamente.
- `apply_remote_actions` — `MessageReader<ApplyRemoteActionsMessage>`; Input valida que el `InputSource` exista, que acepte frames remotos y que `frame.frame_seq` sea más nuevo que el último aplicado para esa fuente. Frames duplicados/viejos se descartan sin panic. Luego copia el frame recibido a su slot de `ActiveActions`. Multiplayer no escribe `ActiveActions` directamente.

## Default de `KeyboardOnly` (rebindeable, no hardcodeado)

El diseño de mano-libre-por-lado (`.+Space` sostenidos
como modifiers de un combo, WASD como triggers) se conserva **como el
binding por defecto que se carga al elegir `KeyboardOnly`**, no como lógica
propia de ningún Brain:

| Combo | `IntentAction` |
|---|---|
| `W`/`A`/`S`/`D` (sin modifiers) | `MoveForward`/`Left`/`Back`/`Right` |
| `I`/`J`/`K`/`L` (sin modifiers) | `LookUp`/`Left`/`Down`/`Right` |
| `Space` | `Jump`, `Glide` |
| `Shift` | `Sprint` |
| `Ctrl` | `Sneak` |
| `1` | `ClimbToggle` |
| `2` | `Mantle` |
| `3` | `Vault` |
| `Period+Space+S` | `Attack` |
| `Period+Space+A` | `Parry` |
| `Period+Space+D` | `Aim` |
| `Period+Space+W` | `Interact` |

Razón ergonómica del combo de 3 teclas (mano derecha entera libre para
seguir apuntando la cámara con I/J/K mientras se ataca): ver
`rationale/keyboard-only-action-layer.md`, ahora reencuadrado como el
*default*, no como mecanismo especial — ver
`rationale/data-driven-keybindings.md`.

## Relaciones con otros sistemas

| Relación | Categoría | Mecanismo |
|---|---|---|
| Movement, Combat, NPCs leen `ActiveActions` para llenar su propio `Intents`/`CombatIntents`/`InteractIntents` | READ | Ninguno lee `ButtonInput<KeyCode>` ni conoce `InputChord`/`InputScheme` — dependen solo de `IntentAction`, la forma más genérica posible (Constitución §5). Cada uno consume gatillos mutando su propio `InputConsumeCursor` |
| Camera lee `InputScheme` + `ActiveActions` (look) para elegir su fuente de `LookAxis` | READ | Presentación, Constitución §20 |
| UI lee `Keybindings` para el menú de reasignación y emite pedidos de rebind | READ + MESSAGE | UI muestra la tabla y emite `RebindRequestMessage`; Input valida y escribe `Keybindings` |
| Persistence serializa `Keybindings` (preferencia del jugador, no estado de partida) | WRITE-OWN (por Input dentro de `PersistenceSet`) | Ver `persistence.md` |
| Multiplayer: `Keybindings`/`InputScheme` son puramente locales al cliente | ninguna | Nunca se replican — la preferencia de bindeo de cada jugador no le importa al host |
| Multiplayer transmite `ActionFrame` ya resuelto (no `Intents`/`CombatIntents`) | READ + MESSAGE + SHARED-CONTRACT | `LocalInputFrame` empaqueta un `ActionFrame` local; en el host, Multiplayer emite `ApplyRemoteActionsMessage` e Input escribe el slot de `ActiveActions` de ese `InputSource` de red. Los mismos Brains de Movement/Combat/NPCs lo traducen — sin `NetworkBrain` de traducción separado (`multiplayer.md`) |

## Decisiones abiertas

- Formato de `Keybindings` en disco (RON, igual que `ItemId`/`Recipe`) y
  UI de reasignación (menú "press a key to bind").
- Esquema `Gamepad` concreto: qué botones/ejes analógicos cubre
  `HardwareTrigger` y qué acciones tienen valor analógico además de bit
  sostenido.
- Esquema `KeyboardMouse`: qué combos por defecto van a `Attack`/`Parry`/
  `Aim`/`Interact` (candidatos obvios: clicks + una tecla).
- Umbral de "sostenido simultáneamente" cuando el input llega en frames
  distintos por timing de teclado (debounce de 1-2 frames a 60Hz).
