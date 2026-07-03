# Persistence

**Carpeta objetivo:** `src/persistence/`

Guardado/carga de partida; tamaño del mundo y modelo de persistencia son
decisión de producto abierta (GDD §13). Sistema transversal: no posee datos
de gameplay, orquesta que cada sistema dueño se serialice a sí mismo — ver
`rationale/persistence-orchestration.md`.

## Datos (Components/Messages/Resources) — propuesta

| Tipo | Dónde | Qué es |
|---|---|---|
| `SaveRequestMessage` / `LoadRequestMessage` | `persistence/messages.rs` | Dispara el ciclo de guardado/carga. Quien lo emite (UI, autosave por tiempo) es indistinto para Persistence. |
| `PendingPersistenceOp` (Resource) | `persistence/mod.rs` | Estado mínimo de orquestación: `None`, `Saving(slot)`, `Loading(slot)`. No contiene datos de gameplay. |
| `PersistenceSet` (SystemSet) | `persistence/mod.rs` | `Begin`, `Collect`, `ApplyLoad`, `Finalize` — cada sistema dueño de datos persistibles agrega su propio `save_x`/`load_x` a estos sets (ver rationale). Persistence no conoce el contenido. |
| `SaveSlot` (Resource) | `persistence/mod.rs` | Metadata del archivo actual (ruta, timestamp) — no el contenido del guardado. |

Persistence **no define** un tipo `SaveGame` con todos los campos de todos
los sistemas — eso volvería a acoplar todo en un solo lugar. Cada sistema
serializa sus propios `Component`s públicos.

## Sistemas (comportamiento) — propuesta

- **BeginSave/BeginLoad** — `MessageReader` de `SaveRequestMessage`/
  `LoadRequestMessage`, actualiza `PendingPersistenceOp`. No intenta ejecutar
  dinámicamente un `SystemSet` desde dentro de un sistema; Bevy schedules son
  declarativos.
- **Collect/ApplyLoad** — sistemas registrados estáticamente por cada dueño,
  gateados por `PendingPersistenceOp`, escriben/leen sus fragmentos.
- **Finalize** — Persistence arma o consume el archivo final (formato sin
  decidir) y limpia `PendingPersistenceOp`.
- Cada sistema dueño (Movement, Health, Inventory, NPCs/`QuestBook`,
  World/`TimeOfDay`+`Weather`, ...) implementa sus propios `save_x`/`load_x`
  dentro de su propio plugin — Persistence no los escribe.

## Relaciones con otros sistemas

| Relación | Categoría | Mecanismo |
|---|---|---|
| Persistence orquesta el guardado de cualquier sistema que lo registre | WRITE-OWN (por el dueño, no por Persistence) | Ver `rationale/persistence-orchestration.md` |
| Multiplayer: snapshots de red vs. archivo de guardado | decisión abierta | Ambos serializan estado de simulación pero con objetivos distintos (transmitir vs. persistir) — si comparten código de serialización o son independientes no está decidido |
| UI dispara `SaveRequestMessage`/`LoadRequestMessage` (menú) | MESSAGE | UI no conoce el formato del archivo |

## Decisiones abiertas

- Formato de archivo (RON, bincode, uno por sistema vs. blob único).
- Tamaño del mundo y si la persistencia es total o por chunks (GDD §13,
  explícitamente abierto — condiciona si esto es viable de diseñar en
  detalle antes de saber el tamaño del mundo).
- Autosave vs. guardado manual únicamente.
- Compatibilidad de versión de guardado entre versiones del juego (¿importa
  para un proyecto GNU sin release formal todavía?).
- Cómo interactúa con sesiones multiplayer (¿solo el host guarda? ¿cada
  cliente guarda su propio progreso si existe progreso individual?).
