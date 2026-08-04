# Leyes que se cobran solas

Plan para que las leyes de `ARCHITECTURE.md` dejen de depender de que alguien se
acuerde (**≤300 líneas**). **Los crates son la última fase, no la primera**: lo
que se puede cerrar con un test esta semana no espera a un refactor de seis.
Estado: **fases 1-6 cerradas; queda la 7**. Se borra cuando cierre — igual que
`AHORA.md` borra lo cerrado; el detalle de cada fase queda en git.

## El principio

**Si una ley se puede convertir en error de compilación o en un test, se
convierte. La que necesita revisor es la que se incumple** — C2 creció mientras
vivió sólo en prosa; el test ahora obliga a que su lista sólo pueda encoger.

Corolario para este repo: **no se agregan leyes.** 21 es más de lo que se
recuerda al escribir código. Lo que sale de aquí es al revés — leyes que se
funden porque el build ya las cobra.

## Lo cerrado, y con qué mecanismo quedó cobrado

| # | Fase | La cobra ahora |
|---|---|---|
| 1 ✅ | Tests de frontera | `tests/architecture.rs`: hardware fuera de `input` y `unsafe`. **No se borra** (ver abajo) |
| 2 ✅ | Determinismo | Replay headless de 120 ticks por `ActorId`, dentro de `bof_simulation` |
| 3 ✅ | Lints | Los `[lints]` de cada `Cargo.toml`: §8, §9, §12, §13 |
| 4 ✅ | Acceso al terreno | `TerrainAccess` es el único `SystemParam` de lectura |
| 5 ✅ | `bof_domain` | §19: el dato compartido no declara `bevy` ni Avian |
| 6 ✅ | `bof_simulation` | §20: no declara `bevy_render`, `bevy_input` ni `bevy_window` |
| 7 | `bof_presentation` + app | §4 como frontera de crate |

**El test de fase 1 sobrevive a su fecha de vencimiento.** El plan decía que en
fase 6 lo reemplazaría Cargo. Eso valía sólo si `input` cruzaba a simulación; se
quedó en la app (abajo), así que Cargo cobra que *simulación* no lea hardware,
pero dentro de la app no llega nadie. Sus 13 entradas siguen ahí y sólo pueden
encoger.

## El corte en crates

```text
breath-of-freedom (bin)   composición: main.rs, scene, world::layout/spawn, input
   ├── bof_presentation   visuals, camera, presentation, sfx, debug, perf, editor
   ├── bof_simulation     movement, combat, mounts, enemies, player, world,
   │                      projectiles, health, inventory, interaction, time_control
   └── bof_domain         datos puros: tipos, unidades, Intents, estados, facts
```

Nombres según la regla del workspace (paquete prefijado con el slug, import
corto por `[lib] name`):

```toml
bof_domain = { package = "breath_of_freedom_domain", path = "crates/domain" }
```

### Hermanas, no pila

```text
Lineal:     domain ← simulation ← presentation ← app
Hermanas:   domain ← simulation ← app
            domain ← presentation ← app
```

**Decisión del usuario 2026-08-03: hermanas.** §20 dice "presentación solo
READ". Si presentación ve simulación, ve sus funciones y sus sistemas, y "solo
leer" vuelve a ser disciplina. Si sólo ve `domain`, que es dato puro, **leer es
lo único que puede hacer**. El impuesto es real y ya se está pagando: todo lo que
presentación lea tiene que ser dato en `domain`.

### Tres cosas que el reparto original decía mal

Se escribió antes de elegir hermanas y antes de ver el código de cerca:

- **`input` se queda en la app.** Lee hardware *y* maneja el cursor de la
  ventana. Afuera, la simulación no *puede* leer teclado; adentro habría
  arrastrado `bevy_window`. No pierde nada: `ActiveActions`,
  `ControlOrientation` e `IntentAction` ya son domain.
- **`world::layout` y `world::spawn` se quedan en el binario.** Son composición,
  como `scene`. El binario es la única capa que ve simulación y presentación a la
  vez, así que es el único lugar donde armar collider **y** malla en la misma
  función es legal. Del resto de `world` cruzó todo: heightfield, semántica por
  celda, marcadores authored y el reloj.
- **De `day_night` cruzó el reloj, no la luz.** `advance_time` escribe cada tick
  y presentación sólo lee, así que no podía terminar en el crate equivocado
  aunque hoy nadie de gameplay consulte la hora.

### La costura que se repitió cinco veces

