# Implementation Plan: proposal-core-extraction

## Contexto

Modo: PLAN  
Slug: `proposal-core-extraction`  
Solucion elegida: Solution 1: Core Generico Con Type Aliases Directos  
Required Improvements: none

El objetivo es extraer el algoritmo generico de propuestas a `src/proposal.rs`,
mantener los nombres publicos de Movement mediante type aliases, reemplazar el
`Vec` por un buffer de capacidad fija y no incluir `multi-actor-migration`,
Combat, Mounts, Enemies ni Multiplayer.

## File Touches

- `docs/implement-feature/proposal-core-extraction-solutions.md`: registrar
  la solucion elegida.
- `docs/implement-feature/proposal-core-extraction-plan.md`: checklist y
  Fidelity Check de la implementacion.
- `docs/tickets/proposal-core-extraction.md`: mantener sincronizado el alcance
  real del ticket.
- `src/main.rs`: declarar el modulo raiz `proposal`.
- `src/proposal.rs`: crear el nucleo generico con `Priority`,
  `TransitionProposal<S>`, `ProposalBuffer<S, N>` y `ProposalOverflow`.
- `src/movement/proposal.rs`: reemplazar la implementacion local por aliases
  publicos de Movement y la constante `MOVEMENT_PROPOSAL_CAPACITY`.
- `src/movement/mod.rs`: cambiar arbitraje y limpieza al contrato nuevo.
- `src/movement/spike.rs`: migrar usos de `.0.push`, `.0.clear` y
  `arbitrate()` al contrato nuevo.
- `src/movement/motors/*.rs`: migrar todos los productores de propuestas y los
  helpers de test que inspeccionan o limpian el buffer.
- `src/movement/brain.rs`: toque solo de formato requerido por `cargo fmt`.
- `src/movement/services/ground.rs`: toque solo de formato requerido por
  `cargo fmt`.
- `src/movement/services/ledge.rs`: toque solo de formato requerido por
  `cargo fmt`.
- `src/world.rs`: toque solo de formato requerido por `cargo fmt`.

## Core Logic Flow

1. Registrar `mod proposal;` en `src/main.rs` para que el nucleo compartido sea
   accesible como `crate::proposal`.
2. Crear `src/proposal.rs` con `Priority` como enum publico ordenable:
   `Default`, `PlayerRequested`, `Opportunistic`, `Forced`, derivando
   `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `PartialOrd` y `Ord`.
3. Crear `TransitionProposal<S>` con campos publicos `target_state: S`,
   `category: Priority`, `override_weight: u32` y
   `source_id: &'static str`, derivando `Debug`, `Clone`, `Copy`,
   `PartialEq` y `Eq`.
4. Implementar `TransitionProposal<S>::new(target_state, category,
   override_weight, source_id)` sin conversiones ni allocations.
5. Crear `ProposalOverflow` como error publico, copiable y comparable, con los
   datos minimos para identificar capacidad agotada y `source_id`.
6. Crear `ProposalBuffer<S, const N: usize>` como `#[derive(Component)]` con
   `slots: [Option<TransitionProposal<S>>; N]` y `len: usize`.
7. Implementar `Default` de `ProposalBuffer` con `std::array::from_fn(|_| None)`
   para no requerir `TransitionProposal<S>: Copy` al construir el arreglo.
8. Implementar `push(&mut self, proposal)` para aceptar si `len < N`, escribir
   en `slots[len]`, incrementar `len` y devolver `Ok(())`.
9. Implementar `push(&mut self, proposal)` para devolver
   `Err(ProposalOverflow)` si `len == N`, sin crecer, sin borrar propuestas ya
   aceptadas y sin panics.
10. Implementar `iter(&self)` sobre `slots[..len]` para exponer solo propuestas
    aceptadas y permitir que tests de Movement dejen de clonar el `Vec`.
11. Implementar `clear(&mut self)` para poner en `None` solo los slots
    ocupados `0..len` y luego resetear `len` a `0`.
12. Implementar `arbitrate(&self, current: S) -> S` bajo bound `S: Copy` para
    devolver `current` si no hay propuestas.
13. Implementar `arbitrate(&self, current: S) -> S` bajo bound `S: Copy` para
    recorrer propuestas en orden de insercion y elegir la de mayor
    `(category, override_weight)`.
14. Implementar el desempate de `arbitrate` dejando como ganador el primer
    elemento insertado cuando `category` y `override_weight` empatan.
15. Agregar tests unitarios en `src/proposal.rs` para buffer vacio conserva
    estado actual, mayor categoria gana, mayor peso desempata dentro de la
    misma categoria, empate exacto conserva el primero y overflow no cambia el
    contenido aceptado.
16. Reemplazar `src/movement/proposal.rs` por aliases publicos:
    `pub use crate::proposal::Priority`,
    `pub type TransitionProposal = crate::proposal::TransitionProposal<LocomotionState>`
    y
    `pub type ProposalBuffer = crate::proposal::ProposalBuffer<LocomotionState, MOVEMENT_PROPOSAL_CAPACITY>`.
