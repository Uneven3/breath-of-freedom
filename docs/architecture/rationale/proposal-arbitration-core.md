# Rationale: núcleo genérico de arbitración de propuestas (codex)

## El problema

Movement, Combat y Mounts necesitan la misma regla: varios productores
proponen transiciones, y un árbitro elige ganador por
`(category, override_weight)`. La regla común no pertenece semánticamente a
Movement, Combat ni Mounts; pertenece a un núcleo interno compartido.

## Por qué se comparte

La forma común ya está definida por tres contratos de arquitectura:
`movement::ProposalBuffer`, `combat::CombatProposalBuffer` y
`mounts::MountProposalBuffer`. Compartir el algoritmo evita que sistemas
pares diverjan en reglas de prioridad. La abstracción se mantiene chica:
estructura de propuesta, prioridad y selección de ganador. Cada sistema
conserva sus tipos públicos propios.

## La decisión

Un módulo nuevo en la raíz del crate, `src/proposal.rs`, sin plugin propio
(no es un sistema, es una librería interna compartida — Constitución §14
aplica a sistemas de gameplay, no a esto):

```rust
pub enum Priority { Default, PlayerRequested, Continuation, Forced }

pub struct TransitionProposal<S> {
    pub target_state: S,
    pub category: Priority,
    pub override_weight: u32,
    pub source_id: &'static str,
}

pub struct ProposalBuffer<S, const N: usize> {
    slots: [Option<TransitionProposal<S>>; N],
    len: usize,
}

impl<S: Copy + PartialEq, const N: usize> ProposalBuffer<S, N> {
    pub fn push(&mut self, proposal: TransitionProposal<S>) -> Result<(), ProposalOverflow>;
    pub fn arbitrate(&self, current: S) -> S;
    pub fn clear(&mut self);
}
```

El buffer es de capacidad fija por sistema. No usa `Vec` ni crece en
`FixedUpdate`, para cumplir Constitución §18. Si se llena, `push` retorna
`ProposalOverflow`; el llamador decide si lo trata como bug de contrato,
telemetría o fallback explícito. `arbitrate(current)` devuelve `current` si
no hay propuestas, evitando estados dummy, `unwrap()` o panic en caso vacío
(Constitución §8/§9).

Cada sistema sigue teniendo su **propio tipo concreto y con nombre propio**
(`movement::ProposalBuffer = proposal::ProposalBuffer<LocomotionState, N>`,
`combat::CombatProposalBuffer = proposal::ProposalBuffer<CombatState, N>`)
vía type alias — nadie de afuera ve un tipo genérico compartido, solo el
nombre de su propio sistema. Esto preserva el aislamiento ya decidido
(`combat.md`: "aislar por sistema, no por instancia compartida") mientras
elimina la duplicación de la lógica de arbitración en sí.

## Por qué genérico y no un trait object

Rendimiento (Constitución §18, sin allocations en el hot path de
`FixedUpdate`): un genérico monomorphiza en tiempo de compilación al mismo
código que dos copias manuales, sin *boxing*, *dynamic dispatch* ni
crecimiento de heap.

## Por qué en la raíz del crate y no dentro de `movement::`

Si viviera en `movement::proposal`, Combat y Mounts tendrían que depender de
un módulo interno de Movement para algo que no es semánticamente de
Movement — invierte la dirección de dependencia (§5) y contradice que los
tres sistemas son pares, no que dos dependen del primero en implementarse.

## Restricción de contrato

El núcleo compartido no puede conocer estados concretos ni reglas de dominio.
Solo ordena propuestas. La validez física de una transición sigue viviendo
en el sistema dueño del estado (`Movement`, `Combat` o `Mounts`). (codex)

El orden del enum es monotónico en compromiso: un fallback pierde contra un
pedido fresco del jugador, que pierde contra continuar una maniobra en curso
(`Continuation`, ex-`Opportunistic`), que pierde contra movimiento
comprometido (`Forced`). Los pesos de desempate dentro de una categoría son
conocimiento de dominio de cada sistema: Movement centraliza los suyos en
`movement::proposal::weight`, con el orden total fijado por `const` asserts
en tiempo de compilación.

## Reconciliar con el `ProposalBuffer` real de Movement (resuelto por el ticket de extracción)

`src/movement/proposal.rs` (código real, pre-extracción) difiere de este
diseño en dos puntos concretos — el ticket `proposal-core-extraction`
(`WORKTREE-WORKFLOW.md`) debe decidirlos a propósito, no heredarlos por
accidente al mover el código:

1. **`override_weight: i32` (real) vs `u32` (acá).** Ningún motor actual
   usa un peso negativo (grep sobre los 19 call sites de
   `TransitionProposal::new`: todos los valores son `0`, `1` o `5`) —
   adoptar `u32` es seguro y no pierde nada.
2. **Fallback en buffer vacío: hardcodea `LocomotionState::Fall` (real,
   `.unwrap_or(LocomotionState::Fall)`) vs `arbitrate(current)` devuelve
   `current` (acá).** En la práctica el buffer de Movement nunca llega
   vacío hoy — `walk::propose` cubre "grounded" a `PlayerRequested` y
   `fall::propose` cubre "no grounded" a `Default`, mutuamente excluyentes
   por `ground.grounded`, así que siempre hay al menos una propuesta. El
   hardcode a `Fall` es código muerto en la práctica, pero **no** es
   equivalente a `return current` si algún día ese supuesto deja de
   cumplirse (ej. un nuevo motor con una condición de guarda que se le
   escapa un frame) — `return current` es estructuralmente más seguro
   (nunca fuerza un estado arbitrario) y es lo que ya van a exigir
   Combat/Mounts al compartir el núcleo. Adoptar `current` al extraer,
   no preservar el hardcode a `Fall`.
