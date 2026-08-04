# Leyes que se cobran solas

Plan para que las leyes de `ARCHITECTURE.md` dejen de depender de que alguien se
acuerde (**≤300 líneas**). **Los crates son la última fase, no la primera**: lo
que se puede cerrar con un test esta semana no espera a un refactor de seis.
Estado: **fases 1-5 cerradas; grafo de crates hermanas elegido**. Se
borra cuando la última fase cierre — igual que `AHORA.md` borra lo cerrado.

## El problema, medido (2026-08-01)

`ARCHITECTURE.md` fija 21 leyes. Varias son verificables por máquina y hoy no
las verifica nadie:

| Ley | Dice | Estado real |
|---|---|---|
| §4 | APIs públicas mínimas | 1392 `pub` contra 74 `pub(crate)`. En un crate único `pub` no restringe nada. |
| §8/§9 | Evitar `unwrap`/`expect`; panic = bug | **93** fuera de `#[cfg(test)]`. |
| §12 | Sin `unsafe` | Cierto hoy; nada lo impide mañana. |
| §13 | `clippy -D warnings` | Cero líneas de `[lints]` en `Cargo.toml`. |
| §20 | Simulación nunca depende de visuales | 5 archivos de simulación nombran `Mesh`/`StandardMaterial`. |
| C2 | Solo `input` lee hardware | **13 archivos fuera de `input`**, congelados por `tests/architecture.rs` (eran 15 el 2026-08-01). |

**El código no está sucio: el listón está bajo.** `cargo clippy` con los lints
por defecto sale **limpio, 0 warnings**. Nada de lo anterior es descuido — es la
distancia entre lo que clippy revisa por defecto y lo que las leyes exigen.

Escala: 168 archivos, 37 410 LOC, 21 módulos, **un solo crate**, 391 tests
(389 unitarios + 2 de arquitectura).
Cualquier cambio recompila los 38 k: con las dependencias en caché y una sola
unidad `Compiling breath-of-freedom`, `clippy` tardó **7 min 21 s**.

## El principio

**Si una ley se puede convertir en error de compilación o en un test, se
convierte. La que necesita revisor es la que se incumple** — C2 creció mientras
vivió sólo en prosa; el test ahora obliga a que su lista sólo pueda encoger.

Corolario para este repo: **no se agregan leyes.** 21 es más de lo que se
recuerda al escribir código. Lo que sale de aquí es al revés — leyes que se
funden porque el build ya las cobra (§8, §12 y la mitad de §13 caben en un
bloque `[lints]`; §4 pasa a ser la frontera de un crate).

## Orden de trabajo

Por retorno sobre esfuerzo, no por prolijidad:

| # | Fase | Cierra | Escala |
|---|---|---|---|
| 1 | Tests de frontera ✅ | C2, §12 | cerrada |
| 2 | Determinismo ✅ | pilar 5 de `NORTE.md` | cerrada |
| 3 | Lints ✅ | §8, §9, §13 | cerrada |
| 4 | Acceso al terreno ✅ | escala del mundo | cerrada |
| 5 | `bof_domain` ✅ | §19 como frontera | cerrada |
| 6 | `bof_simulation` | §20, C2 definitivo | semanas |
| 7 | `bof_presentation` + app | §4 | semanas |

Las fases 1-4 no tocan la estructura y cada una termina jugable (§10) con
`fmt` + `clippy` + `test` verdes.

---

## Fase 1 — Los tests que no esperan al refactor

`tests/architecture.rs` recorre el árbol sin depender del juego y congela dos
leyes antes del refactor:

- Falla ante `ButtonInput`/`MouseMotion`/`MouseWheel` fuera de
  `src/input/`, con una **lista de excepciones conocidas que solo puede
  encoger**. Entró verde y la deuda dejó de crecer.
- El mismo test cubre §12 (`unsafe`).
- Provisional a propósito: en la fase 6 lo hace Cargo y el test se borra.

## Fase 2 — Determinismo