17. Definir `MOVEMENT_PROPOSAL_CAPACITY: usize = 32` en
    `src/movement/proposal.rs` para cubrir con margen los productores actuales
    sin introducir una dependencia nueva.
18. En todos los motores de Movement y en `spike.rs`, reemplazar cada
    `buffer.0.push(TransitionProposal::new(...))` por
    `let _ = buffer.push(TransitionProposal::new(...));`, dejando que overflow
    descarte solo la propuesta nueva y preservando las ya aceptadas.
19. En `src/movement/mod.rs` y `src/movement/spike.rs`, cambiar
    `buffer.arbitrate()` por `buffer.arbitrate(*state)` y despues llamar
    `buffer.clear()`.
20. Reemplazar cualquier `buffer.0.clear()` de tests o sistemas por
    `buffer.clear()`.
21. Reemplazar helpers de test que retornaban `ProposalBuffer.0.clone()` por
    colecciones construidas desde `buffer.iter().copied().collect()` o por
    asserts directos sobre `iter()`.
22. Cambiar pesos de propuesta existentes de enteros `i32` a `u32` en call
    sites, preservando todos los pesos/constantes no negativos actuales bajo
    `u32`.
23. No editar contratos de Combat, Mounts, Enemies, Multiplayer ni
    `multi-actor-migration`; este ticket solo deja el nucleo listo para que
    esos sistemas lo adopten despues.
24. Ejecutar `cargo fmt`, `cargo check`, `cargo clippy` y `cargo test` desde la
    raiz del repo.

## Pre-implementation Checklist

- [x] `docs/CONSTITUTION.md` leido.
- [x] `docs/ARCHITECTURE-MAP.md` leido.
- [x] Plan revisado contra Solution 1.
- [x] File Touches respetan el alcance del ticket.
- [x] Required Improvements integradas: none.

### Fidelity Check

| Step | Location | Notes |
| :--- | :--- | :--- |
| Step 1 | `src/main.rs:11` | Declara `mod proposal;` para exponer `crate::proposal`. |
| Step 2 | `src/proposal.rs:5` | Define `Priority` publico, ordenable y copiable. |
| Step 3 | `src/proposal.rs:13` | Define `TransitionProposal<S>` con `override_weight: u32` y campos publicos. |
| Step 4 | `src/proposal.rs:21` | Implementa constructor sin conversiones ni allocations. |
| Step 5 | `src/proposal.rs:37` | Define `ProposalOverflow` copiable/comparable con capacidad y `source_id`. |
| Step 6 | `src/proposal.rs:43` | Define `ProposalBuffer<S, N>` como `Component` con slots fijos y `len`. |
| Step 7 | `src/proposal.rs:49` | `Default` usa `std::array::from_fn(|_| None)`. |
| Step 8 | `src/proposal.rs:59` | `push` acepta propuestas dentro de capacidad y avanza `len`. |
| Step 9 | `src/proposal.rs:60` | `push` devuelve `ProposalOverflow` al llenarse sin borrar slots existentes. |
| Step 10 | `src/proposal.rs:72` | `iter` expone solo propuestas aceptadas. |
| Step 11 | `src/proposal.rs:76` | `clear` limpia `0..len` y resetea `len`. |
| Step 12 | `src/proposal.rs:84` | `arbitrate(current)` devuelve `current` si no hay propuestas. |
| Step 13 | `src/proposal.rs:87` | `arbitrate` recorre en orden e intenta mejorar por `(category, weight)`. |
| Step 14 | `src/proposal.rs:96` | El empate exacto conserva el ganador previo, por lo tanto el primero insertado. |
| Step 15 | `src/proposal.rs:118` | Tests cubren vacio, prioridad, peso, empate y overflow. |
| Step 16 | `src/movement/proposal.rs:3` | Movement reexporta `Priority` y define aliases publicos propios. |
| Step 17 | `src/movement/proposal.rs:5` | Define `MOVEMENT_PROPOSAL_CAPACITY: usize = 32`. |
| Step 18 | `src/movement/motors/walk.rs:27` | Motores y spike usan `let _ = buffer.push(...)`; `rg` confirma todos los productores. |
| Step 19 | `src/movement/mod.rs:160` | Movement llama `buffer.arbitrate(**state)` y limpia con `buffer.clear()`. |
| Step 20 | `src/movement/spike.rs:191` | No quedan `buffer.0.clear()`; spike limpia con `buffer.clear()`. |
| Step 21 | `src/movement/motors/climb.rs:201` | Helpers de test leen `buffer.iter().copied().collect()` en vez de clonar `Vec`. |
| Step 22 | `src/proposal.rs:17` | Pesos pasan por `u32`; call sites existentes preservan todos los pesos/constantes no negativos actuales. |
| Step 23 | `docs/implement-feature/proposal-core-extraction-plan.md:90` | El diff no modifica Combat, Mounts, Enemies, Multiplayer ni multi-actor. |
| Step 24 | `docs/tickets/proposal-core-extraction.md:78` | `cargo fmt --check`, `cargo check`, `cargo clippy` y `cargo test` pasan. |
