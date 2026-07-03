# Rationale: capa de acciones en WASD para el esquema `KeyboardOnly`

> **Nota de alcance:** este documento explica el razonamiento ergonómico
> detrás del combo `Period+Space+WASD` como **default** rebindeable de
> `KeyboardOnly`. Ya no describe un mecanismo especial ("capa" como
> `Resource` propio) — eso quedó superado por el motor genérico de
> keybindings en `rationale/data-driven-keybindings.md`. La razón de fondo
> (qué dedos quedan libres, hold vs. toggle) sigue vigente sin cambios.
> **Corrección:** la sección de abajo sobre "suprimir Sprint/Sneak" describe
> un supuesto que no sobrevivió la generalización a `Keybindings` — `Shift`
> y `Ctrl` no son modifiers de ningún chord de la capa de acciones, así que
> nada los desactiva a nivel de Input. La supresión de Sprint (no de Sneak
> — ver corrección) ahora es responsabilidad de Combate vía
> `LocomotionConstraintMessage::ForbidSprint`, generalizado a todo estado de
> compromiso activo (`combat.md` § Sistemas). Este documento se conserva por
> el razonamiento del combo en sí, no por esa sección puntual.

## El problema

En `KeyboardMouse`, atacar/parry/apuntar viven naturalmente en los botones
del mouse porque la mano derecha no hace nada más. Sin mouse, la mano
derecha se muda a IJKL (cámara) y se pierden los 4 botones de acción
dedicados: un teclado no tiene sticks analógicos con botones superpuestos
como un gamepad.

## La decisión: WASD hace doble uso, gateado por un chord sostenido

Mientras `Period` (`.`) y `Space` están sostenidos simultáneamente, WASD
deja de significar movimiento y pasa a significar las 4 acciones
(`CombatIntents::wants_attack/parry/aim`,
`npcs::InteractIntents::wants_interact`). Fuera de ese chord, WASD es
movimiento como siempre — ver `input.md`.

### Por qué ese chord específico

Se eligió para no mover ninguna mano de su posición de descanso:

- **Mano izquierda:** sigue en WASD sin moverse — es la que va a *usar* las
  4 teclas reinterpretadas.
- **Mano derecha:** `.` (anular o meñique estirando una fila hacia abajo
  desde `L`) + `Space` (pulgar) — ambas piezas del chord caen en la mano
  derecha, dejando el índice y el medio de esa misma mano libres sobre
  `I`/`J`/`K` para seguir moviendo la cámara mientras se sostiene el chord.
  Esto es lo que permite apuntar y atacar al mismo tiempo.

Alternativa descartada: un chord con una tecla en cada mano (ej. `Shift`
izquierdo + `Enter` derecho) obligaría a mover una mano fuera de WASD o de
IJKL para sostenerlo, perdiendo control de movimiento o de cámara mientras
se sostiene — exactamente lo que este diseño evita.

### Por qué hold y no toggle

Un chord sostenido no tiene estado invisible: la capa activa es una función
pura de qué teclas están abajo *en ese frame*. Un toggle necesitaría un
indicador en pantalla para que el jugador sepa en qué capa está — viola
"UI mínima" (Pilar del GDD) — y agrega un modo de fallo real: atacar cuando
se quería mover, o viceversa, por perder la cuenta de en qué capa se está.

### Por qué Sprint (no Sneak) se restringe durante un compromiso de combate — **corregido, ver nota de arriba**

La intuición original ("no tiene sentido esprintar mientras se ataca",
Pilar "Combate con peso: lento y deliberado") sigue siendo correcta, pero
**Sneak queda explícitamente afuera**: sigilo + ataque es el combo de
bonus de daño que el GDD ya define (§7) — restringirlo rompería esa
mecánica. Y la restricción de Sprint no puede vivir en la tabla de
`Keybindings` (`Shift` no es modifier de ningún chord de la capa de
acciones, nada la "consume") — vive en Combate, que ya tiene el mecanismo
correcto: `sprint::propose` en Movement no exige movimiento para proponer
`Sprint` (alcanza con `grounded && wants_sprint`), así que sin una
restricción explícita, sostener `Shift` durante `Windup`/`Active` puede
ganarle a `Walk` en arbitración con el jugador quieto atacando. Ver
`combat.md` § Sistemas para el mecanismo real (`ForbidSprint` generalizado
más allá de `Aiming`).

## Por qué esto vive en `Input` y no dentro de `movement::brain`

Combat y NPCs también necesitan que `S`/`A`/`D`/`W` disparen sus propias
acciones cuando el combo está activo — es el mismo problema que ya resolvió
`src/proposal.rs` para el núcleo de arbitración: la regla es compartida por
≥2 sistemas reales (no hipotéticos — Combat y NPCs ya tienen su propio
`Intents` documentado esperando esta señal), así que vive en un módulo
neutral que ninguno de los tres posee.

## Consecuencia

Este razonamiento ergonómico define los valores por defecto de la tabla
`Keybindings` de `KeyboardOnly` (`input.md`), no una rama de código
exclusiva. Cada sistema (Movement, Combat, NPCs) sigue sin conocer al otro
ni a la tecla física concreta — todos dependen de `IntentAction`, la forma
genérica que expone `Input` (`rationale/data-driven-keybindings.md`).
