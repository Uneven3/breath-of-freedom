# Ticket: `proposal-core-extraction`

## Sistema(s)

Movement + nucleo compartido interno `src/proposal.rs`.

Este ticket toca Movement porque adapta su `ProposalBuffer` real al contrato
objetivo ya documentado, y toca `src/proposal.rs` porque el algoritmo de
arbitracion debe quedar disponible como libreria interna para Movement,
Combat y Mounts. No implementa Combat ni Mounts.

## Lectura obligatoria, en este orden

1. `docs/CONSTITUTION.md` — completo.
2. `docs/ARCHITECTURE-MAP.md` — filas `Movement` y `Movement, Combat, Mounts`.
3. `docs/COUPLING-MAP.md` — par Tight `Movement<->Combat<->Mounts`.
4. `docs/architecture/movement.md`.
5. `docs/architecture/combat.md` — solo la forma esperada de
   `CombatProposalBuffer`.
6. `docs/architecture/mounts.md` — solo la forma esperada de
   `MountProposalBuffer`.
7. `docs/architecture/rationale/proposal-arbitration-core.md`.
8. `docs/architecture/rationale/multi-actor-dispatch.md` — solo para no
   romper el ticket paralelo `multi-actor-migration`; este ticket no lo
   implementa.

## Acoplamiento

- Tight con Combat y Mounts por la forma de
  `proposal::ProposalBuffer<S, N>`, `proposal::TransitionProposal<S>` y
  `proposal::Priority`.
- Este ticket fija el nucleo compartido antes de que Combat/Mounts dependan
  de el en codigo.
- No depende de `multi-actor-migration`: puede seguir usando el pipeline
  actual de Movement mientras no endurezca ningun supuesto de actor unico
  nuevo.
- Si corre en paralelo con `multi-actor-migration`, coordinar el merge sobre
  `src/movement/mod.rs::arbitrate`: este ticket cambia deliberadamente la
  llamada desde `buffer.arbitrate()` a `buffer.arbitrate(current)`, porque
  el contrato objetivo del nucleo generico necesita fallback explicito al
  estado actual.

## Alcance (File Touches)

- `src/main.rs`
- `src/proposal.rs`
- `src/movement/proposal.rs`
- `src/movement/mod.rs`
- `src/movement/motors/*.rs`
- `src/movement/spike.rs`
- `docs/implement-feature/proposal-core-extraction-solutions.md`
- `docs/implement-feature/proposal-core-extraction-plan.md`
- `docs/architecture/movement.md` (solo si el codigo fuerza ajustar el
  contrato ya documentado)
- `docs/architecture/rationale/proposal-arbitration-core.md` (solo si la
  implementacion revela una decision que el rationale todavia no cubre)
- `docs/tickets/proposal-core-extraction.md`
- `src/movement/brain.rs` (solo formato requerido por `cargo fmt`)
- `src/movement/services/ground.rs` (solo formato requerido por `cargo fmt`)
- `src/movement/services/ledge.rs` (solo formato requerido por `cargo fmt`)
- `src/world.rs` (solo formato requerido por `cargo fmt`)

Nada fuera de esta lista sin actualizar esta seccion primero.

## Fuera de alcance

- No migra `Single<..., With<Player>>` a `Query<..., With<Actor>>`; eso es
  `multi-actor-migration`.
- No implementa Combat, Mounts, Enemies ni Multiplayer.
- No cambia la regla de arbitraje de dominio de ningun motor; solo extrae el
  nucleo generico y adapta Movement a usarlo.
- No agrega crates.

## Definicion de terminado

- [x] `src/proposal.rs` existe como nucleo generico interno sin plugin Bevy.
- [x] `proposal::ProposalBuffer<S, N>` usa capacidad fija y no `Vec`.
- [x] `TransitionProposal<S>` usa `override_weight: u32`.
- [x] `push` devuelve `Result<(), ProposalOverflow>`; el overflow no paniquea
      en runtime normal.
- [x] `arbitrate(current)` devuelve `current` cuando no hay propuestas.
- [x] Movement expone sus nombres propios via `src/movement/proposal.rs`
      (`ProposalBuffer`, `TransitionProposal`, `Priority`) sin obligar a los
      motores a conocer estados de otros sistemas.
- [x] Los motores de Movement manejan explicitamente el resultado de `push`.
      Si se decide ignorar un overflow, queda justificado como decision local
      y no con `unwrap()`/`expect()`.
- [x] Tests de invariantes:
      - prioridad gana sobre peso;
      - peso desempata dentro de la misma prioridad;
      - empate conserva el primer candidato;
      - buffer vacio devuelve `current`;
      - overflow devuelve error sin crecer el buffer.
- [x] `cargo fmt` limpio.
- [x] `cargo clippy` sin warnings nuevos.
- [x] `cargo check`/`cargo test` pasa.

## Notas para el agente que lo toma

La documentacion ya eligio el contrato objetivo; no preservar por accidente
el fallback actual a `LocomotionState::Fall` ni el `Vec` actual de Movement.
El objetivo de este ticket es hacer que el codigo alcance ese contrato antes
de que otros sistemas dependan de la forma compartida. (codex)

Si otro worktree toca Movement al mismo tiempo, este ticket deberia mergearse
antes o rebasearse con cuidado, porque el cambio de firma de `arbitrate` es
pequeno pero mecanicamente transversal. (codex)
