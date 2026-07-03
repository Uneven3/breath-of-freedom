# Inventory / Equipment

**Carpeta objetivo:** `src/inventory/`

Qué posee y qué tiene equipado cada actor (GDD §7-8). Fundacional: Combate
depende de esto para variar arsenal/durabilidad, Crafting depende de esto
para consumir/producir ítems.

## Datos (Components/Messages/Resources) — propuesta

| Tipo | Dónde | Qué es |
|---|---|---|
| `ItemId` | `inventory/item.rs` | Identificador estable de tipo de ítem (arma, material, elixir). Los datos concretos (peso, daño base, tags de material) viven en un asset/tabla, no hardcodeados. |
| `Inventory` | `inventory/mod.rs` | Componente por actor: contiene colecciones separadas de capacidad fija para optimizar almacenamiento y evitar reallocs en gameplay: `stacks: [Option<ItemStack>; MAX_STACKS]` para acumulables y `unique_items: [Option<UniqueItem>; MAX_UNIQUE_ITEMS]` para objetos con estado dinámico. |
| `ItemStack` | `inventory/mod.rs` | `{ item: ItemId, count: u32 }` para ítems acumulables (sin estado individual). |
| `UniqueItem` | `inventory/mod.rs` | `{ id: ItemId, durability: f32, ... }` para ítems no acumulables que conservan su propia durabilidad y modificadores en el inventario. |
| `EquipmentSlots` | `inventory/equipment.rs` | Componente por actor: `melee: Option<Entity>`, `ranged: Option<Entity>` (arco), `armor: Option<Entity>`. Al equiparse, el ítem se instancia como entidad hija para permitir colisionadores, transformaciones y adjuntar componentes como `Durability`. |
| `Durability` | `inventory/equipment.rs` | `{ current: f32, max: f32 }` en la entidad-ítem equipada. Estructuralmente similar a `Health` pero **no es `Health`** — un arma no muere, se rompe y deja de poder equiparse (`health.md` § Relaciones). |
| `EquipItemMessage` / `UnequipItemMessage` | `inventory/messages.rs` | Pedido de equipar/desequipar un slot. Al equipar, lee el `UniqueItem` del inventario y spawnea su entidad. Al desequipar, lee el estado de la entidad, la despawnea y guarda el `UniqueItem` actualizado de vuelta en el inventario. |
| `ApplyDurabilityLossMessage` | `inventory/messages.rs` | Pedido de descontar durabilidad al arma equipada de un actor. Inventory valida que el arma exista y que el golpe aplique al slot correcto. |
| `WeaponBrokenMessage` | `inventory/messages.rs` | Emitido cuando `Durability::current` llega a 0 en un arma equipada; el slot queda vacío y la entidad se despawnea. |
| `PickupItemMessage` | `inventory/messages.rs` | Un ítem del mundo entra al `Inventory` del actor. |
| `InventoryTransactionMessage` | `inventory/messages.rs` | Pedido atómico `{ actor, consume, grant, reason }` para Crafting/Quests. `consume`/`grant` son listas de capacidad fija; Inventory revalida disponibilidad y espacio antes de mutar. |
| `InventoryTransactionAppliedMessage` / `InventoryTransactionRejectedMessage` | `inventory/messages.rs` | Resultado explícito de una transacción; evita que Crafting/UI asuman que un pedido siempre se aplicó. El rechazo incluye causa (`MissingItems`, `InventoryFull`, `InvalidSlot`, etc.). |

## Sistemas (comportamiento) — propuesta

- **ApplyDurabilityLoss** — escucha `ApplyDurabilityLossMessage` de Combate
  (golpe conectado con el arma equipada) y descuenta `Durability`; si llega
  a 0, emite `WeaponBrokenMessage` y vacía el slot en `EquipmentSlots`.
- **Equip/Unequip** — `MessageReader<EquipItemMessage>`/
  `UnequipItemMessage`, maneja la instanciación/desinstanciación de la
  entidad del ítem y su traspaso hacia/desde `Inventory`.
- **Pickup** — `MessageReader<PickupItemMessage>`, agrega el ítem al
  inventario en la colección correspondiente (`stacks` si es apilable,
  `unique_items` si es único). Si no hay espacio, emite rechazo/feedback en
  vez de realocar o panicar.
- **ApplyTransaction** — `MessageReader<InventoryTransactionMessage>`,
  revalida disponibilidad, aplica consume/grant de forma atómica y emite
  resultado aplicado/rechazado. Ningún sistema externo muta `Inventory`
  directamente.

Sin `ProposalBuffer`/Broker: no hay competencia entre "comportamientos
activos" por frame, solo mutación de datos ante mensajes discretos — mismo
criterio que Health (`rationale/when-not-broker-pattern.md`).

## Relaciones con otros sistemas

| Relación | Categoría | Mecanismo |
|---|---|---|
| Combate lee `EquipmentSlots` (peso/velocidad/alcance del arma activa afecta `Windup`/`Recovery`) | READ | Query read-only — ver `combat.md` § Decisiones abiertas |
| Combate emite `ApplyDurabilityLossMessage` cuando conecta un golpe | MESSAGE | Combate no muta `Durability` directamente — respeta ownership |
| Crafting pide consumir/producir ítems | MESSAGE | Ver `crafting.md`; Inventory valida y muta sus propios datos |
| UI lee `Inventory`/`EquipmentSlots` para el menú (GDD "UI mínima") | READ | Query read-only |
| StatusEffects lee tags de material del ítem equipado (metal atrae rayos, GDD §10) | READ | Ver `status-effects.md` |

## Decisiones abiertas

- Formato de datos de ítems (¿asset RON, tabla en código?) y su pipeline
  (GDD §13 "pipeline de assets").
- Límites de stack/peso — ¿existe encumbrance que afecte Stamina/Movement?
- Cuántos slots de armadura, y si armadura interactúa con `StatusEffects`
  (resistencia a frío/calor).
- Origen de ítems en el mundo (loot de Enemies, cosecha, cofres) — depende de
  World/Enemies.
