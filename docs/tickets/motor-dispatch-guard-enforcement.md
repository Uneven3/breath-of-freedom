# Ticket: `motor-dispatch-guard-enforcement`

## Sistema(s)

Movement (el guard por entidad de cada `tick` y cómo se despachan los 13
motores).

## Contexto / bug

El invariante más importante del pipeline — "exactamente un motor mueve cada
cuerpo por frame" — hoy descansa en una **convención**, no en el compilador ni
en el schedule. Cada `tick` empieza con:

```rust
if *state != LocomotionState::X { continue; }
```

(los tres motores de suelo plano ya lo canalizan por
`motor_common::ground_locomotion_step`, que hace el mismo chequeo). Si un motor
nuevo **olvida** ese guard, o lo escribe con el estado equivocado, dos motores
escriben `Transform`/`BodyVelocity` del mismo cuerpo el mismo frame. No hay
error de compilación ni panic: solo "el juego se siente raro". Es la peor clase
de bug — invisible hasta que se manifiesta como feel, meses después.

Ya existe cobertura parcial:

- `arbitration_matrix` (`src/movement/proposal.rs`) garantiza que cada
  `LocomotionState` tiene **exactamente un** motor dueño y que dos motores no se
  empatan en `(Priority, weight)`. Eso caza "agregué un estado sin dueño" o "dos
  motores reclaman el mismo estado".
- **No** caza "un `tick` se olvidó del guard": eso exige correr el `tick` bajo
  física real, que se intentó y abandonó (ver la nota al final de
  `actor_isolation_tests` en `src/movement/mod.rs`: Avian necesita varios
  sub-plugins que solo arma `DefaultPlugins`).

El costo de este agujero crece con cada motor nuevo (swim, dive, y luego
Combat). Este ticket es para cerrarlo estructuralmente cuando el conteo de
motores lo justifique.

## La decisión de fondo: cómo se representa "el motor activo"

Esto es antes que cualquier mecanismo. La fragilidad del guard es el **precio de
la decisión de representación** ya tomada, y el ECS idiomático ofrece otra:

- **A. Enum-SSoT (actual).** `LocomotionState` es un enum en un componente. La
  exclusividad es gratis (un campo no puede ser dos estados), pero el dispatch es
  un guard runtime que cada `tick` puede olvidar. La fragilidad de este ticket
  **es** el costo de A.
- **B. Marker dispatch (lo más idiomático en ECS).** Un componente marker de
  "motor activo"; cada sistema filtra `Query<..., With<ActiveX>>`. El
  scheduler/archetypes hacen cumplir la exclusividad **estructuralmente** — no
  hay guard que olvidar, y Bevy puede paralelizar motores que no colisionan. Es
  la evolución natural del `run_if` global (`in_loco_state`) que ya reemplazaron
  por guards por entidad: el marker es el equivalente por-entidad de ese
  run condition. Costo: la exclusividad pasa a ser manual (insertar uno, quitar
  el resto) y hay un archetype move por transición. Riesgo: si se conserva el
  enum **además** del marker, hay dos SSoT (viola §6/§7).
- **C. Híbrido: enum SSoT + marker derivado (recomendado a evaluar).** El enum
  sigue siendo la única fuente de verdad legible; el árbitro —que ya es el
  **único escritor**— además intercambia un marker derivado en cada transición
  (solo cuando `*state != winner`, no cada frame), y los motores filtran por el
  marker. Un solo escritor mantiene ambos atómicamente, así que el marker es una
  **vista materializada** del enum, no un segundo hecho independiente (no viola
  §6/§7). Elimina el guard *y* conserva el enum legible. Costo: el archetype move
  por transición — medido abajo.

### Medición §18 (probe descartable, ya corrido)

`marker_dispatch_probe.rs` (throwaway, borrado tras registrar esto) midió el
costo de intercambiar un marker derivado por transición, perfil de test
`optimized + debuginfo`, medición cruda de un solo run (no benchmark riguroso):

| Payload del actor | enum-write | marker-swap | overhead |
|---|---|---|---|
| liviano (solo enum) | ~61 ns | ~363 ns | ~0.30 µs |
| pesado (~512 B) | ~48 ns | ~645 ns | ~0.60 µs |

