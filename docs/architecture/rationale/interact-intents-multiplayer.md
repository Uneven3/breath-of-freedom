# Rationale: Componente InteractIntents en el Flujo de Entrada de Multiplayer y NPC (antigravity)

## El problema

En el diseño original de `npcs.md`, la interacción física del jugador con NPCs u objetos del mundo se describía mediante un ambiguo "input de interactuar". Esto asume que el sistema de proximidad del NPC puede consultar directamente las pulsaciones de teclado del sistema operativo locales (`Input<KeyCode>`).

Este supuesto rompe con dos invariantes fundamentales del proyecto:
1. **Multiplayer Host-Autoritativo:** En un cliente multijugador, la simulación física autoritativa corre en el host. El host no tiene acceso al hardware (teclado/mouse) del cliente. Si la lógica de proximidad del NPC espera interactuar leyendo el hardware local, las interacciones de los clientes remotos nunca se dispararán en el host.
2. **Modularidad e Inversión de Dependencias (Constitución §5 y §20):** La simulación de diálogos y quests debe reaccionar a una intención de entrada abstracta (ej. "el actor desea interactuar con lo que tenga enfrente"), de forma que la misma lógica funcione si es gatillada por un jugador local, un jugador remoto, o un compañero IA (Enemies).

---

## La decisión

Se introduce el componente `InteractIntents` (`{ wants_interact: bool }`) en la entidad del actor:

1. **Captura de Entrada Uniforme:**
   * Para el actor local: `Input` resuelve hardware a `ActiveActions` por
     `InputSource`; `CaptureInteractIntent` traduce `IntentAction::Interact`
     a `InteractIntents` en el actor enlazado por `InputControlledBy`.
     (codex)
   * Para actores remotos: el host recibe `LocalInputFrame` con un
     `input::ActionFrame`; Multiplayer emite `ApplyRemoteActionsMessage`;
     Input aplica el frame al
     `InputSource` de red del `RemoteActor`; y el mismo
     `CaptureInteractIntent` escribe `InteractIntents`. No hay
     `NetworkBrain` de traducción separado. (codex)
   * Para IA: un sistema propio de IA puede escribir `InteractIntents`
     directamente si el actor no está controlado por `Input`. (codex)

2. **Procesamiento Autoritativo en el Host:**
   El sistema `DetectInteraction` corre en `FixedUpdate` (simulación) y solo se activa en el host (o en modo single-player). Consulta:
   ```rust
   Query<(&Transform, &InteractIntents), With<Actor>>
   ```
   Si un actor tiene `wants_interact == true`, el sistema busca el interactuable más cercano dentro de su rango físico y desencadena la interacción (abre diálogo, emite `PickupItemMessage`, abre cofre, etc.). (codex)

---

## Consecuencia

La interacción con NPCs y el entorno queda totalmente desacoplada del hardware local, habilitando soporte nativo para multijugador autoritativo y comportamientos autónomos de IA (compañeros que interactúan con objetos). Además, se respeta la separación estricta entre lectura de inputs y simulación de lógica de juego.