Terreno, player, enemigos, caballo y el reloj estaban todos acoplados a `scene`
por lo mismo: **cuándo** nacen. La respuesta fue siempre igual — simulación
expone *cómo* se construye (una función o un mensaje), la tabla de escenas decide
*cuándo*, y la entidad declara su vida con `SceneScoped`, que la app bindea a
`DespawnOnExit`. Ningún sistema de simulación nombra `AppState`.

Vale como patrón para lo que venga: si algo de simulación quiere consultar el
estado de la app, la pregunta correcta suele ser qué mensaje debería recibir.

## Fase 7 — `bof_presentation`

Lo que falta es que presentación deje de leer `bof_simulation`. Medido el
2026-08-04, son ~35 referencias en `camera`, `debug`, `visuals`, `presentation`,
`sfx` y `inventory::pickup`, en dos grupos muy distintos:

- **Renombre puro** (el tipo ya vive en domain y simulación sólo lo reexporta):
  `CombatState`, `FacingSource`, `BodyDimensions`, `CastTrace`, `Horse`,
  `GroundSensing` y todos los `movement::{facts, intents, state, stamina,
  proposal, probe_data}`. Es cambiar el prefijo.
- **Deuda de diseño**, ocho tipos que hay que decidir dónde van:
  `combat::motors::aim::{BowFiredMessage, DrawStrength, BOW_SOCKET_LOCAL}`,
  `attack::{ComboLocal, HitImpactMessage}`, `motors::sneak::Crouched`,
  `enemies::{Enemy, Awareness}`, `health::Health` y `MovementSet`. Casi todos
  son mensajes o componentes de estado, o sea justo lo que domain debería
  contener; `MovementSet` es orden y pertenece a quien arma el schedule.

`ComboLocal::current_step` está `pub` sólo porque `visuals::vfx` dibuja el arco
del swing con `reach`/`arc_deg`. O el motor publica ese par como fact, o
`ComboLocal` baja a domain. Es el caso testigo del grupo de arriba.

`JumpLocal::grant_coyote` es lo inverso y se puede cerrar ya: existía para un
test de `mounts` que estaba del otro lado de la frontera, y hoy están juntos.

## La frontera se prueba, no se declara

Cargo prohíbe la dependencia inversa, pero no prueba que la simulación siga
siendo la misma sin pantalla. Eso sólo se prueba corriéndola. El smoke headless
ya no levanta una esfera con gravedad: levanta `SimulationPlugin` entero, sin
ventana ni GPU, en segundos.

La fase 2 fijó el snapshot: `Transform`/`BodyVelocity`/`LocomotionState` de cada
`ActorId` durante 120 ticks. En fase 7 se corre el mismo contrato con y sin
presentación para probar que la capa sólo lee.

**Los crates se testean por paquete.** Meter el binario en la misma invocación
que el crate unifica las features de Avian y el smoke deja de ser headless
(panickea). `cargo test`, `cargo test -p breath_of_freedom_simulation` y
`cargo test -p breath_of_freedom_domain`, por separado.

## Más allá de los crates: los tipos

Los crates ordenan *quién ve a quién*. Aparte hay error de runtime que podría ser
error de compilación. Es aditivo: se paga por módulo, empezando por `movement`.

- **229 campos `f32` crudos contra 2 newtypes** (`JumpStaminaCost`,
  `Awareness`): nada impide sumar metros a metros/segundo, ni pasar un radio
  donde va una altura. Las unidades de movimiento son el mayor retorno.
- **122 campos `bool` contra 47 enums**: un `bool` se invierte sin que nadie se
  entere; un enum de dos variantes con `wildcard_enum_match_arm` convierte cada
  estado nuevo en un error en todos sus consumidores.
- **0 usos de `#[require(...)]`** (Bevy 0.19): "un actor necesita `Intents` y
  `BodyVelocity`" vive en funciones de spawn en vez de en el tipo.

## Qué pasa con las leyes al terminar

Este plan no agrega leyes: las funde. Al cerrar cada fase, su ley se reescribe
como una línea que **nombra su mecanismo**, y el espacio liberado paga el techo
de `ARCHITECTURE.md` (200/200, al límite). Ya se hizo con §4, §14, §19 y §20.

Ese techo debería cobrarse solo: un test que sume los cuatro documentos core y
falle sobre 1000 líneas. Hoy es honor-system.

## Criterios de aceptación

| Fase | Verde cuando |
|---|---|
| 6 ✅ | Sin `Mesh`/`StandardMaterial`/`bevy_input` en `crates/simulation/src`; smoke headless levanta el juego entero; checkpoint jugado. |
| 7 | `cargo tree` de presentation sin `bof_simulation`; checkpoint jugado; frame time sin regresión contra el baseline de `AHORA.md`. |
