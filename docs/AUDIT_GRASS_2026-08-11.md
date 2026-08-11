# Auditoría de lectura — pradera/billboards vs. documentación (2026-08-11)

Auditoría de solo lectura (código + git, sin `cargo run`/`cargo test`) pedida
para revisar si `AHORA.md` y `BOTWGrass.md` describen lo que el código hace
hoy, tras el commit `1facec5` (2026-08-10, "improve grass cards and visual
diagnostics"). No se tocó código. Metodología: leer `grass.rs`,
`grass_material.rs`, `card_mesh_lab.rs`, `grass_tiles.rs`, `grass_records.rs`,
`grass.wgsl` completos, y los diffs de `docs/BOTWGrass.md`, `docs/MVPS.md`,
`docs/NORTE.md`, `src/world/day_night.rs`, `src/perf/shot.rs`,
`src/visuals/diagnostic.rs` en ese commit.

## Resultado en una frase

**`BOTWGrass.md` está al día; `AHORA.md` no.** El commit más reciente
reemplazó por completo el mecanismo de carta que `AHORA.md` describe como
estado vigente, y ese cambio (carta texturizada, laboratorio de aprobación,
un fix de sombras no relacionado con el pasto) no dejó rastro en `AHORA.md`.

## Discrepancias

### 1. `AHORA.md` describe un mecanismo de recorte que el código ya no tiene

`AHORA.md` (líneas 359-371, "Técnica 1, 'suficientemente bien por ahora'
(2026-08-10)") dice que la carta recorta su silueta con **dientes
procedurales** (columnas 7/5 → 3/2) y que `CARD_SILHOUETTE_AREA` "queda en su
valor viejo (0,583)" como deuda, con "Técnica 2 y 3 para la próxima sesión".

El código hoy no recorta con geometría procedural en absoluto:
`assets/shaders/grass.wgsl` muestrea una textura PNG con alpha
(`T_GrassMeadowCard_Albedo.png`, función `sample_card_albedo`,
`grass.wgsl:134-136`, usada en el fragment principal `grass.wgsl:855-862` y
en el prepass `grass.wgsl:1069-1077`). `src/visuals/grass.rs:1450-1481`
tiene un test (`textured_card_contract_is_shared_by_colour_and_prepass`) que
**falla si el WGSL vuelve a contener** el símbolo del recorte dentado viejo
— el propio código certifica que ese mecanismo fue retirado, no solo
recalibrado.

En la misma fecha que la deuda anotada (2026-08-10), en el mismo commit, se
construyó un laboratorio aislado (`src/visuals/card_mesh_lab.rs`, nuevo, 406
líneas) para aprobar una carta ilustrada y esa carta reemplazó el mecanismo
dentado en la pradera de producción. `docs/BOTWGrass.md` documenta este giro
en ~116 líneas nuevas; `AHORA.md` quedó describiendo el estado que ese mismo
commit abandonó.

**Por qué importa:** quien abra `AHORA.md` hoy cree que el siguiente paso es
"Técnica 2: ruido en el borde del anillo". El código y `BOTWGrass.md` dicen
que hubo un desvío completo (carta texturizada) entre medio, sin cerrar
todavía su checkpoint jugado (ver punto 5).

### 2. `CARD_SILHOUETTE_AREA` no cambió de valor, pero sí de rol

Sigue en `0.583` (`src/visuals/grass.rs:415`), pero el comentario que lo
acompaña (`grass.rs:409-414`) ya no lo presenta como la receta activa de
recorte: dice que es la calibración de la carta procedural **anterior**, que
la carta ilustrada la conserva **provisionalmente**, y que su alpha efectivo
"se tiene que medir por distancia antes de cambiar densidad". `BOTWGrass.md`
coincide palabra por palabra. El número es el mismo; la afirmación que
`AHORA.md` hace sobre qué representa ese número ya no es correcta.

### 3. `day_night.rs` tiene un fix nuevo, sin relación con el bug de pasto ya documentado, y `AHORA.md` no lo menciona

`AHORA.md` (líneas 93-96) documenta un bug ya cerrado: `track_meadow_focus`
confundía `Sun` con `MoonLight` al buscar `sun_direction` — sigue arreglado
en `grass.rs:1029-1037`.

El commit `1facec5` agrega algo distinto: `shadow_casters()` en
`src/world/day_night.rs`, que garantiza que **como máximo una** de las dos
luces direccionales (sol/luna) tenga `shadow_maps_enabled = true` a la vez
— antes, cerca del horizonte, ambas podían castear sombra a la vez y
duplicar el costo de cascadas. Hay un test que barre el día minuto a minuto
(`at_most_one_directional_light_casts_shadows_throughout_the_day`).
`docs/MVPS.md` sí quedó actualizado ("Hecho en código [...]; falta
checkpoint jugado"); `AHORA.md` no dice nada de esto en ningún lugar,
aunque alimenta directamente `grass_data.sun_color`/`sun_direction`.

## Consistente (para no reportar falsos problemas)

- `CARDS_ENABLED = true` (`grass.rs:42`) y su historia (apagada/reencendida
  2026-08-09) — coincide con `AHORA.md`.
- Los tres anillos "uno por forma" (`grass.rs:117-130`) — coincide.
- Registro de 16 bytes / malla índice compartida / `MeshTag` — coincide con
  `BOTWGrass.md` y `grass_records.rs`.
- Depth+normal prepass propio (`build_blade` compartido) — coincide
  exactamente con `AHORA.md` § "Depth pre-pass: implementado y andando".
- Iluminación difusa + ambiente en vez de `apply_pbr_lighting` — coincide.
- Diffs de `docs/MVPS.md`/`docs/NORTE.md` en el commit: housekeeping honesto
  (marcar hecho, actualizar el plan de anillos a sus tres pasos reales); sin
  discrepancias encontradas ahí.
- `card_mesh_lab.rs` (7 especímenes, sin sombra, escena exclusiva
  `SceneId::CardMesh`) coincide con lo que describe `BOTWGrass.md`.

## Qué hace el sistema de cards/billboards hoy (mecanismo real)

Cada brizna elige su forma (`Leaf`/`Spike`/`Card`) por su ancho en píxeles de
pantalla, no por nivel — `blade_shape_at` (`grass.wgsl:232-238`), espejado en
Rust en `shape_at`/`card_from_m`/`spike_from_m` (`grass.rs:46-55,362-372`).
Una brizna `Card` se abre como billboard vertical contra el eje derecho de
cámara (`grass.wgsl:705-731`) con ancho variable por brizna (±30% sobre
1.0). La silueta ya **no** se recorta con geometría: fragment y prepass
muestrean la misma textura (`grass.rs:677`, cargada una vez y compartida
por los tres materiales de nivel) vía `sample_card_albedo`, alpha como
máscara (discard bajo 0,5 en el prepass; `AlphaToCoverage` en el material
principal). El color RGB de la textura modula además la luminosidad de la
brizna iluminada (`grass.wgsl:1014-1018`), sin reemplazar la iluminación
simplificada del resto del sistema. La densidad sigue derivándose de
`CARD_WIDTH` (0,25 → **0,30 m**) × `CARD_SILHOUETTE_AREA` (0,583, deuda sin
remedir) vía `footprint_m()` (`grass.rs:88-93`).

El laboratorio `card_mesh_lab` es una escena de prueba aislada que no toca
la pradera de producción — sirvió para aprobar la textura antes de
adoptarla ahí.

## Otras observaciones

- **Rama posiblemente muerta:** `src/perf/shot.rs:477` agrega
  `"forma" => Vec::new()` a `shot_categories`, pero `GRASS_DEBUG_STEPS`
  (`crates/domain/src/perf.rs:155-163`) solo admite `off/anillo/chunk/
  brizna/crecimiento/subpixel/medir` — `"forma"` no aparece en ninguna otra
  parte del árbol. No rompe nada (`match` inalcanzable), pero el comentario
  que lo acompaña sugiere una vista de diagnóstico que no llegó a
  conectarse.
- **Sin mips en la textura de carta:** `BOTWGrass.md` declara la falta de
  mips como deuda explícita ("a vigilar por shimmer durante el checkpoint
  jugado"), pero eso no aparece en `AHORA.md` ni en el código
  (`grass_material.rs`). Si el usuario juega y ve shimmer/moiré en las
  cartas lejanas, ésta es la causa más probable y hoy no está anotada donde
  él mira primero (`AHORA.md`).
- **Checkpoint jugado todavía abierto:** todas las secciones nuevas de
  `BOTWGrass.md` para este trabajo están marcadas `(abierto, 2026-08-10)` —
  el código ya adoptó la carta texturizada en producción, pero la
  validación jugada de ese cambio no se cerró formalmente todavía.

## Recomendación

No se tocó código ni docs en esta auditoría — es solo lectura. Antes de
seguir con Técnica 2/3 de los anillos (el plan que `AHORA.md` da como
próximo paso), conviene una sesión corta que:

1. Actualice `AHORA.md` con el resultado real del 2026-08-10 (carta
   texturizada, no dentada) y retire la mención a la deuda de dientes que ya
   no existe.
2. Cierre el checkpoint jugado de la carta texturizada (los `(abierto,
   2026-08-10)` de `BOTWGrass.md`) antes de invertir en Técnica 2/3, porque
   esas técnicas asumen la carta actual como base y todavía no está
   validada jugando.
3. Decida si la rama `"forma"` en `shot.rs` se termina de conectar o se
   borra — hoy es código muerto con intención sin cumplir.

## Seguimiento de esta sesión

La recomendación 1 ya se aplicó en `AHORA.md`: ahora describe la carta
texturizada, la calibración provisional de su área, la falta de mips y el
checkpoint jugado que sigue abierto. La rama `"forma"` se borró: no había vista
ni perilla que pudiera producirla, así que mantenerla sólo agrandaba un
`match` muerto. El checkpoint visual y la medición de cobertura siguen siendo
trabajo de pasto pendiente.

## Archivos leídos

`src/visuals/grass.rs`, `src/visuals/grass_material.rs`,
`src/visuals/card_mesh_lab.rs`, `src/visuals/grass_tiles.rs`,
`src/visuals/grass_records.rs`, `assets/shaders/grass.wgsl`,
`src/world/day_night.rs`, `src/perf/shot.rs`, `src/visuals/diagnostic.rs`,
`docs/AHORA.md`, `docs/BOTWGrass.md`, `docs/MVPS.md`, `docs/NORTE.md`,
`crates/domain/src/perf.rs`, y los diffs de `1facec5` sobre esos archivos.
