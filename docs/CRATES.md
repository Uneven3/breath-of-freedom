# Leyes que se cobran solas

Plan para que las leyes de `ARCHITECTURE.md` dejen de depender de que alguien se
acuerde (**≤300 líneas**). **Los crates son la última fase, no la primera**: lo
que se puede cerrar con un test esta semana no espera a un refactor de seis.
Estado: **propuesta**, ninguna fase ejecutada. Se borra cuando la última fase
cierre — igual que `AHORA.md` borra lo cerrado.

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
| C2 | Solo `input` lee hardware | **15 archivos fuera de `input`** (era 14 en la auditoría del 2026-07-25: la deuda crece). |

**El código no está sucio: el listón está bajo.** `cargo clippy` con los lints
por defecto sale **limpio, 0 warnings**. Nada de lo anterior es descuido — es la
distancia entre lo que clippy revisa por defecto y lo que las leyes exigen.

Escala: 168 archivos, 38 198 LOC, 21 módulos, **un solo crate**, 394 tests.
Cualquier cambio recompila los 38 k: con las dependencias en caché y una sola
unidad `Compiling breath-of-freedom`, `clippy` tardó **7 min 21 s**.

## El principio

**Si una ley se puede convertir en error de compilación o en un test, se
convierte. La que necesita revisor es la que se incumple** — C2 lleva desde el
2026-07-25 documentada en tres archivos, ya se cobró la tecla `Tab`, y creció.

Corolario para este repo: **no se agregan leyes.** 21 es más de lo que se
recuerda al escribir código. Lo que sale de aquí es al revés — leyes que se
funden porque el build ya las cobra (§8, §12 y la mitad de §13 caben en un
bloque `[lints]`; §4 pasa a ser la frontera de un crate).

## Orden de trabajo

Por retorno sobre esfuerzo, no por prolijidad:

| # | Fase | Cierra | Escala |
|---|---|---|---|
| 1 | Tests de frontera | C2, §12 | días |
| 2 | Determinismo | pilar 5 de `NORTE.md` | días |
| 3 | Lints | §8, §9, §13 | 1-2 sesiones |
| 4 | Acceso al terreno | escala del mundo | 1 sesión |
| 5 | `bof_domain` | §19 como frontera | semanas |
| 6 | `bof_simulation` | §20, C2 definitivo | semanas |
| 7 | `bof_presentation` + app | §4 | semanas |

Las fases 1-4 no tocan la estructura y cada una termina jugable (§10) con
`fmt` + `clippy` + `test` verdes.

---

## Fase 1 — Los tests que no esperan al refactor

`AHORA.md` ya especifica el remedio de C2: *"un test que prohíba `ButtonInput`
fuera de `src/input/`"*. Se escribe hoy, sin mover un módulo, y hay precedente
de tests que leen el árbol con `std::fs` (`world/terrain.rs`,
`editor/persist.rs`).

- Recorrer `src/`, fallar ante `ButtonInput`/`MouseMotion`/`MouseWheel` fuera de
  `src/input/`, con una **lista de excepciones conocidas que solo puede
  encoger**. El test entra en verde el día uno y la deuda deja de crecer.
- El mismo test cubre §12 (`unsafe`) por dos líneas más.
- Provisional a propósito: en la fase 6 lo hace Cargo y el test se borra.

## Fase 2 — Determinismo

`src/combat/motors/aim.rs:288`:

```rust
let mut seed =
    ((time.elapsed_secs_f64().fract() * 100000.0) as u32) ^ (shooter.to_bits() as u32);
```

El spread del arco depende de **`Entity::to_bits()`** — el orden de spawn y la
generación tras despawns — y de un `f64` de tiempo acumulado. Dos máquinas en
co-op no ponen la flecha en el mismo sitio: es la primera divergencia del pilar
5 (*multiplayer host-autoritativo*), y está en el código hoy. Lo bueno es que no
hay dependencia `rand` y que `terrain.rs`/`grass.rs` ya siembran explícito — la
disciplina existe, le falta ser obligatoria.

