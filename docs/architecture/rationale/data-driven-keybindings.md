# Rationale: keybindings genéricos y rebindeables, no lógica hardcodeada por esquema

## El problema

La primera versión de `input.md` resolvía el truco de "WASD hace doble uso"
con un `Resource` booleano (`ActionLayer`) calculado por una regla fija
(`Period`+`Space` sostenidos) y cada Brain (Movement/Combat/NPCs) seguía
leyendo `ButtonInput<KeyCode>` crudo para WASD. Eso hardcodea **una** forma
de jugar. El punto real (WoW: podés bindear "Atacar" a `1` o a `Alt+1`, a tu
gusto) es que el *feeling* de qué tecla se siente bien para cada acción
solo se descubre jugando — la arquitectura no puede asumir un mapa fijo, ni
siquiera el que salió de una buena sesión de diseño (`rationale/keyboard-only-action-layer.md`).

## La decisión: Input resuelve combos genéricos, los Brains solo leen acciones resueltas

Se introduce `IntentAction` (enum plano, domain-agnóstico: `Attack`,
`Jump`, `MoveForward`, etc.) como la única forma semántica que Input expone.
Una tabla `Keybindings` rebindeable mapea `InputChord` (modifiers + trigger
de hardware abstracto) → `[IntentAction; MAX_ACTIONS_PER_CHORD]`. Un sistema
(`resolve_bindings`) evalúa esa tabla contra el hardware una vez por frame y
escribe `ActiveActions` por `InputSource`. El enlace actor→fuente vive en
`InputControlledBy(InputSource)`. **Ningún** Brain de Movement/Combat/NPCs
vuelve a tocar `ButtonInput<KeyCode>` — cada uno lee `ActiveActions` mediante
ese enlace, consume gatillos con su propio `InputConsumeCursor` y arma su
propio `Intents`/`CombatIntents`/`InteractIntents` a partir de acciones ya
resueltas, sin saber qué tecla física las disparó. (codex)

Esto sigue la misma regla de inversión de dependencia que ya se aplicó a
`src/proposal.rs`
(`rationale/proposal-arbitration-core.md`) — un núcleo compartido no puede
conocer reglas de dominio, y los dominios no pueden conocer el mecanismo
del núcleo. Acá el núcleo es "qué combo está activo", no "qué significa
para el combate".

## Resolución por especificidad (por qué `Alt+1` no dispara también el bind de `1`)

Si dos bindings comparten la misma tecla disparadora (`trigger`) pero
distintos modifiers (ej. `1` → `Jump`, `Alt+1` → `Attack`), sostener
`Alt+1` debe disparar **solo** `Attack`. La regla: entre los bindings cuyo
`trigger` coincide y cuyos `modifiers` están **todos** presionados, gana el
de más modifiers. Es la misma convención que usan WoW, la mayoría de
motores de keybinding de Unity/Unreal, y el propio combo `Period+Space+S`
que diseñamos para `KeyboardOnly` — ese combo de 3 teclas no es un
mecanismo especial, es simplemente el chord con más modifiers de todos los
que comparten trigger `S` (el otro es `S` solo → `MoveBack`). Si el chord
ganador tiene varias acciones asociadas, se activan todas; esto permite
defaults contextuales como `Space` → `Jump` + `Glide` sin hardcodear una
tecla en Movement. (codex)

Dos chords distintos con el mismo número de modifiers sobre la misma tecla
y distinta especificidad esperada son un error de configuración, no una
ambigüedad a resolver en runtime — se rechazan al grabar el bind (mismo
principio que Constitución §8/§9: los estados inválidos deben ser
irrepresentables o rechazados temprano, no silenciosamente arbitrados).

## Supresión de modifiers (por qué `Period+Space+S` no dispara también `Space` solo)

La regla de especificidad de arriba solo desambigua chords que comparten el
**mismo** `trigger`. `Space` aparece dos veces en la tabla por defecto:
como `trigger` de su propio binding (`Space` sola → `Jump`+`Glide`) y como
**modifier** de otros cuatro (`Period+Space+S/A/D/W` → `Attack`/`Parry`/
`Aim`/`Interact`). Son evaluaciones separadas por `trigger` — nada en la
regla de especificidad, por sí sola, impide que ambas ganen a la vez:
sosteniendo `Period+Space+S`, el trigger `S` resuelve a `Attack` (2
modifiers le gana a `S` sola), pero el trigger `Space` —evaluado aparte—
sigue viendo `Space` presionada con 0 modifiers requeridos, que se
satisface trivialmente, y dispararía `Jump`+`Glide` al mismo tiempo que se
ataca.

Por eso `resolve_bindings` aplica una segunda fase, no solo la de
especificidad: primero calcula el ganador por cada trigger presionado; luego
marca qué `HardwareTrigger` participaron como modifier de esos ganadores; por
último descarta cualquier ganador cuyo propio trigger haya quedado consumido
como modifier de otro ganador. `Space` queda consumido por el chord ganador de
`S` (o `A`/`D`/`W`), así que su propio binding de `Jump`+`Glide` no se activa
ese frame. Si el jugador solo sostiene `Space` (sin `Period`), ningún chord la
consume como modifier, y `Jump`+`Glide` dispara normalmente. (codex)

## Consecuencia sobre `keyboard-only-action-layer.md`

Ese rationale sigue siendo válido para explicar **por qué** el default de
`KeyboardOnly` usa `Period+Space` como modifiers (ergonomía de manos/dedos
libres) — pero deja de describir un mecanismo exclusivo de ese esquema.
Es un caso particular de la tabla `Keybindings`, no una rama de código
aparte. Un jugador puede reasignar ese combo a otra cosa sin que Movement,
Combat o NPCs se enteren.

## Consecuencia sobre los Brains existentes

`movement::brain::read_intents` deja de leer `KeyCode::KeyW` et al.
directamente — lee `ActiveActions` (`input.md`). Esto es la
generalización completa del comentario ya existente en `brain.rs` ("input
solo entra por el Brain"): ahora *ni siquiera el Brain* sabe qué tecla
física corresponde a qué acción, solo qué acciones están activas. El mismo
código de Movement/Combat/NPCs sirve sin cambios para `Gamepad`,
`KeyboardOnly` o `KeyboardMouse`. (codex)
