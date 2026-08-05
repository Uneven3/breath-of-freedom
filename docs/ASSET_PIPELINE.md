# Pipeline de assets — Blender → Bevy

Contrato de autoría e integración de arte propio (**≤250 líneas**). Visión:
`NORTE.md`; leyes de capas: `ARCHITECTURE.md`; trabajo activo: `AHORA.md`;
texturas: `TEXTURES.md`; vegetación: `BOTWGrass.md`.

## Principios

- Blender 5.2 LTS es la fuente; runtime usa glTF 2.0 binario (`.glb`). Soltar un
  GLB válido y recompilar lo registra, sin una ruta ni una receta Rust por asset.
- Identidad de gameplay, apariencia y perfil espacial son **claves tipadas
  distintas**. Ninguna es una ruta ni un `Handle`.
- **Desacoplamiento de capas (§20):** presentación (`Update`) se queda con las
  mallas `SM_`/`SK_`, las UV y los materiales `M_`; simulación (`FixedUpdate`)
  con lógica pura en Rust. Una malla renderizada **nunca** es collider: los
  helpers `U*_` se hornean a datos puros en compilación y pierden su render, y
  `FixedUpdate` jamás lee una escena GLB.
- **Protección compile-time:** `build.rs` rechaza nomenclatura inválida, prefijos
  desconocidos, LODs discontinuos o licencia faltante, nombrando asset y regla.
- Pocos `StandardMaterial` mate compartidos. La belleza viene de paleta, luz y
  atmósfera, no de multiplicar materiales o polígonos.

## Carpetas

```text
art/blender/<categoria>/            fuentes propias .blend
art/vendor/<catalogo>/              fuentes y licencias de terceros
assets/game/authored/<categoria>/   GLB propios; scanner estricto
assets/game/legacy/<catalogo>/      runtime vendor aún necesario
assets/textures/<categoria>/        texturas (contrato en TEXTURES.md)
```

Categoría → directorio runtime: `char_`→`characters/`, `creature_`→`creatures/`,
`prop_`→`props/`, `structure_`→`structures/`, `tree_`→`trees/`,
`weapon_`→`weapons/`.

`assets/game/authored/` es la única frontera autodescubierta. Legacy no se valida
contra esta convención y conserva receta explícita hasta ser reemplazado.

**Hueco conocido: la geometría estampada.** Una brizna de pasto se consume al
hornear la malla del chunk y **nunca se spawnea como nodo de escena**, así que no
tiene collider, ni material de paleta, ni bandas `VisibilityRange` — su LOD es la
densidad por anillo, otro mecanismo. Ninguna categoría actual la describe, y
meterla como `prop_` haría que `build.rs` le exija reglas que no le aplican.
`BOTWGrass.md` propone una categoría propia con sus propias reglas (tope de tris;
prohibido `U*_`, `SKT_`, `M_` y sufijo `_LOD`). **Decisión abierta**, no
implementada.

## Tutorial recomendado: primero el contrato, después el detalle

No empezar esculpiendo: primero una versión gris que demuestre escala, pivote y
colisión, probada en Bevy; recién después silueta, materiales y LODs. Para
`prop_barrel`:

1. **Identidad y carpeta.** `art/blender/props/prop_barrel.blend`. El nombre del
   archivo define la clave estable; no es una ruta de gameplay.
2. **Metros, frente y pivote.** Metric/1, origen apoyado en el suelo, tamaño
   verificado junto a una referencia humana.
3. **La colisión primero.** Una primitiva Blender separada, ajustada al volumen
   jugable, con escala/rotación aplicadas, y con **objeto y mesh datablock**
   nombrados según lo que Bevy debe construir: `UBX_/USP_/UCY_/UCP_/UCX_`.
4. **El render alrededor del collider.** `SM_Barrel_LOD0`; nunca reutilizarlo
   como colisión. El helper se ve en Blender, pero el loader le quita el render.
5. **Paleta.** Reusar `M_Wood`, `M_Steel`… Un look realmente nuevo se aprueba y
   se agrega a la paleta *antes* de exportar.
6. **Raíz y ficha.** Todo bajo `ROOT_prop_barrel`, declarando `bof_license`,
   `bof_profile`, `bof_material_kind` y `bof_climbable`.
7. **Lo opcional al final.** Sockets `SKT_*`; después `LOD1/LOD2`; animaciones
   sólo para `SK_*`. Validar cada incremento.
8. **Exportar y compilar** con los comandos de "Export reproducible". Un fallo se
   corrige en Blender, nunca con escala, collider o material bespoke en Rust.

Árbol mínimo: `ROOT_prop_barrel` con `SM_Barrel_LOD0` `[M_Wood, M_Steel]`,
`UBX_Body` y `SKT_Top`.

