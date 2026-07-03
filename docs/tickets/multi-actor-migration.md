# Ticket: `multi-actor-migration`

## Sistema(s)

Movement (refactor de `Single<Player>` → `Query<Actor>` en motores,
arbitraje y spawn).

## Lectura obligatoria, en este orden

1. `docs/CONSTITUTION.md` — completo.
2. `docs/ARCHITECTURE-MAP.md` — fila `Movement`.
3. `docs/COUPLING-MAP.md` — Movement es Tight con Enemies y Multiplayer
   (este ticket es el `BLOCKING-PREREQUISITE` de ambos).
4. `docs/architecture/movement.md`.
5. `docs/architecture/rationale/multi-actor-dispatch.md` (si existe; si no
   existe todavía, `src/movement/spike.rs` es la referencia normativa del
   patrón — leerlo completo, es la prueba ya validada de este mismo diseño).

No leer nada fuera de esta lista para decidir diseño.

## Acoplamiento

- **Tight con Enemies y Multiplayer**: ambos quedan bloqueados hasta que
  este ticket cierre. No abrir worktrees de Enemies/Multiplayer en
  paralelo con este.
- **Con Input (`brain.rs`)**: hoy `brain::read_intents` lee
  `ButtonInput<KeyCode>` crudo vía `Single<&mut Intents, With<Player>>`,
  no el modelo de `input.md` (`ActiveActions`/`InputControlledBy`). Este
  ticket **no lo toca** — ver Fuera de alcance. Si el ticket de Input se
  abre en paralelo, no hay conflicto de archivo porque `brain.rs` queda
  intacto acá.
- **Con `proposal-core-extraction`** (si corre en paralelo): ese ticket
  toca `src/proposal.rs` (nuevo, núcleo genérico) y el cuerpo interno de
  `src/movement/proposal.rs::ProposalBuffer::arbitrate`. Este ticket llama
  a `buffer.arbitrate()` desde `mod.rs::arbitrate()` pero no cambia su
  firma pública (`&self -> LocomotionState`) — si esa firma cambiara del
  otro lado, coordinar antes de mergear, no asumir.

## Alcance (File Touches)

- `src/movement/mod.rs` (arbitrate, in_loco_state, spawn_player, registro
  de sistemas en `MovementPlugin`)
- `src/movement/motors/*.rs` (los 13 motores: `propose` y `tick`)
- `src/movement/motor_common.rs` (si el helper compartido asume `Single`)
- `src/camera.rs` (debe seguir al actor local marcado, no a "el" único
  `Player`)
- `src/movement/spike.rs` (queda como referencia normativa del patrón, no
  se borra aunque dejen de ser necesarias sus pruebas)
- `docs/architecture/movement.md` (si el resultado real diverge del texto
  actual)
- `docs/tickets/multi-actor-migration.md` (este archivo)

## Fuera de alcance

- No toca `src/movement/brain.rs` ni el modelo de Input — `read_intents`
  sigue escribiendo `Intents` solo para el jugador local vía
  `Single<Player>` tal como está hoy. Generalizar quién controla qué
  `Actor` es responsabilidad del ticket de Input (`InputControlledBy`) y,
  más adelante, de Enemies/Multiplayer (IA/red escribiendo `Intents` de
  otros actores).
- No agrega Enemies ni Multiplayer — solo deja el pipeline listo para que
  esos tickets puedan empezar después.
- No resuelve las discrepancias de `override_weight: i32` vs `u32` ni el
  `unwrap_or(Fall)` vs `return current` de `proposal.rs` — eso es
  `proposal-core-extraction` (ver `rationale/proposal-arbitration-core.md`).

## Definición de terminado

- [x] `Player` se conserva como marker (sigue siendo el jugador local),
      pero se introduce `Actor` como marker genérico que `Player` también
      lleva — los 13 motores corren sobre `Query<.., With<Actor>>` con
      guard interno por entidad (`if *state != LocomotionState::X {
      continue }`), no `run_if` global.
