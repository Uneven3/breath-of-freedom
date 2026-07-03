# Rationale: Almacenamiento de Durabilidad e Ítems Únicos en el Inventario (antigravity)

## El problema

En la propuesta original de `inventory.md`, el inventario se describía de forma simplificada como una colección plana de `ItemStack { item: ItemId, count: u32 }`. 

Sin embargo, las armas y escudos del juego poseen **durabilidad individual** (además de posibles modificadores aleatorios, tags de material dinámico, etc.). Si todas las armas de tipo `ItemId::Sword` en el inventario se almacenan en un `ItemStack` genérico con una cantidad, nos enfrentamos a una inconsistencia insalvable:
1. **Pérdida de Estado:** Al desequipar una espada dañada (que era una entidad con `Durability` en `EquipmentSlots`), si la guardamos en un `ItemStack { item: ItemId::Sword, count: 2 }`, perdemos por completo el registro de cuánta durabilidad le quedaba.
2. **Duplicación Irreal:** Si se agrupan en pilas, al equipar una espada de la pila, ¿qué durabilidad tendría? ¿la máxima? Esto permitiría curar espadas infinitamente simplemente desequipándolas y volviéndolas a equipar.

---

## La decisión

Se divide el almacenamiento del inventario en dos colecciones diferenciadas según la naturaleza del ítem:

1. **Ítems Acumulables (Stacks):**
   * Estructura: `stacks: [Option<ItemStack>; MAX_STACKS]` donde `ItemStack` es `{ item: ItemId, count: u32 }`.
   * Para: Flechas, ingredientes de comida/elixires, materiales de crafteo (madera, metal, etc.).
   * Propiedades: No tienen estado individual, se apilan para ahorrar memoria y procesamiento; si se llena la capacidad, Inventory rechaza el pickup/transacción con error explícito. (codex)

2. **Ítems Únicos (UniqueItems):**
   * Estructura: `unique_items: [Option<UniqueItem>; MAX_UNIQUE_ITEMS]` donde `UniqueItem` es `{ id: ItemId, durability: f32, ... }`.
   * Para: Espadas, arcos, escudos, armaduras.
   * Propiedades: Nunca se apilan (incluso si tienen el mismo `ItemId`). Cada elemento de la lista conserva su propia durabilidad y estadísticas dinámicas; no hay crecimiento de heap en el camino de gameplay. (codex)

### Flujo de Equipado y Desequipado (Frontera de Entidades):
* **Al equipar (Traspaso de Datos a Entidad):**
  Se busca el `UniqueItem` en el inventario del actor, se remueve de la lista, y se spawnea una entidad de Bevy (hija del actor) con los componentes visuales necesarios y el componente `Durability { current: unique_item.durability }`. El ID de la entidad se guarda en `EquipmentSlots`.
* **Al desequipar (Consolidación de Entidad a Datos):**
  Se lee el componente `Durability` de la entidad equipada, se despawnea la entidad de Bevy, y se inserta un nuevo `UniqueItem { id, durability: durability.current }` de vuelta en la colección `unique_items` del inventario del actor.
* **Al romperse (Descarte):**
  Se emite `WeaponBrokenMessage`, se vacía el slot en `EquipmentSlots` y se despawnea la entidad de Bevy sin devolver nada al inventario. (codex)

---

## Consecuencia

El sistema soporta correctamente el desgaste de equipo y las variaciones dinámicas de armas y escudos individuales de manera consistente con el diseño de juego (estilo *Breath of the Wild*). Al mismo tiempo, evita tener docenas de entidades inactivas y pesadas en el ECS de Bevy para representar objetos no equipados del inventario, serializándose de forma compacta como datos planos.