Estado: **cerrada**. `ShotSpreadRng` vive por actor con semilla authored;
`ActorId` reemplaza a `Entity` en patrullaje, LOD de sensores y desempates de
percepción. El replay headless ejecuta Movement + Avian reales por 120 ticks,
invierte el orden de spawn, desplaza los IDs transitorios con 17 entidades dummy
y exige `Transform`/`BodyVelocity`/`LocomotionState` idénticos por `ActorId`.

## Fase 3 — Lints

```toml
[lints.rust]
unsafe_code = "forbid"

[lints.clippy]
all = { level = "deny", priority = -1 }
expect_used = "deny"
unwrap_used = "deny"
cast_possible_truncation = "deny"
cast_sign_loss = "deny"
float_cmp = "deny"
wildcard_enum_match_arm = "deny"
```

Estado: los seis lints están en `deny` y `clippy --all-targets -D warnings`
pasa. El inventario real de Clippy corrigió la medición textual: en producción
había 3 `expect` y ningún `unwrap`; el conteo de 93 incluía módulos de test. Los
tests permiten `unwrap`/`expect` y comparaciones exactas de floats para fixtures
y constantes authored, pero no conversiones truncantes.

Se hicieron exhaustivos 6 matches, se eliminó la única igualdad directa de
floats en producción y se corrigieron 55 avisos del inventario conjunto. Las
conversiones enteras se comprueban; terreno concentra el único helper acotado
f32→índice, y grass conserva como excepción puntual su conteo redondeado de
briznas. El terreno quedó confirmado visualmente y un test fija el reloj
`HH:MM`; fase cerrada.

`wildcard_enum_match_arm` hace que agregar una variante rompa cada consumidor
que debía enterarse; `float_cmp` cobra la igualdad exacta fuera de tests.

## Fase 4 — El acceso al terreno

Estado: `TerrainAccess` es el único `SystemParam` de lectura y conserva dentro
la cardinalidad actual. Player, Movement, layout, grass, visual y editor piden
`height_at`/`kind_at`/pertenencia; las queries directas que quedan mutan el dato
o reconstruyen su collider dentro del dueño. No se implementó streaming.

Automatización verde y cero `terrain.single()`/`ground.single()` en `src/`. El
checkpoint post-refactor abrió/cerró limpio, releyó `sandbox.ron`, permitió
esculpir y lo guardó repetidamente el 2026-08-03. Fase cerrada (§10).

---

## El corte en crates (fases 5-7)

```text
breath-of-freedom (bin)   composición: main.rs, wiring de plugins, scene
   ├── bof_presentation   visuals, camera, presentation, sfx, debug, perf, editor
   ├── bof_simulation     movement, combat, mounts, projectiles, health,
   │                      inventory, enemies, world, interaction, time_control,
   │                      player, input, asset_pipeline
   └── bof_domain         datos puros: tipos, unidades, Intents, estados, facts
```

Nombres según la regla del workspace (paquete prefijado con el slug, import
corto por `[lib] name`):

```toml
bof_domain = { package = "breath_of_freedom_domain", path = "crates/domain" }
```

Reparto de los 21 módulos (LOC entre paréntesis):

- **Solo presentation**: `visuals` (4 285), `presentation` (2 779), `debug`
  (2 130), `perf` (2 014), `editor` (1 299), `camera` (749), `sfx` (162). De
  `debug` y `perf` bajan a domain el `DebugSnapshot` y las perillas: son dato
  puro. `editor` escribe en `world` por mensaje.
- **Solo simulation**: `player` (395), `time_control` (86) — dueño de
  `Time<Virtual>`, que es simulación y no presentación (§20).
- **Partidos domain + simulation** (los `data.rs`/`state.rs` arriba, los
  sistemas abajo): `movement` (9 986), `world` (3 489, con `GameLayer` y los
  tipos en domain), `combat` (2 251), `mounts` (1 884), `inventory` (1 689),
  `enemies` (1 482), `input` (681, `ActiveActions` en domain y el muestreo de
  hardware como **único** que declara `bevy_input`), `projectiles` (387),
  `interaction` (352), `health` (341).
- **Solo domain**: `asset_pipeline` (956, `schema.rs` es SoT con `build.rs`) y
  `proposal`, el núcleo genérico de arbitración.
