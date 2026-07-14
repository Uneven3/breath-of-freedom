# Ticket: `wall-jump-neutral-input`

## Regla

Mientras el actor está en `Climb` o `Ladder`, pulsar Jump sin dirección inicia
el WallJump de retroceso. Las direcciones explícitas mantienen las variantes
de salto actuales; EdgeLeap conserva prioridad en los bordes laterales.

## Problema

`WallJump` leía solo el estado sostenido de Jump. Una pulsación breve puede
empezar y terminar entre ticks fijos, de modo que el motor nunca ve la
intención. El motor Jump ya posee buffering, pero ese borde no se publicaba
en `Intents` para los saltos especializados.

## Implementación

- Input publica una generación discreta para `IntentAction::Jump` además del
  estado sostenido usado por salto variable/Glide.
- Movement consume esa generación una vez por actor como `Intents::jump_pressed`.
- Jump, WallJump, EdgeLeap y Ladder aceptan el borde para sus acciones de
  salto/release, sin cambiar sus latches de hold.

## Verificación

- [x] Una pulsación neutra desde Climb propone WallJump.
- [x] Una pulsación breve se conserva hasta el siguiente tick fijo.
- [x] `cargo fmt`, tests y clippy pasan.
