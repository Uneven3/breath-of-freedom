# Snowboard

**Carpeta objetivo:** `src/movement/` (motor nuevo, mismo plugin de
Movement — ver `rationale/traversal-extensions-in-movement.md`).

Deslizar en nieve/pendientes (GDD §9). El más simple de los tres traversals
nuevos: no introduce un pool propio, reutiliza `Stamina`/facts existentes.

## Datos (Components/Messages/Resources) — propuesta

| Tipo | Dónde | Qué es |
|---|---|---|
| `LocomotionState::Snowboard` | `movement/state.rs` | Variante nueva del enum SSoT. |
| `SlopeFacts` | `movement/facts.rs` | Hecho nuevo calculado por un servicio (`services/slope.rs` o extensión de `services/ground.rs`): `{ steepness: f32, surface: SurfaceKind }`. `SurfaceKind` distingue nieve de otras superficies (roca, pasto) para gatear el motor. |

No requiere un ítem-tabla propio (la "tabla" es el objeto físico); si más
adelante se craftea o se recoge un snowboard como ítem, eso es una relación
nueva hacia `inventory.md`, sin decidir todavía.

## Sistemas (comportamiento) — propuesta

- `services::slope` (o extensión de `ground`) — en `MovementSet::SenseWorld`,
  escribe `SlopeFacts` a partir de la normal del terreno bajo el actor y su
  material.
- `motors::snowboard::propose` — en `MovementSet::GatherProposals`, propone
  `Snowboard` cuando `SlopeFacts.surface == Snow` y `steepness` supera un
  umbral, con prioridad que compite igual que cualquier otro motor en
  `arbitrate()` (ej. cede ante `Jump`/`Climb` si el jugador los pide).

## Relaciones con otros sistemas

| Relación | Categoría | Mecanismo |
|---|---|---|
| World: la nieve como superficie/bioma origina `SurfaceKind::Snow` | READ | `services::slope` lee datos de terreno de World |
| StatusEffects: deslizar en nieve no moja (a diferencia de nadar) pero sí puede exponer a frío (GDD §10) | READ o MESSAGE | Sin decidir — ver `status-effects.md` |
| Combate: ¿se puede atacar/apuntar mientras se hace snowboard? | decisión abierta | Análogo a la pregunta equivalente en `swim.md` |

## Decisiones abiertas

- Requiere un objeto físico (tabla) equipable, o es una animación/modo de
  movimiento sin ítem — depende de si Inventory lo modela.
- Control aéreo/saltos sobre la tabla (¿reusa `Jump`/`Glide` o es exclusivo?).
- Cómo transiciona de vuelta a `Walk`/`Fall` al perder la pendiente o la
  superficie de nieve.
- Consumo de `Stamina` (¿gratis como Fall, o con costo como Sprint?).