- **Al binario**: `scene` (510) — decide qué existe y cuándo, es composición.

El grafo de `crate::X` **ya es casi un DAG**. Los únicos ciclos son de una línea:

- `world/spawn.rs:189` → `crate::visuals::VisualOf(target)`
- `visuals/enemy.rs:47` → `crate::presentation::juice::HitFlash` en un `Without<>`
- `world/terrain.rs:9-10` → solo doc-links

`VisualOf` es el contrato actor↔visual: baja a `domain` y el ciclo desaparece
sin mover lógica. `HitFlash` es igual, o `juice` se funde con `visuals`.

### Fase 5 — `bof_domain`

Estado: **cerrada el 2026-08-03** con la topología hermanas elegida por el
usuario. `breath_of_freedom_domain` posee los contratos compartidos de input,
movimiento, combate, health, inventario, mounts, projectiles, debug/perf y
assets; los módulos viejos reexportan durante la migración para no mezclar el
corte de datos con el de sistemas. `build.rs`, `schema.rs` y el manifiesto
generado se mudaron juntos. El estado visual de flecha se separó del filtro
físico; las recetas con `Transform` quedaron correctamente en presentation.
La build post-corte abrió/cerró limpia sobre Vulkan/Polaris.

Es promover §19 ("datos separados de sistemas") de convención de archivos a
frontera de crate, y la costura ya está hecha: 16 archivos
`data.rs`/`state.rs`/`intents.rs`/`facts.rs`/`proposal.rs` suman **2 269 LOC** y
solo uno menciona avian (`projectiles/data.rs`, 1 línea). Se suman
`movement/{body,stamina,facing,sensing,probe_data,abilities,diag}.rs`,
`combat/weapon.rs`, `asset_pipeline/schema.rs`, `visuals/catalog.rs`.

Deps runtime: `bevy_ecs`, `bevy_math`, `bevy_reflect`. **No** `bevy`, **no**
Avian; test de arquitectura y `cargo tree` fijan que tampoco entra render.

Lo que presentación lee hoy de `movement` es casi todo dato puro (`Actor`,
`BodyVelocity`, `BodyDimensions`, los `*Facts`, `Intents`, `LocomotionState`,
`Stamina`, `FacingSource`, `GroundSensing`, `TraversalProbe`, `ProposalBuffer`,
`CastTrace`). Dos excepciones a resolver a mano:

- `motors::stairs::expected_feet_y` — una función. O helper puro en domain, o el
  visual lee un fact que el motor ya publica.
- `MovementSet` — un `SystemSet`; el orden pertenece a quien arma el schedule.

### Fase 6 — `bof_simulation`

Estado: **en curso; 6.1–6.2 cerradas el 2026-08-04**. Cada fila termina
compilable y verde; primero se traslada sin rediseñar, luego se mejora.

| Corte | Movimiento de código |
|---|---|
| 6.1 ✅ | Esqueleto Cargo + Avian mínimo + smoke test headless. |
| 6.2 ✅ | `health`, `interaction`, `time_control`. |
| 6.3 | `inventory`, `projectiles`. |
| 6.4 | Movement: infraestructura, schedules, servicios. |
| 6.5 | Movement: motores y orquestación; conserva replay determinista. |
| 6.6 | `combat`, `enemies`, `mounts`. |
| 6.7 | `player`, `input`, `world` y runtime de assets; render queda en adaptadores. |
| 6.8 | Cableado raíz, replay headless, retiro de shims/test redundante y checkpoint. |

Avian usará `default-features = false` con `3d`, `f32`, `parry-f32`, `parallel`
y `xpbd_joints`: `debug-plugin` pasa a presentación y `collider-from-mesh` no se
usa. Así el target headless no linkea `bevy_render` ni bifurca innecesariamente
el build compartido. `build.rs` ya vive con schema/manifiesto en `bof_domain`.
El smoke se corre como paquete aislado: seleccionar también el binario en la
misma invocación unifica sus features legacy de Avian y deja de medir headless.

### Fase 7 — `bof_presentation` y el binario (hermanas)

¿Pila lineal o hermanas?

