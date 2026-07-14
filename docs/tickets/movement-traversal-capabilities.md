## Ticket: `movement-traversal-capabilities`

## Sistema(s)

Movement. Migra las capacidades persistentes de traversal ya validadas para
que sus motores seleccionen actores compatibles y lean tuning por actor, sin
cambiar sus reglas de transicion ni el feeling actual.

## Lectura obligatoria, en este orden

1. `docs/CONSTITUTION.md`
2. `docs/ARCHITECTURE-MAP.md` (fila Movement)
3. `docs/architecture/movement.md`
4. `docs/architecture/rationale/movement-capability-composition.md`
5. `docs/WORKING-CONTEXT.md`

## Decision de batch

El usuario solicito probar juntas las migraciones de traversal estable. Este
ticket las agrupa solo porque son cambios mecanicos de configuracion: todos
los valores Player deben mantenerse exactamente iguales y cada motor conserva
sus contratos actuales. No agrupa cambios de comportamiento ni nuevos
disenos.

| Capacidad | Motores incluidos |
|---|---|
| `ClimbMovement` | Climb |
| `LadderMovement` | Ladder |
| `LedgeTraversal` | Mantle, AutoVault |
| `WallJumpMovement` | WallJump, EdgeLeap |

## Alcance (File Touches)

- `src/movement/abilities.rs`
- `src/movement/mod.rs`
- `src/movement/motors/climb.rs`
- `src/movement/motors/ladder.rs`
- `src/movement/motors/mantle.rs`
- `src/movement/motors/auto_vault.rs`
- `src/movement/motors/wall_jump.rs`
- `src/movement/motors/edge_leap.rs`
- `docs/architecture/movement.md`
- `docs/architecture/rationale/movement-capability-composition.md`
- `docs/tickets/movement-traversal-capabilities.md`
- `docs/WORKING-CONTEXT.md`

## Fuera de alcance

- No cambia `LocomotionState`, propuestas, prioridades, arbitraje, sensores o
  el orden del schedule.
- No habilita WallJump estilo Metroid ni introduce memoria de contacto.
- No migra Jump, Glide o Stairs: aun requieren sus propios disenos de
  capacidad.
- No crea actores nuevos, IA, red ni monturas.
- No modifica ningun valor del Player ni el feeling validado.

## Definicion de terminado

- [x] Cada motor incluido requiere su capacidad persistente tanto al proponer
      como al ejecutar.
- [x] Las cuatro capacidades contienen el tuning por actor que antes vivia en
      constantes de motor; constantes geometricas y de arbitraje permanecen
      fuera de ellas.
- [x] `PLAYER` conserva todos los valores anteriores exactamente.
- [x] Tests de arquitectura cubren que un actor sin cada capacidad no propone
      el estado correspondiente.
- [x] `cargo fmt`, `cargo test` y `cargo clippy --all-targets -- -D warnings`
      pasan.
- [x] El usuario valida en `cargo run` todo traversal del mapa antes de abrir
      otro ticket.

## Estado

Checkpoint jugable aceptado. Este ticket reemplazo temporalmente el corte
individual de Climb porque el usuario eligio probar traversal en batch.