- [x] `arbitrate()` corre sobre `Query<(&mut LocomotionState, &mut
      ProposalBuffer), With<Actor>>`, no `Single<.., With<Player>>`.
- [x] `spawn_player` sigue existiendo tal cual (un solo actor hoy), pero
      el shape de sus componentes es el que cualquier `Actor` futuro
      (enemigo, remoto) necesitará replicar — documentado en
      `movement.md` § Datos (fila de componentes promovidos).
- [x] `src/camera.rs` sigue al actor local marcado explícitamente (no
      asume que "el único actor" es siempre la cámara — importante para
      cuando existan NPCs/enemigos con el mismo componente `Actor`).
      Cambio solo de comentario, sin lógica nueva.
- [x] `src/movement/spike.rs` sigue pasando sin cambios (`git diff` vacío,
      sus 3 tests pasan). No se encontró drift respecto al código real
      migrado.
- [x] Test nuevo (no de *feeling*, invariante de arquitectura):
      `src/movement/mod.rs::actor_isolation_tests`, 3 tests contra
      `propose`/`arbitrate` reales (no `spike.rs`) — confirman que
      `LocomotionState`/`JumpLocal`/`SprintLock` no cruzan entre dos
      actores. **Cobertura parcial, a propósito:** un 4to test que
      corriera `tick` real bajo física de Avian se intentó y se descartó
      (ver `docs/implement-feature/multi-actor-migration-plan.md` §
      Fidelity Check, fila 16, para el detalle de por qué) — la
      correctitud de `tick` bajo física real queda como terreno de
      play-test, mismo límite que ya trazan `motors::climb::tests`/
      `motors::edge_leap::tests`, confirmado explícitamente por el
      usuario.
- [x] `cargo fmt` limpio.
- [x] `cargo clippy` sin warnings nuevos — ningún `#[allow(...)]` sin
      justificación explícita en el commit (20 warnings nuevos de
      `type_complexity` resueltos con alias de tipo, no con `#[allow]`).
- [x] `cargo check` / `cargo test` pasa completo (27/27, incluye
      `spike.rs`).
- [x] El comportamiento coincide con `movement.md` — doc actualizado en
      este mismo ticket (tabla de Datos + nota de prerequisito resuelto).
- [x] Sin `unsafe`; sin `unwrap()`/`expect()` nuevo fuera de código de
      test (confirmado por sweep `rg` contra el baseline pre-migración).
- [x] No aplica cambio de relación entre sistemas (`Movement` no cambió
      su contrato con otros sistemas, solo su mecanismo interno) — sí se
      actualizó `ARCHITECTURE-MAP.md` para marcar el `BLOCKING-PREREQUISITE`
      de Enemies/Multiplayer como resuelto. `COUPLING-MAP.md` no requiere
      cambio (el nivel de acoplamiento declarado no varió).

## Notas para el agente que lo toma

- `spike.rs` ya es la prueba de que el patrón funciona (`Query<Actor>` +
  guard interno + componente de estado por entidad en vez de `Local`) —
  este ticket es "hacer eso mismo, pero en los 13 motores reales", no
  inventar un patrón nuevo. Si algo en el código real no encaja con el
  patrón del spike, es más probable que el motor real tenga un caso no
  cubierto por el spike (ej. `Local<bool>` de `sprint.rs::stamina_locked`)
  que haya que resolver, no que el patrón esté mal.
- Motores conocidos con estado `Local<...>` que hay que migrar a
  componente por entidad (grep `Local<` en `src/movement/motors/`):
  `sprint.rs` (`stamina_locked`), y cualquier otro que aparezca en el
  grep — no asumir que la lista está completa solo con este.
- No hace falta esperar el checkpoint de *feeling* para los tests de
  invariante de arquitectura de este ticket (Constitución §10/§11,
  excepción explícita).