- Un `SimRng` como recurso de simulación, con semilla registrada e inyectada.
  Nunca leer el reloj para decidir un resultado.
- **`Entity` no entra en ningún cálculo que decida un resultado.** Es
  transitorio. Para desempatar entre dos tiradores, un índice de actor estable.
- Test: misma escena, misma semilla, dos corridas, `Transform`/`BodyVelocity`/
  `LocomotionState` idénticos N ticks. Sin él, "host-autoritativo desde
  temprano" es una intención y no una propiedad.

## Fase 3 — Lints

```toml
[lints.rust]
unsafe_code = "forbid"

[lints.clippy]
all = { level = "deny", priority = -1 }
unwrap_used = "deny"
cast_possible_truncation = "deny"
cast_sign_loss = "deny"
float_cmp = "deny"
wildcard_enum_match_arm = "deny"
```

Arrancar en `warn` y subir a `deny` por familia. Trabajo esperado: 93
`unwrap`/`expect` de producción, ~140 casts (`as f32` ×52, `as usize` ×43,
`as u32` ×19, `as f64` ×13, resto ×13) y 32 brazos `_ =>`. Tests exentos con
`#![cfg_attr(test, allow(clippy::unwrap_used))]`, como permite §8.

`wildcard_enum_match_arm` es la más valiosa: con 47 enums, hace que **agregar
una variante rompa el build en cada sitio que debía enterarse**. `float_cmp`
cobra de paso una deuda ya anotada (apilado de comida por igualdad exacta de
`f32`). Y como Bevy 0.19 permite sistemas falibles, `Result` es la salida de los
93 `unwrap` sin convertirlos en `if let` mudos.

## Fase 4 — El acceso al terreno

`Terrain` es un singleton: `.single()` en `visuals/terrain.rs`,
`visuals/grass.rs` ×2, `editor/brush.rs`, `editor/mod.rs`. Con 320×320 m
funciona perfecto y la stop-line contra chunks/streaming está bien puesta.

Pero el día que el mundo crezca, chunks tocan esos cinco sitios más `height_at`,
el collider y el remuestreo. **No se implementa streaming** — se cambia el
*acceso*: nadie hace `terrain.single()`, todos preguntan `height_at(world_pos)` /
`kind_at(world_pos)` a un `SystemParam`. Misma jugada que ya funcionó con
`TreeKind → VisualCatalog`: separar la pregunta de quién la responde. Una tarde
ahora; el refactor que no se hace nunca después. Va antes de los crates porque
decide si `Terrain` es dato de `domain` o servicio de `simulation`.

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

Es promover §19 ("datos separados de sistemas") de convención de archivos a
frontera de crate, y la costura ya está hecha: 16 archivos
`data.rs`/`state.rs`/`intents.rs`/`facts.rs`/`proposal.rs` suman **2 269 LOC** y
solo uno menciona avian (`projectiles/data.rs`, 1 línea). Se suman
`movement/{body,stamina,facing,sensing,probe_data,abilities,diag}.rs`,
`combat/weapon.rs`, `asset_pipeline/schema.rs`, `visuals/catalog.rs`.

Deps: `bevy_ecs`, `bevy_math`, `bevy_reflect`. **No** `bevy`, **no** avian.

Lo que presentación lee hoy de `movement` es casi todo dato puro (`Actor`,
`BodyVelocity`, `BodyDimensions`, los `*Facts`, `Intents`, `LocomotionState`,
`Stamina`, `FacingSource`, `GroundSensing`, `TraversalProbe`, `ProposalBuffer`,
`CastTrace`). Dos excepciones a resolver a mano:

- `motors::stairs::expected_feet_y` — una función. O helper puro en domain, o el
  visual lee un fact que el motor ya publica.
- `MovementSet` — un `SystemSet`; el orden pertenece a quien arma el schedule.

### Fase 6 — `bof_simulation`

