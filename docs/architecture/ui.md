# UI

**Carpeta objetivo:** `src/ui/`

Pilar del GDD: "UI mínima" — el mundo comunica el estado, la pantalla se
mantiene limpia. UI no introduce datos de simulación nuevos: es consumidor
de presentación descrito en Constitución §20.

## Datos (Components/Messages/Resources) — propuesta

| Tipo | Dónde | Qué es |
|---|---|---|
| Nodos de Bevy UI (HUD, prompts contextuales) | `ui/hud.rs` | Presentación pura, sin estado propio de simulación. |
| `UiState` / estado de navegación | `ui/state.rs` | Estado de presentación: menú abierto, foco, selección, animaciones. Nunca participa en reglas de simulación. |

No hay `Component`/`Resource` de simulación propio de UI — todo lo que
muestra como estado de juego ya existe en otro sistema. UI sí puede tener
estado propio de presentación.

## Sistemas (comportamiento) — propuesta

- Sistemas de lectura en `Update` que leen `Stamina` (Movement), `Health`
  (Health), `Bond` (Monturas), `Inventory`/`EquipmentSlots` (Inventory),
  `RecipeBook` (Crafting) e `InteractionCandidate`/`QuestBook` (NPCs), y
  actualizan nodos de Bevy UI. Ninguno escribe a un componente de simulación.
- Menú de guardado/carga: dispara `persistence::SaveRequestMessage`/
  `LoadRequestMessage` ante una acción del jugador. Es el único caso donde UI
  emite un mensaje — no es un dato de simulación, es un pedido de
  orquestación que Persistence decide cómo cumplir (`persistence.md`).
- Menú de reasignación de teclas: lee `input::Keybindings` para presentar la
  tabla actual y emite `input::RebindRequestMessage`; Input valida y escribe
  su propio `Keybindings`.

## Relaciones con otros sistemas

| Relación | Categoría | Mecanismo |
|---|---|---|
| UI lee `Movement::Stamina` | READ | Query read-only |
| UI lee `Health` | READ | Query read-only — ver `health.md` |
| UI lee `Monturas::Bond` | READ | Query read-only |
| UI lee `Inventory`/`EquipmentSlots` (menú) | READ | Query read-only — ver `inventory.md` |
| UI lee `RecipeBook` + `Inventory` (menú de crafteo) | READ | Query read-only — ver `crafting.md` |
| UI lee `InteractionCandidate` (prompt contextual) y `QuestBook`/`QuestProgressedMessage` (registro de quests) | READ + MESSAGE | Ver `npcs.md` |
| UI dispara `SaveRequestMessage`/`LoadRequestMessage` (menú) | MESSAGE | Único caso donde UI emite — ver `persistence.md`; UI no conoce el formato del archivo |
| UI lee `Keybindings` y pide cambios de binding | READ + MESSAGE | Ver `input.md`; UI nunca muta la tabla de Input directamente |

Salvo el guardado/carga, UI no aparece como emisor en ninguna otra
relación — sigue siendo hoja del grafo de simulación.

## Decisiones abiertas

- Qué prompts contextuales existen (interactuar, montar, escalar).
- Estructura del menú de inventario/crafteo (ambos ya tienen diseño propio
  en `inventory.md`/`crafting.md`, falta el layout de UI).
