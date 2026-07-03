# Solutions: proposal-core-extraction

## Contexto

Modo: PROPOSE  
Slug: `proposal-core-extraction`  
Critica previa: none

El ticket debe extraer el nucleo generico de arbitracion a `src/proposal.rs`,
adaptar Movement sin cambiar su API publica de nombres propios, reemplazar el
`Vec` del hot path por capacidad fija y mantener fuera de alcance
`multi-actor-migration`, Combat, Mounts, Enemies y Multiplayer.

## Solution 1: Core Generico Con Type Aliases Directos

### Approach

Crear `src/proposal.rs` con `Priority`, `TransitionProposal<S>`,
`ProposalBuffer<S, const N: usize>` y `ProposalOverflow`, usando slots de
capacidad fija y `len`. En `src/movement/proposal.rs`, convertir los nombres
publicos de Movement en aliases: `ProposalBuffer =
crate::proposal::ProposalBuffer<LocomotionState, MOVEMENT_PROPOSAL_CAPACITY>`
y `TransitionProposal = crate::proposal::TransitionProposal<LocomotionState>`,
re-exportando `Priority`. Adaptar los motores para llamar `push(...)` y manejar
el `Result` localmente, y cambiar `movement::arbitrate` a
`buffer.arbitrate(*state)` seguido de `buffer.clear()`.

### CONSTITUTION clauses at risk

- §3: los aliases publicos deben preservar el contrato esperado por los
  motores de Movement y por futuros usuarios de Combat/Mounts.
- §8 y §9: el overflow no puede resolverse con `unwrap()`/`expect()` ni panic
  en runtime normal.
- §18: el buffer fijo no debe esconder allocations en `FixedUpdate`.
- §19: `src/proposal.rs` debe quedar como datos/algoritmo generico, sin mezclar
  reglas concretas de Movement.

### Tradeoffs

Pros:
- Es la forma mas cercana al contrato ya documentado en
  `rationale/proposal-arbitration-core.md`.
- Reduce duplicacion: los tests centrales de prioridad, desempate, vacio y
  overflow viven donde esta el algoritmo compartido.
- Deja Combat y Mounts preparados para usar el mismo nucleo sin depender de
  Movement.

Cons:
- Requiere una edicion mecanica amplia porque todos los `.0.push(...)` y
  `.0.clear()` de Movement deben cambiar.
- Los tests existentes que inspeccionan `ProposalBuffer.0.clone()` dejan de
  compilar y deben pasar por una API explicita de iteracion o snapshot.
- Los type aliases no permiten implementar traits propios en el alias si mas
  adelante Movement necesita comportamiento especifico del componente.

### Edge cases

- Buffer vacio durante un frame por un nuevo motor mal guardado: `arbitrate`
  debe conservar el `LocomotionState` actual, no forzar `Fall`.
- Dos propuestas con misma prioridad y peso: debe ganar la primera insertada
  para preservar estabilidad.
- Capacidad agotada por demasiados motores proponiendo el mismo frame: `push`
  devuelve `ProposalOverflow` y el buffer no crece ni pierde propuestas ya
  aceptadas.

## Solution 2: Core Generico Con Newtype Publico De Movement

### Approach

Crear el mismo `src/proposal.rs` generico, pero mantener
`movement::proposal::ProposalBuffer` como newtype component propio que contiene
`crate::proposal::ProposalBuffer<LocomotionState, MOVEMENT_PROPOSAL_CAPACITY>`.
`movement::proposal.rs` expone metodos `push`, `arbitrate(current)`, `clear` e
`iter` que delegan al nucleo, mientras `TransitionProposal` y `Priority` se
re-exportan o se aliasan para conservar nombres publicos propios. Los motores
se adaptan a `buffer.push(...)`, pero otros detalles internos del buffer quedan
encapsulados tras el newtype de Movement.

### CONSTITUTION clauses at risk

- §1: el wrapper de Movement no debe empezar a contener reglas de dominio ni
  duplicar la arbitracion del nucleo.
- §3: el newtype debe comportarse igual que el nucleo compartido para no crear
  sorpresas entre Movement, Combat y Mounts.
- §4: la API del wrapper debe seguir chica; solo exponer lo que los motores y
  tests necesitan.
