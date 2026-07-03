# Por qué Nadar/Bucear y Snowboard son motores de Movement, no plugins nuevos

**Decisión:** `Swim`, `Dive` y `Snowboard` se implementan como motores nuevos
dentro de `src/movement/motors/` (nuevas variantes de `LocomotionState`,
nuevos `*Facts`, nuevos `propose()`), no como plugins independientes
(`SwimPlugin`, `SnowboardPlugin`).

## Por qué no viola Constitución §14

§14 dice "un plugin de Bevy por sistema" — la pregunta es qué cuenta como
"sistema". Nadar, bucear y deslizar en nieve son **modos de traversal del
mismo cuerpo cinemático**, exactamente como Caminar, Trepar o Planear ya lo
son: comparten `Intents`, `BodyVelocity`, `Stamina` (u oxígeno como pool
análogo) y el mismo pipeline `ReadIntents → SenseWorld → GatherProposals →
Arbitrate → TickActiveMotor`. Son un "sistema" en el sentido de "una
responsabilidad" (locomoción del actor), no trece sistemas distintos — igual
que Climb y Glide ya conviven en el mismo plugin.

## Por qué NO se duplica el pipeline

Crear `SwimPlugin` obligaría a duplicar `Intents`, `LocomotionState`,
`ProposalBuffer` y el ciclo de arbitración completo solo para agregar 3
estados más a un enum que ya tiene 13 — repite exactamente el error que
`rationale/proposal-arbitration-core.md` evitó para Combat/Mounts (ahí la
solución fue compartir el *algoritmo* con tipos propios; acá ni siquiera
hace falta un tipo propio, el dominio es literalmente el mismo).

## Costo aceptado

`LocomotionState` crece (13 → 16 variantes) y `movement.md` deja de caber
cómodamente en el límite blando de ~100 líneas si se documentan los 3 modos
ahí — por eso viven en `swim.md`/`snowboard.md` propios, aunque el código
viva en la misma carpeta/plugin. Esto es documentación, no arquitectura: el
código sigue siendo un solo plugin.

## Cuándo SÍ ameritaría separar

Si Nadar/Bucear necesitara su propio ciclo de arbitración independiente
(ej. sub-estados que compiten *dentro* del agua sin interactuar con
Caminar/Trepar en absoluto) o dejara de compartir `Intents`, ahí sí se
justificaría un plugin propio — no es el caso hoy.