- El archetype move cuesta **sub-microsegundo** por transición y escala con el
  tamaño del actor (memcpy entre tablas).
- Peor caso irreal (4000 actores transicionando **todos** en un mismo frame de
  60 Hz): ~1.5–2.6 ms de un presupuesto de 16.67 ms. Ya es tolerable, y no es un
  caso real: las transiciones locomotoras ocurren pocas veces por segundo por
  actor, no cada frame. A escala realista (decenas de transiciones por frame) el
  costo es de nanosegundos por frame — despreciable.
- **Conclusión §18: no es bloqueante para C** a la escala esperada. Caveats: el
  probe mutó el `World` directo; vía `Commands` (lo que haría el árbitro como
  sistema) hay algo más de overhead y un sync point. Validar con conteo real de
  actores antes de comprometerse.

## Mecanismos de enforcement (si se decide quedarse en A, enum puro)

Solo relevantes si NO se adopta B/C:

1. **Detector de doble-escritura (debug).** Chequeo `#[cfg(debug_assertions)]`
   en el único punto de escritura (`body_move_and_slide`) que paniquea si la
   misma entidad se mueve dos veces en un tick. Caza **cualquier** guard olvidado
   en el momento exacto de la falla, sin reestructurar los `tick`. Costo:
   hilvanar estado por-frame por la firma del camino de escritura; cero impacto
   en release.
2. **Dispatch tipado genérico.** Trait `Motor` con `const STATE` y un driver
   `tick_motor::<M>`. Enforcement real, pero es la vía **menos idiomática**
   (pelea con genéricos de `SystemParam`); solo tiene sentido si se quiere el
   enum puro *y* enforcement de compilación.
3. **Mantener convención + auditoría (estado actual).** El test de totalidad de
   `arbitration_matrix` + review. Barato, cero riesgo, sigue siendo convención.

Recomendación: **evaluar C** (híbrido enum + marker derivado) como el arreglo
estructural correcto y idiomático; la medición §18 no lo bloquea. Si se descarta
el cambio de representación, el detector de doble-escritura (opción 1 de arriba)
es el mejor enforcement barato para el enum puro.

## Lectura obligatoria, en este orden

1. `docs/CONSTITUTION.md` — completo (§13 fmt/clippy sin warnings silenciados;
   §18 sin allocations en el hot path de `FixedUpdate`).
2. `docs/architecture/rationale/multi-actor-dispatch.md` — por qué el guard por
   entidad reemplazó a los `run_if` globales.
3. `docs/architecture/movement.md` — § Sistemas (fases del Broker) y el rol de
   `motor_common` como único lugar que mueve el cuerpo.
4. `src/movement/mod.rs` — la nota al final de `actor_isolation_tests` sobre por
   qué el `tick` bajo física no está testeado.

## Alcance (File Touches)

- `src/movement/motor_common.rs` (el driver/detector central)
- `src/movement/motors/*.rs` (según la opción: relocalizar el guard o instrumentar
  el call site de escritura)
- `src/movement/mod.rs` (registro de sistemas si cambia el dispatch)
- `docs/architecture/movement.md` y
  `docs/architecture/rationale/multi-actor-dispatch.md`
- `docs/tickets/motor-dispatch-guard-enforcement.md` (este archivo)

## Fuera de alcance

No cambiar el modelo de arbitraje (`propose`/`arbitrate`/`(Priority, weight)`).
No convertir `LocomotionState` en concurrente. No tocar la tabla
`proposal::weight`.

## Definición de terminado

- [ ] Un motor nuevo que olvide su guard es **cazado** por la máquina (compilación
      en la opción 1, panic de debug en la opción 2), no por playtest.
- [ ] El comportamiento de los 13 motores existentes queda idéntico
      (validado jugando — checkpoint §10, porque es hot-path).
- [ ] `arbitration_matrix` sigue verde.
- [ ] `cargo fmt` / `cargo clippy --all-targets -- -D warnings` limpios;
      `cargo test` pasa.
- [ ] `docs/architecture/movement.md` actualizado.
