# Crafting

**Carpeta objetivo:** `src/crafting/`

Árbol de crafteo de equipo a partir de materiales recolectados (GDD §8,
prioridad #5 en §11) — "más profundidad que solo cocinar". Depende
enteramente de Inventory para existir (`BLOCKING-PREREQUISITE`).

## Datos (Components/Messages/Resources) — propuesta

| Tipo | Dónde | Qué es |
|---|---|---|
| `Recipe` | `crafting/recipe.rs` | Dato puro: `{ inputs: Vec<(ItemId, u32)>, output: ItemId, output_count: u32 }`. Vive en un asset/tabla (misma fuente de datos que `ItemId` en `inventory.md`), no hardcodeado en el sistema. |
| `RecipeBook` (Resource) | `crafting/mod.rs` | Colección de `Recipe` disponibles — no es estado por actor, es dato de contenido del juego. |
| `CraftItemMessage` | `crafting/messages.rs` | Pedido de craftear una `Recipe` específica para un actor. |
| `ItemCraftedMessage` | `crafting/messages.rs` | Confirmación de que el craft ocurrió (para que UI/SFX/VFX reaccionen), emitida solo después de una transacción aplicada por Inventory. |

## Sistemas (comportamiento) — propuesta

- **ValidateAndRequestTransaction** — `MessageReader<CraftItemMessage>`:
  verifica que la receta exista y emite `inventory::InventoryTransactionMessage`
  con los `inputs`/`output`. Inventory revalida disponibilidad y muta sus
  propios datos; si rechaza el pedido no hay panic — Constitución §9,
  condición esperable de juego.
- **ConfirmCraft** — escucha `InventoryTransactionAppliedMessage` con
  `reason: Crafting` y emite `ItemCraftedMessage`.

Sin Broker: crafteo es una acción discreta pedida por el jugador, no un
estado que compite por ser "el activo" — mismo criterio que Inventory/Health.

## Relaciones con otros sistemas

| Relación | Categoría | Mecanismo |
|---|---|---|
| Crafting lee `Inventory` y pide una transacción | READ + MESSAGE | Nunca muta `Inventory` ni `EquipmentSlots` directamente; el jugador equipa después vía `EquipItemMessage` |
| Crafting | Inventory | `BLOCKING-PREREQUISITE` | No tiene sentido sin el modelo de ítems de `inventory.md` |
| UI lee `RecipeBook` + `Inventory` para mostrar qué se puede craftear | READ | Query read-only sobre ambos |

## Decisiones abiertas

- Profundidad del árbol de recetas (GDD §13: "diseño concreto del árbol de
  crafteo" — explícitamente abierto).
- ¿Estaciones de crafteo requeridas (yunque, fogata) o crafteo libre en
  cualquier lugar?
- Origen de materiales (recolección en el mundo — depende de World, sin
  diseñar todavía).
- Si existen recetas de consumibles (elixires) con efecto sobre
  `StatusEffects` (resistencia a frío/calor) — ver `status-effects.md`.