Aquí se cobran §20 y C2 de forma definitiva: sin `bevy` declarado, los 5
archivos que tocan `Mesh` no compilan hasta ceder eso a presentación, y
`KeyCode` deja de ser alcanzable fuera de su dueño.

**avian3d no es el obstáculo que parecía.** Declara `bevy` con
`default-features = false` y solo `["std", "bevy_log"]`, así que
`bevy_render` entra solo por sus features `debug-plugin` y `collider-from-mesh`,
ambas en `default` y ambas prescindibles: `collider-from-mesh` no se usa (**0**
llamadas a `from_mesh`/`trimesh_from`/`convex_hull_from`) y `debug-plugin` se
muda a presentación con `PhysicsDebugPlugin`, que ya vive en `main.rs`. Con
`default-features = false, features = ["3d","f32","parry-f32","parallel","xpbd_joints"]`,
un target headless que dependa solo de `bof_simulation` no linkea `bevy_render`.
Y paga una deuda que no es de arquitectura: esas features bifurcan nuestro árbol
de bevy del de los otros juegos y llenan el `build-dir` compartido (`AHORA.md`).

Obstáculo real a resolver primero: **`build.rs` incluye
`src/asset_pipeline/schema.rs` por `#[path]`** y `generated.rs` hace
`include!(OUT_DIR/authored_assets.rs)`. Al mover `schema.rs` a `crates/domain/`,
el `build.rs` se muda con él y el binario consume el resultado ya generado.

### Fase 7 — `bof_presentation`, el binario, y la decisión pendiente

¿Pila lineal o hermanas?

```text
Lineal:     domain ← simulation ← presentation ← app
Hermanas:   domain ← simulation ← app
            domain ← presentation ← app
```

**Recomendación: hermanas**, y la razón es de este juego. §20 dice
"presentación solo READ". Si presentación ve simulación, ve sus funciones y sus
sistemas, y "solo leer" vuelve a ser disciplina — la misma que hoy falla en 15
archivos. Si solo ve `domain`, que es dato puro, **leer es lo único que puede
hacer**.

El impuesto es real: todo lo que presentación lea tiene que ser dato en
`domain`, así que `domain` crece y el contrato entre capas se vuelve explícito.
La lista de la fase 5 dice que ya estamos casi ahí.

**Es la decisión que hay que tomar antes de empezar la fase 5**, porque
determina qué baja a domain.

---

## La frontera se prueba, no se declara

Cargo prohíbe la dependencia inversa, pero no prueba que la simulación siga
siendo la misma sin pantalla. Eso solo se prueba corriéndola: **el mismo
escenario headless y renderizado, exigiendo estado idéntico tick a tick**. Si
divergen, presentación está escribiendo verdad. Se apoya en la fase 2 — sin
semilla determinista esa comparación no existe — y el beneficio llega aunque el
co-op no llegue nunca: tests de simulación sin ventana ni GPU, en segundos.

Falta decidir qué es "estado idéntico" en un mundo abierto con avian:
probablemente `Transform`/`BodyVelocity`/`LocomotionState` de los actores en una
escena fija y un número acotado de ticks, no el mundo entero.

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
| 2 | Dos corridas con la misma semilla dan estado idéntico N ticks; `Entity::to_bits` no aparece en ningún cálculo de resultado. |
| 3 | `cargo clippy --all-targets -- -D warnings` limpio con los lints en `deny`. |
| 4 | `grep -rn "terrain.single()" src/` vacío; checkpoint jugado (esculpir, guardar, reentrar). |
| 5 | `cargo tree -p breath_of_freedom_domain` sin `bevy_render`. |
| 6 | `grep -rl "Mesh\|StandardMaterial" crates/simulation/src` vacío; `bevy_input` solo en el dueño de input; el test de la fase 1 se borra por redundante; checkpoint jugado. |
| 7 | `cargo tree` de presentation sin `bof_simulation`; checkpoint jugado; frame time sin regresión contra el baseline de `AHORA.md`. |