La primitiva pierde su historial de "fui creada como Cube" al pasar a glTF:
queda como geometría. **Por eso el prefijo es el contrato autoritativo** — `UBX_`
significa "Bevy construirá un box desde estos bounds" aunque alguien deforme el
helper. Para que sea lo que dice ser: partir de la primitiva correspondiente, no
editar su topología, aplicar transforms. `UCX_` es la excepción — sus vértices
authored sí definen el hull. Un prefijo desconocido **falla el build**.

Tres controles, con responsabilidades distintas: el **exportador Blender**
rechaza nombres, transforms, jerarquía, materiales o LODs inválidos antes de
escribir el GLB; **`build.rs`** abre el GLB real y falla `cargo check` nombrando
asset y regla, además de hornear sockets/colliders a datos puros; y **el loader**
remapea paleta, aplica LOD, oculta helpers y comprueba `GltfExtras`, conservando
el proxy y logueando si la escena no carga.

Registro automático no significa spawn automático: soltar el GLB lo incorpora al
catálogo, pero qué identidad de gameplay lo usa y dónde aparece se sigue eligiendo
en `world/layout.rs`, por claves semánticas y nunca por paths ni handles.

## Sistema de coordenadas y escena Blender

- `Metric`, `Unit Scale = 1`: una unidad es un metro.
- Blender `+Z` arriba y frente `-Y`; el exportador produce Bevy `+Y` y `-Z`.
- Escala y rotación aplicadas en render meshes. Pivote en el suelo para
  estáticos, entre los pies para personajes.
- Una raíz `ROOT_<asset_key>` (nombre del archivo sin extensión). Nombres ASCII,
  únicos, sin sufijos automáticos `.001`.

## Tamaños de referencia (metros)

Escala común para juzgar proporciones nuevas contra lo ya integrado. La altura
de los authored sale de los bounds del manifiesto; la de los proxies, de las
primitivas en `visuals/forest.rs`.

| Asset | Tipo | Alto | Ancho / copa ⌀ | Notas |
| --- | --- | --- | --- | --- |
| Player graybox | cápsula | 2.0 | radio 0.5 | deriva de `BodyDimensions::PLAYER` |
| `tree_pine_a` | pino authored | 7.6 | copa ~3.5 | tronco `UCY_Trunk`; primera vertical propia |
| Proxies graybox | árbol | 7.4–8.5 | copa 3.6–4.0 | `TreeSilhouette::{Rounded,Conical,Gnarled}` |

El player mide ~2 m, así que un pino de 7.6 m es ~3.8× su altura. Todo asset
nuevo se dimensiona contra esta columna antes de exportar.

## Nomenclatura

### Archivos

`<categoria>_<nombre>[_<variante>].glb`, todo `lower_snake_case`:
`char_villager`, `tree_pine_a`, `prop_barrel`, `weapon_sword_short`.

### Render y LOD

- Estático: `SM_<Parte>_LOD0`; skinned: `SK_<Parte>_LOD0`.
- `Parte` usa PascalCase ASCII: `SM_Trunk_LOD0`, `SK_Body_LOD1`.
- `LOD0` obligatorio; los opcionales son contiguos hasta `LOD2`. Cada node y su
  mesh datablock comparten exactamente el mismo nombre.
- Bandas default: LOD0 0–30 m, LOD1 20–58 m, LOD2 50–70 m, con margen de
  transición mediante `VisibilityRange`. El perfil móvil puede acotar el final.

### Materiales

- `M_<ClavePaleta>`: `M_Bark`, `M_FoliagePine`, `M_Steel`. La clave debe existir
  en la paleta; el loader reemplaza el material importado por el único
  `Handle<StandardMaterial>` canónico, y una clave desconocida invalida el asset.
- Baseline: `metallic = 0`, `roughness ≥ 0.8`, sin textura salvo excepción medida
  y aprobada (reglas de textura: `TEXTURES.md`). Mismo look, misma clave.

### Sockets

- Empty `SKT_<Slot>`: `SKT_MainHand`, `SKT_OffHand`, `SKT_Canopy`. Su transform
  local se hornea al manifiesto: el attach de simulación lee ese dato puro, el
  visual puede seguir el node instanciado.

### Colisión

| Prefijo | Forma |
|---|---|
| `UCX_` | convex hull |
| `UBX_` | box |
| `UCP_` | capsule |
| `USP_` | sphere |
| `UCY_` | cylinder; extensión propia para troncos/pilares baratos |

