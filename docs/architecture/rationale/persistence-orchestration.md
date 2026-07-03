# Por qué Persistence no conoce los internals de los demás sistemas

**Problema:** guardar/cargar partida necesita tocar datos de *todos* los
sistemas (posición, `Inventory`, `Health`, `QuestBook`, `TimeOfDay`/`Weather`,
...). Un diseño ingenuo haría que `Persistence` importe los tipos concretos
de cada sistema y los serialice directamente — viola Constitución §5
(inversión de dependencias) y §2 (extensión aditiva): cada sistema nuevo
obligaría a editar `Persistence` para soportarlo.

**Decisión:** Persistence no serializa; **orquesta**. Cada sistema que
quiere ser persistido:

1. Deriva `serde::Serialize`/`Deserialize` en sus propios `Component`s
   públicos (dato puro, Constitución §6 — no cambia su lógica).
2. Registra su propio par de sistemas `save_<sistema>`/`load_<sistema>` en
   su propio plugin, ordenados dentro de `PersistenceSet::Collect`/
   `::ApplyLoad` (definido por Persistence, implementado por cada dueño).
   (codex)
3. Persistence solo define el `SystemSet`, el trigger (`SaveRequestMessage`/
   `LoadRequestMessage`), `PendingPersistenceOp` y el archivo/formato final —
   nunca el contenido. (codex)

Esto es exactamente el mismo patrón que `LocomotionConstraintMessage`
(Combate pide, Movement decide) invertido: acá Persistence marca una
operación pendiente y cada sistema decide cómo guardar/cargar sus datos.
Aditivo por construcción — un sistema nuevo se suma agregando su propio
`save_x`/`load_x` al `SystemSet`, sin tocar `src/persistence/`. (codex)

## Costo aceptado

El archivo de guardado queda compuesto por N fragmentos independientes (uno
por sistema) en vez de un único blob con forma conocida de antemano — el
formato exacto (RON por sistema, un blob bincode con secciones, etc.) es una
decisión abierta en `persistence.md`, no bloqueante para el diseño de
arquitectura.