```text
Lineal:     domain ← simulation ← presentation ← app
Hermanas:   domain ← simulation ← app
            domain ← presentation ← app
```

**Decisión del usuario 2026-08-03: hermanas.** §20 dice
"presentación solo READ". Si presentación ve simulación, ve sus funciones y sus
sistemas, y "solo leer" vuelve a ser disciplina — la misma que hoy falla en 15
archivos. Si solo ve `domain`, que es dato puro, **leer es lo único que puede
hacer**.

El impuesto es real: todo lo que presentación lea tiene que ser dato en
`domain`, así que `domain` crece y el contrato entre capas se vuelve explícito.
La lista de la fase 5 dice que ya estamos casi ahí.

Esta decisión ya gobernó fase 5: todo contrato que ambas leen baja a domain;
funciones, sistemas, render y física permanecen con su dueño.

---

## La frontera se prueba, no se declara

Cargo prohíbe la dependencia inversa, pero no prueba que la simulación siga
siendo la misma sin pantalla. Eso solo se prueba corriéndola: **el mismo
escenario headless y renderizado, exigiendo estado idéntico tick a tick**. Si
divergen, presentación está escribiendo verdad. Se apoya en la fase 2 — sin
semilla determinista esa comparación no existe — y el beneficio llega aunque el
co-op no llegue nunca: tests de simulación sin ventana ni GPU, en segundos.

La fase 2 fijó el primer snapshot: `Transform`/`BodyVelocity`/
`LocomotionState` de cada `ActorId` durante 120 ticks. En fase 7 se corre el
mismo contrato con y sin presentación para probar que la capa sólo lee.

## Más allá de los crates: los tipos

Los crates ordenan *quién ve a quién*. Aparte hay error de runtime que podría
ser error de compilación. Es aditivo: se paga por módulo, empezando por
`movement`, cuando ese módulo ya esté en su crate.

- **229 campos `f32` crudos contra 2 newtypes** (`JumpStaminaCost`,
  `Awareness`): nada impide sumar metros a metros/segundo, ni pasar un radio
  donde va una altura. Las unidades de movimiento son el mayor retorno.
- **122 campos `bool` contra 47 enums**: un `bool` se invierte sin que nadie se
  entere; un enum de dos variantes con `wildcard_enum_match_arm` convierte cada
  estado nuevo en un error en todos sus consumidores.
- **0 usos de `#[require(...)]`** (Bevy 0.19): "un actor necesita `Intents` y
  `BodyVelocity`" vive en funciones de spawn en vez de en el tipo.

## Qué pasa con las leyes al terminar

`ARCHITECTURE.md` está en 200/200 líneas y los cuatro core suman **994 de 1000**.
Este plan no agrega leyes: las funde. Al cerrar cada fase, su ley se reescribe
como una línea que **nombra su mecanismo** (§8/§12/§13 → "los `[lints]` de cada
`Cargo.toml`"; §4 → "la frontera del crate"; §20 → "`bof_simulation` no declara
`bevy`"), y el espacio liberado paga el techo. Que además debería cobrarse solo:
un test que sume los cuatro core y falle sobre 1000. Hoy es honor-system, y está
a 6 líneas del límite.

## Criterios de aceptación

| Fase | Verde cuando |
|---|---|
| 1 | El test de frontera pasa y su lista de excepciones solo encoge. |
| 2 ✅ | Dos corridas con la misma semilla dan estado idéntico N ticks; `Entity::to_bits` no aparece en ningún cálculo de resultado. |
| 3 ✅ | Lints en `deny`, Clippy limpio y checkpoint cerrado. |
| 4 ✅ | Grep vacío, suite verde y checkpoint jugado aceptado. |
| 5 ✅ | `cargo tree -p breath_of_freedom_domain` sin `bevy_render`; 50 tests propios y frontera automática. |
| 6 | `grep -rl "Mesh\|StandardMaterial" crates/simulation/src` vacío; `bevy_input` solo en el dueño de input; el test de la fase 1 se borra por redundante; checkpoint jugado. |
| 7 | `cargo tree` de presentation sin `bof_simulation`; checkpoint jugado; frame time sin regresión contra el baseline de `AHORA.md`. |
