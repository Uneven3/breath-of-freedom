# Ticket: `climb-lip-jump-mantle`

## Regla

Desde `Climb` o `Ladder`, si LedgeService confirma un borde con Mantle
disponible, Jump significa `Mantle`, no `WallJump`. Fuera del borde, Jump
conserva WallJump. La acción de Mantle dedicada continúa disponible.

## Implementación

Mantle propone con `Priority::Forced` y peso 10 al recibir `wants_jump` o
`jump_pressed` desde Climb o Ladder. Eso vence el `WallJump` de peso 5 durante la
arbitrariedad del mismo tick.

## Verificación

- [x] `Climb + borde válido + jump_pressed` propone Mantle.
- [x] `Ladder + borde válido + jump_pressed` arbitra a Mantle sobre WallJump.
- [ ] Play-test: Jump en borde hace pull-up; Jump fuera del borde hace WallJump.
