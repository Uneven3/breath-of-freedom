# NPCs / Quests

**Carpeta objetivo:** `src/npcs/`

Personajes con problemas propios, estilo *Majora's Mask* (GDD §4, prioridad
#7 en §11) — el jugador opta por resolverlos, sin trama principal
obligatoria (GDD §12). Distinto de `Enemies`: no hostil, no usa
`EnemyAiState`/`CombatIntents`.

## Datos (Components/Messages/Resources) — propuesta

| Tipo | Dónde | Qué es |
|---|---|---|
| `Npc` (marker) | `npcs/mod.rs` | Marca la entidad como personaje no-hostil interactuable. |
| `QuestId` | `npcs/quest.rs` | Identificador estable de un "problema" resoluble. |
| `QuestState` | `npcs/quest.rs` | Enum SSoT por quest: `NotStarted`, `InProgress`, `Resolved`. Solo un sistema propio lo muta. |
| `QuestBook` (Resource) | `npcs/mod.rs` | Colección de quests activas en el mundo y su `QuestState` — dato de contenido, no por-actor. |
| `InteractIntents` | `npcs/intent.rs` | Componente por actor: `{ wants_interact: bool }`. Los inputs de interacción (físicos, de IA o de red) se mapean aquí para ser procesados por la simulación del host. |
| `InteractionCandidate` | `npcs/interaction.rs` | Componente/estado de presentación por actor local: NPC interactuable actual, si existe. UI lo lee para mostrar el prompt; no es un suceso discreto. |
| `QuestProgressedMessage` | `npcs/messages.rs` | Emitido cuando una acción del jugador hace avanzar un `QuestState`. |

## Sistemas (comportamiento) — propuesta

- **CaptureInteractIntent** — para el actor local, escribe
  `InteractIntents::wants_interact` desde `input::ActiveActions` para el
  `InputSource` enlazado por `input::InputControlledBy` en el actor
  (`input.md`, ver también
  `rationale/interact-intents-multiplayer.md`): `true` si
  `IntentAction::Interact` tiene una generación nueva para el
  `InputConsumeCursor` de NPCs/actor. NPCs no lee `ButtonInput<KeyCode>`, no
  sabe qué tecla o esquema la dispara y no consume el snapshot global.
- **DetectInteraction** — proximidad + `InteractIntents::wants_interact` →
  actualiza `InteractionCandidate` y dispara diálogo/interacción cuando el
  intent está activo. No usa `ProposalBuffer`: es detección de rango, no
  arbitración de "comportamiento activo".
- **NpcRoutine** — comportamiento no-hostil por NPC (horario, posición según `TimeOfDay`) — simple, no compite con nada, sin Broker. En multiplayer, este comportamiento de simulación corre únicamente en el `Host`, y las posiciones de los NPCs se replican a los clientes.
- **AdvanceQuest** — `MessageReader` de mensajes de gameplay relevantes
  (matar un objetivo, entregar un ítem, llegar a un lugar) que hacen avanzar
  `QuestState`, emite `QuestProgressedMessage`.

## Relaciones con otros sistemas

| Relación | Categoría | Mecanismo |
|---|---|---|
| NPCs lee `World::TimeOfDay` para rutinas (¿duerme de noche?) | READ | Query read-only |
| UI lee `InteractionCandidate`/`QuestBook` y escucha `QuestProgressedMessage` | READ + MESSAGE | Nunca escribe hacia atrás |
| Un quest puede requerir un ítem de `Inventory` (entregar un objeto) | READ | Lee `Inventory` del jugador, no lo muta directamente — el consumo (si aplica) pasa por la misma API que Crafting usa |
| Persistence necesita serializar `QuestBook`/`QuestState` | WRITE-OWN (por NPCs dentro de `PersistenceSet`) | Ver `persistence.md` — NPCs registra sus propios sistemas de guardado/carga |

## Decisiones abiertas

- Estructura concreta de "problemas resolubles" (GDD §13, explícitamente
  abierto): ¿árbol de quests, independientes entre sí, o interconectadas?
- Formato de diálogo/datos narrativos (texto plano, asset, herramienta de
  autoría) — sin decidir.
- ¿Los NPCs tienen `Health` (pueden morir/ser atacados por error) o son
  invulnerables? Afecta si comparten componente con `health.md`.
- Si un NPC puede volverse hostil bajo alguna condición (cruce con
  `enemies.md`) — no contemplado en el GDD actual.
