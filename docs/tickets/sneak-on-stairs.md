# Ticket: `sneak-on-stairs`

## Estado: RESUELTO (opción 3) — pendiente de playtest

Se implementó la **opción 3 (crouch como modificador ortogonal)**, que es la
raíz correcta y también destraba `jump-while-crouched-under-ceiling`:

- `sync_sneak_collider` → `sync_crouch_collider`: la cápsula agachada ahora sigue
  la intención de crouch (o un stand-up bloqueado) durante *cualquier* estado de
  locomoción de suelo (`Walk`/`Sprint`/`Sneak`/`Stairs`), vía
  `is_ground_locomotion`, en vez de `state == Sneak`. Corre cada frame tras
  `Arbitrate` (el crouch cambia con la intención, no con la transición de estado).
- `stairs::tick` respeta el crouch: usa la media-altura agachada para el snap por
  peldaño (evita jitter/float por el desfase de cápsula) y el nuevo
  `StairsLocomotion::sneak_multiplier` (0.5) para la velocidad.
- El `Sneak` plano queda idéntico: `sneak::propose` no cambió, y la cápsula se
  agacha igual que antes cuando `gait == Sneak` en suelo plano.
- Sin cambios en la matriz de arbitraje: Stairs (`Forced`) sigue ganando sobre
  Sneak (`PlayerRequested`); el crouch ya no depende de ganar ese arbitraje.

Falta el checkpoint §10: **jugar** Sneak plano + Sneak en escaleras (subir/bajar,
lateral) + techo sobre escaleras, y confirmar que no hay jitter ni regresión del
Sneak plano.

## Sistema(s)

Movement (arbitraje Sneak vs Stairs, y si "agacharse" es un estado locomotor
o un modificador ortogonal del suelo).

## Contexto / bug

Agacharse en una escalera no hace nada. Diagnóstico exacto:

- `stairs::propose` empuja `(Forced, weight::STAIRS)` cuando `on_stairs` y
  (grounded o ya pegado a Stairs).
- `sneak::propose` empuja `(PlayerRequested, weight::SNEAK)`.
- `Forced > PlayerRequested`, así que **Stairs siempre gana**: estando en
  escaleras nunca se entra a `Sneak`.
- Además `stairs::tick` solo mira `GaitIntent::Sprint` (velocidad); ignora por
  completo `GaitIntent::Sneak`. No hay perfil agachado ni cápsula agachada en
  escaleras.

Resultado observable: sobre una escalera, el botón de agacharse no cambia ni la
velocidad ni la altura de la cápsula.

Causa raíz (más profunda que el arbitraje): hoy "agacharse" está modelado como
un **estado** locomotor (`Sneak`), par de `Walk`/`Sprint`/`Stairs`, y los
estados son mutuamente excluyentes por diseño (`state.rs`, Constitución §6/§7).
Pero agacharse es en realidad un **modificador ortogonal** que debería componer
con la locomoción de suelo (caminar-agachado, escalera-agachado). Es el mismo
acoplamiento estado-vs-forma-física que ya documenta
[`jump-while-crouched-under-ceiling`](jump-while-crouched-under-ceiling.md):
`sync_sneak_collider` cambia la cápsula según `state == Sneak`, no según una
señal de crouch independiente.

## Opciones (decidir con playtest, no de antemano)

1. **Stairs consciente de Sneak (parche acotado).** `stairs::tick` respeta
   `GaitIntent::Sneak` igual que ya respeta `Sprint`: usa el perfil de
   velocidad agachado y aplica la cápsula agachada mientras esté en escaleras.
   Ojo: aplicar la cápsula agachada con `state == Stairs` exige desacoplar
   `Crouched`/collider de `LocomotionState::Sneak` — es decir, arrastra la misma
   decoupling que la opción 3. Sin eso, agachar en escalera cambiaría velocidad
   pero no altura.
2. **Sneak gana sobre Stairs.** Bajar la prioridad de Stairs o subir la de
   Sneak para poder agacharse normal en escaleras. Se pierde el Y-snap por
   escalón (el motor de Stairs existe justo para eso), así que probablemente
   produce clipping/jitter en los peldaños. Candidata a descartar.
3. **Crouch como modificador ortogonal (arreglo de fondo, recomendado).**
   Introducir un `CrouchIntent` (o un flag `Crouched` derivado de intent +
   `StandClearance`) que sea **independiente** del `LocomotionState`, y que cada
   motor de suelo (Walk, Sprint→bloqueado, Stairs, quizá Ladder) consuma para
   elegir perfil y cápsula. Esto cierra este bug **y**
   `jump-while-crouched-under-ceiling` de una vez, y encaja mejor con "la IA usa
   los mismos intents". Es la más grande: toca `intents.rs`, `sneak.rs`,
   `stairs.rs`, `sync_sneak_collider`, y posiblemente el rol de
   `LocomotionState::Sneak`.

Recomendación: no parchar a ciegas. La opción 3 es la correcta de fondo; la
opción 1 es el mínimo si solo se quiere "agacharse en escalera" ya, pero como
igual arrastra la decoupling de la cápsula, conviene evaluar hacer 3 directo.
Decidir el comportamiento deseado jugando antes de escribir código.

## Lectura obligatoria, en este orden

1. `docs/CONSTITUTION.md` — completo (especial atención a §6/§7: un dueño por
   hecho; estados exclusivos como enum, no como flags).
2. `docs/ARCHITECTURE-MAP.md` — fila `Movement`.
3. `docs/architecture/movement.md` — § Sistemas y el arbitraje por
   `(Priority, weight)`; el módulo `arbitration_matrix` en
   `src/movement/proposal.rs` documenta la matriz completa.
4. `docs/tickets/sneak-stand-clearance.md` y
   `docs/tickets/jump-while-crouched-under-ceiling.md` — la garantía de
   clearance y el bug hermano con la misma causa raíz.

## Alcance (File Touches)

- `src/movement/motors/stairs.rs` y `src/movement/motors/sneak.rs`
- `src/movement/intents.rs` (si se añade `CrouchIntent`, opción 3)
- `src/movement/state.rs` (solo si la opción 3 replantea `LocomotionState::Sneak`)
- `src/movement/proposal.rs` (fila del `arbitration_matrix` si cambian emisiones)
- `docs/architecture/movement.md` (si el diseño documentado cambia)
- `docs/tickets/sneak-on-stairs.md` (este archivo)

## Fuera de alcance

No rediseñar el motor de Stairs (Y-snap por peldaño) más allá de aceptar el
perfil/cápsula agachados. No tocar la tabla `proposal::weight` de otros motores
ni el abstain de Sprint (ya resueltos).

## Definición de terminado

- [ ] Agacharse en una escalera produce el comportamiento decidido en playtest
      (velocidad agachada y, si aplica, cápsula agachada) sin romper el Y-snap
      por peldaño ni clippear en los riscos (checkpoint §10).
- [ ] Si se elige la opción 3: la cápsula agachada la maneja una señal de crouch
      independiente del `LocomotionState`, y el `arbitration_matrix` sigue verde.
- [ ] Test de invariante acorde a la decisión (Constitución §11).
- [ ] `cargo fmt` / `cargo clippy --all-targets -- -D warnings` limpios;
      `cargo test` pasa.
- [ ] `docs/architecture/movement.md` actualizado si el diseño cambió.