Los helpers llevan nombre de propósito (`UCY_Trunk`, `UBX_Body_00`), no material
renderizable. `UCX_` lee vértices sólo de su mesh explícito; nunca deriva un
hull/trimesh del `SM_`/`SK_`. **Un prefijo fuera de esta tabla falla el build**
(hasta el 2026-07-26 caía en silencio a cilindro).

### Animaciones

- `AN_<Accion>[_<Variante>]`: `AN_Idle`, `AN_Walk`, `AN_AttackLight_01`.
- Un asset puramente `SM_` no exige ni admite clips; un `SK_` requiere clips
  nombrados, y los catálogos de animación conservan identidad propia.

#### Contrato de animación del player (plug and play + guardrail)

Los nombres de clip authored son un **contrato rígido con una sola fuente de
verdad**: `schema.rs::PLAYER_CLIP_CONTRACT`, consumida por `build.rs`. El player
actual es una cápsula sin rig; el resolvedor runtime se diseña con el primer
personaje compatible, no antes. Enforcement disponible:

- **Compile-time:** un GLB con `bof_animset =
  "player"` **falla el build** si le falta un clip `required`, listando cuáles.

**Lo que un asset `bof_animset = "player"` debe traer** — 13 clips `required`,
uno por motor de locomoción en producción:

```text
AN_Idle  AN_Walk  AN_Run   AN_Sneak  AN_Jump   AN_Fall  AN_Glide
AN_Climb AN_Ladder AN_Mantle AN_Vault AN_WallJump AN_EdgeLeap
```

La relación futura estado→rol y sus fallbacks pertenece al código del resolvedor
cuando exista. **Roles planeados** (`required: false`, validados si existen):
`AN_Swim`, `AN_Dive` y el eje direccional del modo facing-bloqueado (`…Bwd`,
`…StrafeL`, `…StrafeR`).

## Custom properties

Blender exporta propiedades `bof_*` a `extras`, que Bevy lee vía `GltfExtras`.
En la raíz:

- `bof_license`: SPDX o procedencia declarada; **obligatorio**.
- `bof_profile`: clave espacial estable si hay sockets o colisión.
- `bof_material_kind`: superficie semántica (`wood`, `stone`, …).
- `bof_climbable`: booleano; default `true` para geometría de mundo.
- `bof_animset`: opta al contrato de animación. Con `"player"` el build exige
  todos los clips `required` o falla nombrando los que falten; sin el extra, un
  `SK_` sólo exige que sus clips tengan forma `AN_*`.

El import build-time es **la autoridad para simulación**. La lectura runtime de
`GltfExtras` verifica consistencia y alimenta presentación/debug; nunca modifica
un collider en respuesta a una escena que terminó de cargar.

## Import en Bevy

1. `build.rs` escanea `assets/game/authored/` y valida archivo/directorio, raíz,
   nodes, materiales, LOD, animaciones, extras y geometría de colisión.
2. Genera en `OUT_DIR` un manifiesto con paths de presentación y descriptores
   espaciales puros. Duplicados o convenciones inválidas fallan el build
   nombrando el asset y la regla exacta.
3. `VisualCatalog` combina el manifiesto con recetas legacy; al instanciar, el
   loader remapea paleta, asigna rangos LOD, quita el render de los helpers y
   comprueba extras.
4. La representación anterior sigue visible durante la carga: éxito hace swap
   atómico, fallo conserva el fallback y loguea sin panic.

## Export reproducible

```text
timeout 120s blender -noaudio --background --factory-startup \
  --python tools/export_blender_asset.py \
  -- --source art/blender/<cat>/<key>.blend \
     --output assets/game/authored/<cat>/<key>.glb
```

Settings fijos: GLB, animaciones por actions, conversión Y-up, sin datos ajenos
al asset. El script valida antes de escribir y sale distinto de cero ante una
violación. El `timeout` evita además el cuelgue de cierre por PipeWire.

Validación de entrega: `cargo fmt` + `cargo clippy --all-targets -- -D warnings`
+ `cargo test`, y el checkpoint jugado por el usuario. Para assets visuales,
además F1 → material breakdown, flythrough y watchdog de triángulos. Sólo
entonces se retira el placeholder reemplazado; fuente, licencia y catálogo de
procedencia permanecen.

## Vegetación

**El tema es de `BOTWGrass.md`.** Lo único que toca a *este* contrato: la
vegetación authored usa la paleta mate (`M_Foliage*`) y **sin canal alfa**.

> Acá había una sección que describía en presente el billboarding hacia cámara,
> el subsurface scattering y la densidad por frustum. **Ninguna existe en el
> código** (verificado 2026-07-26). Se borró en vez de corregirse porque duplicaba
> a `BOTWGrass.md`, que existe justamente porque la doc anterior del pasto
> afirmaba cosas que la medición desmintió.
