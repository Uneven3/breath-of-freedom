# Rationale: Gating de Simulación en Clientes de Multiplayer (codex)

## El problema

En el modelo multiplayer host-autoritativo (LAN/Hamachi-style), el host simula todas las entidades y el mundo como única fuente de verdad (SSoT). El cliente envía sus inputs y recibe los estados replicados (transforms, `LocomotionState`, `CombatState`, `TimeOfDay`, `Weather`, etc.).

Sin embargo, dado que `MovementPlugin` y `WorldPlugin` registran sus sistemas en `FixedUpdate` (como `arbitrate`, `motors::tick_active_motor`, `advance_clock`, etc.), en una sesión cliente-servidor el cliente ejecutaría estos sistemas sobre su propio jugador y mundo de forma local de manera predeterminada. Esto causaría:
1. **Conflictos de Simulación:** El cliente simularía localmente su posición y física en `FixedUpdate`, mientras que en `Update` el sistema de replicación de red intentaría sobreescribir la posición con los datos enviados por el host. Esto produce fluctuaciones extremas (jitter) y desincronización constante.
2. **Ciclo de Día/Noche divergente:** El reloj local del cliente avanzaría de forma independiente al del host, desalineando el ciclo día/noche y clima.

---

## La decisión

Para evitar que el cliente ejecute lógica autoritativa, se separan los sets
de red en captura/envío de input, simulación autoritativa y aplicación de
snapshots. El cliente puede capturar input local, pero no escribe
componentes de simulación autoritativa como `Intents`/`LocomotionState`.

### 1. Qué corre en el Cliente
El cliente ejecuta:
* `InputCapture`: lee `input::ActiveActions` (resuelto localmente por
  `resolve_bindings` a partir de la `Keybindings` del jugador — nunca
  hardware crudo, ver `input.md`), construye `input::ActionFrame` y lo envuelve
  en `LocalInputFrame`. (codex)
* `ClientSendInput`: serializa `LocalInputFrame` y lo transmite al host.
* `ClientApplySnapshot`: aplica snapshots autorizados del host a entidades
  replicadas.
* Presentación en `Update`: interpolación visual, cámara, HUD, SFX/VFX.

### 2. Qué corre en el Host
El host ejecuta:
* `HostReceiveInput`: deserializa `LocalInputFrame` y emite
  `input::ApplyRemoteActionsMessage` para el `InputSource` de red del
  `RemoteActor`. Multiplayer no muta `ActiveActions` directamente. (codex)
* `input::apply_remote_actions`: valida el `InputSource` remoto y escribe
  `ActiveActions`. No traduce a `Intents`/`CombatIntents` directamente. Los
  mismos Brains genéricos de Movement/Combat/NPCs corren para ese actor
  dentro de `ReadIntents`, igual que para un actor local (`input.md`).
  (codex)
* `AuthoritativeSimulation`: Movement, Combat, Mounts, World y toda lógica que
  decide estado de juego.
* `HostReplicateSnapshot`: emite snapshots para clientes.

### 3. Qué se cancela en el Cliente
Los siguientes sistemas y fases de simulación autoritativa se omiten en el
cliente mediante condiciones de ejecución asociadas a `NetworkRole::Client`:

* **Sensores del Mundo:** `MovementSet::SenseWorld` (los servicios de raycasts/sphere-casts del mundo no corren, ahorrando CPU en el cliente).
* **Propuestas y Arbitración:** `MovementSet::GatherProposals` y `MovementSet::Arbitrate` (no se proponen estados ni se decide el `LocomotionState` localmente; el estado viene del snapshot autorizado).
* **Movimiento Físico (Motores):** `MovementSet::TickActiveMotor` (no se integra posición ni se aplica `MoveAndSlide` localmente; el transform se recibe del host e interpola en `Update`).
* **Reloj y Clima:** `advance_clock` y `advance_weather` del `WorldPlugin` (se cancela el avance local; el tiempo de juego y clima se sincronizan mediante replicación del host).

---

## Consecuencia

El cliente se comporta de manera pasiva respecto de la simulación física y
temporal, pero activa respecto de input y presentación. Esto evita una doble
fuente de verdad sin mezclar componentes autoritativos con snapshots
replicados. (codex)