- §18: el wrapper no debe reintroducir `Vec` ni snapshots con allocation en el
  camino caliente.

### Tradeoffs

Pros:
- Mantiene una identidad de tipo real para el componente de Movement, no solo
  un alias generico.
- Permite adaptar tests y motores con una API mas expresiva (`push`, `clear`,
  `iter`) sin exponer slots internos.
- Deja espacio para instrumentacion local de Movement ante overflow sin tocar
  el nucleo compartido.

Cons:
- Agrega una capa de delegacion que puede ser innecesaria para un contrato ya
  definido como alias en la arquitectura.
- Combat y Mounts podrian terminar copiando wrappers equivalentes, aumentando
  boilerplate si no se mantiene disciplina.
- Hay mas superficie para divergencia accidental si alguien agrega logica al
  wrapper en vez de al motor correspondiente.

### Edge cases

- Un test necesita verificar el contenido propuesto por `climb` o `edge_leap`:
  debe usar `iter()` o un helper de test sin clonar un `Vec` del componente.
- Overflow dentro de un motor opcional: el wrapper puede devolver el error y
  el motor decide ignorarlo explicitamente o registrarlo como contrato roto.
- `arbitrate(current)` llamado despues de `clear()`: debe devolver `current`
  sin depender de `LocomotionState::default()`.

## Solution 3: Extraccion En Dos Capas Con Adaptador Temporal De Push

### Approach

Crear `src/proposal.rs` con el contrato final, y en
`src/movement/proposal.rs` ofrecer una API de migracion minima que conserva
constructores y nombres publicos pero fuerza a los motores a pasar por un
helper local, por ejemplo `push_transition(&mut ProposalBuffer,
TransitionProposal)`. La implementacion interna ya usa capacidad fija,
`override_weight: u32` y `arbitrate(current)`, pero la migracion mecanica de
call sites se concentra en reemplazar `.0.push(...)` por el helper en todos
los motores y en `spike.rs`. Una vez compilado, los tests centrales se ubican
en `src/proposal.rs` y los tests de Movement solo cubren integracion y nombres
publicos.

### CONSTITUTION clauses at risk

- §2: el adaptador temporal no debe convertirse en una segunda ruta permanente
  de arbitraje.
- §4: el helper debe ser deliberadamente pequeno y no exponer internals del
  buffer fijo.
- §13: al tocar muchos call sites mecanicos, `fmt`, `clippy`, `check` y tests
  son importantes para no dejar warnings ni rutas sin migrar.
- §18: los helpers de test no deben usarse desde `FixedUpdate` si asignan
  memoria para inspeccion.

### Tradeoffs

Pros:
- Baja el riesgo de una migracion transversal porque todos los call sites usan
  una forma uniforme.
- Facilita revisar que cada motor manejo el resultado de `push` de manera
  explicita.
- Permite retirar el adaptador en una limpieza posterior si el equipo decide
  estandarizar en `buffer.push(...)`.

Cons:
- Introduce una API intermedia que no esta en el contrato objetivo y puede
  necesitar otra edicion posterior.
- Puede ocultar demasiado la decision local de overflow si el helper decide
  por todos los motores.
- Es menos directa para Combat/Mounts, porque ellos probablemente no deberian
  copiar el helper de Movement.

### Edge cases

- Un motor existente propone dos veces en el mismo frame: el helper debe
  preservar orden de insercion para que los empates sigan siendo estables.
- `spike.rs` usa el mismo contrato publico para pruebas de arquitectura: debe
  migrar al helper o al nuevo `push` sin quedar atado a internals.
- Si la capacidad elegida queda corta, el overflow debe ser observable por el
  llamador y no degradar silenciosamente a un estado incorrecto.

## Chosen Solution

Solution 1: Core Generico Con Type Aliases Directos.

Rationale: es la opcion mas alineada con
`docs/architecture/rationale/proposal-arbitration-core.md` y con el mapa de
arquitectura, que ya define Movement, Combat y Mounts como pares que comparten
un contrato generico de arbitraje sin depender de internals de Movement. El
type alias conserva los nombres publicos propios de Movement mientras mueve la
logica comun a `src/proposal.rs`, elimina el `Vec` del hot path y deja a
Combat/Mounts listos para adoptar el mismo nucleo despues sin tocar este
ticket.
