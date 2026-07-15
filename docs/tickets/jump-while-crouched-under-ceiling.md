# Ticket: `jump-while-crouched-under-ceiling`

## Sistema(s)

Movement (arbitraje Sneak vs Jump y el swap de cápsula de `sync_sneak_collider`).

## Contexto / bug

`StandClearance` garantiza que un actor agachado bajo un techo no vuelve a
la cápsula de pie hasta que quepa: Sneak sigue proponiendo
(`must_remain_crouched`) aunque se suelte el botón, y Sprint ya se abstiene
en ese caso (arreglado junto con la tabla `proposal::weight`).

Queda un agujero: **Jump**. Con el actor agachado bajo un techo bajo,
`jump::propose` gana el arbitraje (`Forced` > `PlayerRequested`) y el estado
sale a `Jump`. Como `Jump` no es locomoción de suelo,
`sync_crouch_collider` restaura la cápsula de pie **dentro del techo** —
exactamente lo que `StandClearance` existe para impedir. El resultado
observable es overlap/jitter contra el techo al pulsar salto agachado.

Nota: tras el refactor de `sneak-on-stairs`, el crouch es un modificador
ortogonal manejado por `sync_crouch_collider` (ya no `sync_sneak_collider`), con
`want_crouch = is_ground_locomotion(state) && (gait==Sneak || must_remain_crouched)`.
El agujero de Jump ahora es puntual: el `is_ground_locomotion(state)` deja caer el
crouch en cuanto el estado es `Jump`, ignorando `must_remain_crouched`.

## Opciones (decidir con playtest, no de antemano)

1. `jump::propose` se abstiene mientras `Crouched && !StandClearance`
   (espejo del abstain de Sprint). Ojo: Jump es una capacidad separada de
   Ground — un actor con `JumpMovement` sin `GroundMovement` no tiene
   `StandClearance`, así que la query necesitaría `Option<&…>` o el ticket
   define que Jump-bajo-techo requiere ambas capacidades.
2. **(Ahora la más simple)** `sync_crouch_collider` respeta
   `must_remain_crouched` aunque el estado no sea de suelo: p. ej.
   `want_crouch = must_remain_crouched || (is_ground_locomotion(state) && gait==Sneak)`.
   `update_stand_clearance` ya prueba la cápsula estando el actor agachado sin
   importar `grounded`, así que el crouch se sostendría en el aire bajo el techo
   hasta que haya clearance. No introduce segundo SSoT: `Crouched` sigue siendo
   forma física derivada por un único sistema (revisar §6/§7 igual).

## Lectura obligatoria, en este orden

1. `docs/CONSTITUTION.md` — completo.
2. `docs/ARCHITECTURE-MAP.md` — fila `Movement`.
3. `docs/architecture/movement.md` (§ Sistemas, párrafo de
   `sync_sneak_collider` / `update_stand_clearance`).
4. `docs/tickets/sneak-stand-clearance.md` — el ticket que introdujo la
   garantía que este bug rompe.

## Alcance (File Touches)

- `src/movement/motors/jump.rs` y/o `src/movement/motors/sneak.rs`
- `src/movement/mod.rs` (solo si cambia ordering/registro)
- `docs/architecture/movement.md` (si la decisión cambia el diseño documentado)
- `docs/tickets/jump-while-crouched-under-ceiling.md` (este archivo)

## Fuera de alcance

No tocar la tabla `proposal::weight` ni el abstain de Sprint (ya resueltos).
No rediseñar el swap declarativo de collider más allá de lo que exija la
opción elegida.

## Definición de terminado

- [ ] Saltar agachado bajo un techo sin clearance no expande la cápsula
      dentro de la geometría (validado jugando — checkpoint §10).
- [ ] Test de invariante: con `Crouched && !StandClearance`, el arbitraje
      no produce un estado que dispare la restauración de la cápsula de pie
      (Constitución §11, excepción de arquitectura).
- [ ] `cargo fmt` / `cargo clippy` limpios; `cargo test` pasa.
- [ ] `docs/architecture/movement.md` actualizado si el diseño cambió.
