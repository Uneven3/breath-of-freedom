# Multiplayer

**Carpeta objetivo:** `src/net/`

Modelo decidido: **host-autoritativo, sin servidor dedicado** — un cliente
hostea su propia instancia y otros se conectan (estilo LAN/Hamachi, no
MMORPG). Rationale completo, incluyendo por qué no P2P ni rollback, en
`rationale/multiplayer-model.md`.

## Datos (Components/Messages/Resources) — propuesta

| Tipo | Dónde | Qué es |
|---|---|---|
| `NetworkRole` | `net/mod.rs` (Resource) | `Host` o `Client`. Decide qué sets de red/simulación corren: el cliente captura input y aplica snapshots; el host aplica input resuelto a un `InputSource` remoto y simula. |
| `RemoteActor` | `net/mod.rs` | Marker sobre un `Actor` (ver `rationale/multi-actor-dispatch.md`) controlado por un `InputSource` de red, no por hardware local. |
| `LocalInputFrame` | `net/input.rs` | Wire payload que contiene `input::ActionFrame` **ya resuelto localmente** (no `ButtonInput<KeyCode>` crudo) con `frame_seq` monotónico para rechazar duplicados/reordenamiento. La resolución de `Keybindings` es una preferencia del cliente, no algo que el host deba conocer. No es estado de simulación autoritativa. |
| Mensajes de red (input, snapshot) | `net/protocol.rs` | Formato de wire alineado con la librería de networking aprobada. |

## Sistemas (comportamiento) — propuesta

- **`InputCapture`** — en una máquina `Client`, lee `input::ActiveActions`
  (nunca hardware crudo — mismo mecanismo que un Brain local, ver `input.md`),
  construye un `input::ActionFrame` local y lo envuelve en `LocalInputFrame`
  para enviarlo al host. No escribe `Intents` de simulación autoritativa en el
  cliente.
- **`HostReceiveInput`** — en el `Host`, deserializa `LocalInputFrame`
  recibido y emite `input::ApplyRemoteActionsMessage` hacia el `InputSource`
  de red asociado al `RemoteActor`. Multiplayer no escribe
  `input::ActiveActions` directamente.
- **`input::apply_remote_actions`** — corre en el host, valida el source
  remoto y escribe `ActiveActions` del `InputSource` correspondiente. **No
  traduce a `Intents`/`CombatIntents`** — los mismos Brains de
  Movement/Combat/NPCs que ya leen `ActiveActions` por `InputControlledBy`
  corren igual para el `RemoteActor`, sin lógica de traducción separada ni
  conocer que el origen es de red (`input.md`).
- **Replicación** — el `Host` serializa `LocomotionState`/`CombatState`/
  transforms tras `Arbitrate` y los envía a todos los `Client`; cada
  `Client` interpola su copia en `Update` (presentación, nunca simulación).
  Un cliente nunca decide su propio `LocomotionState`, el host lo hace.
- **Sets de multiplayer:** `InputCapture` y `ClientSendInput` corren en
  cliente; `HostReceiveInput`, `input::apply_remote_actions` y
  `AuthoritativeSimulation` corren en host; `ClientApplySnapshot` y
  presentación corren en cliente. Ver
  `rationale/multiplayer-gating.md`.

## Relaciones con otros sistemas

| Relación | Categoría | Mecanismo |
|---|---|---|
| Un jugador remoto es un `Actor` más para Movement/Combate | SHARED-CONTRACT | Mismos Brains genéricos; cambia el `InputSource` que alimenta `ActiveActions`, no la lógica de traducción a `Intents` |
| Input remoto | MESSAGE + SHARED-CONTRACT | `LocalInputFrame` replica `input::ActionFrame` resuelto localmente; Multiplayer emite `ApplyRemoteActionsMessage` e Input aplica el frame al `InputSource` del `RemoteActor` (`input.md`) |
| World (`TimeOfDay`/`Weather`) es estado de sesión compartido | SHARED-CONTRACT | Solo el `Host` lo simula; se replica igual que el resto |
| Toda la simulación (Movement/Combate/Monturas) | BLOCKING-PREREQUISITE | Requiere contrato multi-actor — ver `rationale/multi-actor-dispatch.md` |

## Decisiones abiertas

- Librería de networking (requiere aprobación explícita, §17): necesita
  descubrimiento de sesión, NAT traversal, canales confiables/no confiables.
- Sin client-side prediction en v1 (ver rationale).
- Reconexión / qué pasa si el host se cae (sin migración de host en v1).
- Cómo se une un jugador a mitad de sesión (snapshot inicial completo).
- Número objetivo de jugadores simultáneos (GDD §13).
