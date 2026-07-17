# Log de tickets cerrados

Todos los tickets listados están implementados, jugados y mergeados; el
detalle histórico vive en `git log -- docs/tickets/<slug>.md`. La arquitectura
resultante está en `docs/architecture/`; los porqués, en
`docs/architecture/rationale/`. Los tickets nuevos siguen usando
`docs/TICKET-TEMPLATE.md` como archivos individuales en esta carpeta, y se
condensan a una línea aquí al cerrarse.

- `bokobo-brain` — Enemies (nuevo plugin `src/enemies/`). Primer slice: un enemigo de graybox
- `climb-lip-jump-mantle` — Desde `Climb` o `Ladder`, si LedgeService confirma un borde con Mantle
- `combat-bow-fixes` — Revisión de código 2026-07-16 sobre HEAD (`52475eb`). La nota **KNOWN BUG**
- `combat-bow` — Combat (estado `Aiming` + motor `aim`), **Projectiles** (plugin nuevo),
- `combat-game-feel` — Presentation (nuevo `presentation/juice.rs`), con contratos nuevos en Combat
- `combat-melee-combo` — Combat (sobre `combat-scaffolding`), con placeholder de VFX en Visuals y
- `combat-scaffolding` — Combat (nuevo plugin `src/combat/`), con un toque quirúrgico en Movement
- `diagonal-climb-continuation-normal` — El trace mostró `climb=false/true side=true/true n=(0,0,0)` al aproximarse a
- `enemies-combat` — Enemies (brain de combate, `EnemyAiState::Combat`, arquetipo arquero),
- `enemy-awareness` — Enemies. Reemplaza la detección binaria del slice `bokobo-brain` por un
- `enemy-hearing-damage-aggro` — Enemies. Agrega los dos estímulos que faltaban al modelo de sentidos de
- `health-core` — Health (`src/health/`, plugin nuevo — fase 3 de `combat.md`), con cableado
- `input-camera-foundation` — Input, Movement y Camera. No se pueden separar: Movement hoy lee hardware y
- `jump-while-crouched-under-ceiling` — Movement (arbitraje Sneak vs Jump y el swap de cápsula de `sync_sneak_collider`).
- `ladder-attachment-traversal` — Movement / World. Ladder es un motor de anclaje vertical sin stamina; no es
- `motor-dispatch-guard-enforcement` — Movement (el guard por entidad de cada `tick` y cómo se despachan los 13
- `mounts-core` — Este ticket queda superseded para trabajo futuro por
- `movement-air-and-stairs-capabilities` — Movement. Migra Jump y Glide a capacidades persistentes independientes y
- `movement-airborne-profile` — Movement. Migra el tuning actualmente global de `Fall` a un perfil persistente
- `movement-body-dimensions` — Movement. Migra las dimensiones globales de la cápsula del actor a
- `movement-composition-bundles` — Movement. Reemplaza el armado manual y frágil del Player por bundles de datos
- `movement-ground-ability` — Checkpoint aceptado. El Player conserva el comportamiento validado de Walk;
- `movement-ground-modes` — Movement. Extiende la capacidad persistente `GroundMovement` para que sus
- `movement-sensor-profiles` — Movement sensing. Convierte los alcances, alturas y umbrales que todavía son
- `movement-traversal-capabilities` — Movement. Migra las capacidades persistentes de traversal ya validadas para
- `multi-actor-migration` — Movement (refactor de `Single<Player>` → `Query<Actor>` en motores,
- `probe-mantle-glide` — Movement (extensión del `TraversalProbe` del ticket `traversal-probe`). El
- `proposal-core-extraction` — Movement + nucleo compartido interno `src/proposal.rs`.
- `sfx-system-scaffolding` — SFX (y el bus de presentación compartido `presentation/cues.rs`).
- `sneak-on-stairs` — Se implementó la **opción 3 (crouch como modificador ortogonal)**, que es la
- `sneak-stand-clearance` — Movement / Sneak. El cambio está confinado al motor Sneak y a sus datos por
- `stairs-geometry-matrix` — Movement / World. World aporta cursos graybox authored; Movement mantiene el
- `traversal-probe` — Movement. El probe es un controlador de integracion del curso gris: consume el
- `wall-jump-neutral-input` — Mientras el actor está en `Climb` o `Ladder`, pulsar Jump sin dirección inicia
