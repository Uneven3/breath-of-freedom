# Pipeline de assets — Blender → Bevy

Contrato de autoría e integración de arte propio. Presupuesto: **≤250 líneas**.
Visión visual: `NORTE.md`; leyes de capas: `ARCHITECTURE.md`; migración activa:
`AHORA.md`.

## Principios

- Blender 5.2 LTS es la fuente de autoría; runtime usa glTF 2.0 binario (`.glb`).
- Soltar un GLB válido y recompilar lo registra sin agregar una ruta o receta
  Rust por asset.
- Identidad de gameplay, apariencia y perfil espacial son claves tipadas
  distintas. Ninguna es una ruta ni un `Handle`.
- Mesh renderizado nunca es collider. Colisiones y sockets authored se importan
  como datos puros antes de `Startup`; `FixedUpdate` nunca lee escenas, huesos o
  `AnimationPlayer`.
- Pocos `StandardMaterial` mate compartidos. La belleza proviene de paleta, luz
  y atmósfera, no de multiplicar materiales o polígonos.

## Carpetas

```text
art/blender/<categoria>/              fuentes propias .blend
art/vendor/<catalogo>/                fuentes y licencias de terceros
assets/game/authored/<categoria>/     GLB propios; scanner estricto
assets/game/legacy/<catalogo>/        runtime vendor aún necesario
```

Categorías y directorios runtime:

| Prefijo | Directorio |
|---|---|
| `char_` | `characters/` |
| `creature_` | `creatures/` |
| `prop_` | `props/` |
| `structure_` | `structures/` |
| `tree_` | `trees/` |
| `weapon_` | `weapons/` |

`assets/game/authored/` es la única frontera autodescubierta. Legacy no se
valida contra esta convención y conserva receta explícita hasta ser reemplazado.

## Tutorial recomendado: primero el contrato, después el detalle

No empezar esculpiendo. Primero construir en Blender una versión gris que
demuestre escala, pivote y colisión; probarla en Bevy; recién después agregar
silueta, materiales y LODs. Para `prop_barrel`:

1. **Elegir identidad y carpeta.** Crear
   `art/blender/props/prop_barrel.blend`. El nombre define la clave estable; no
   es una ruta de gameplay.
2. **Fijar metros, frente y pivote.** Usar unidades Metric/1, apoyar el origen
   en el suelo y verificar el tamaño junto a una referencia humana.
3. **Crear la colisión primero.** Agregar una primitiva Blender separada
   (Cube, UV Sphere, Cylinder o Capsule), ajustarla al volumen jugable, aplicar
   escala/rotación y nombrar objeto **y mesh datablock** según lo que Bevy debe
   construir: `UBX_Body`, `USP_Body`, `UCY_Body` o `UCP_Body`.
4. **Crear el render alrededor del collider.** El primer mesh es
   `SM_Barrel_LOD0`; nunca reutilizarlo como colisión. El helper puede verse en
   Blender, pero el loader le quita su render al instanciar el GLB.
5. **Asignar paleta.** Reusar `M_Wood`, `M_Steel`, etc. Si el look ya existe,
   no crear otra clave. Un look realmente nuevo se aprueba y agrega a la
   paleta antes de exportar.
6. **Agregar raíz y ficha.** Parentar todo bajo `ROOT_prop_barrel` y declarar
   `bof_license`, `bof_profile`, `bof_material_kind` y `bof_climbable`.
7. **Agregar lo opcional al final.** Primero sockets `SKT_*`; después
   `LOD1/LOD2`; animaciones sólo para `SK_*`. Validar cada incremento.
8. **Guardar, exportar y compilar** con los comandos de “Export reproducible”.
   Un fallo debe corregirse en Blender, no parchearse con escala, collider o
   material bespoke en Rust.

Árbol mínimo esperado:

```text
ROOT_prop_barrel
├── SM_Barrel_LOD0   [M_Wood, M_Steel]
├── UBX_Body
└── SKT_Top
```

La primitiva de Blender pierde su historial de “fui creada como Cube” al
convertirse en glTF: queda como geometría. Por eso el prefijo es el contrato
autoritativo. `UBX_` significa “Bevy construirá un box desde estos bounds”,
aunque alguien deforme el helper; `UCY_` significa cylinder, etc. Para que “sea
lo que dice ser”, partir de la primitiva correspondiente, no editar su
topología y aplicar transforms. `UCX_` es la excepción: sus vértices authored
sí definen el convex hull.

Hay tres controles, con responsabilidades distintas:

1. **Exportador Blender:** rechaza nombres, transforms, jerarquía, materiales o
   LODs inválidos antes de escribir el GLB.
2. **Bevy al compilar:** `build.rs` abre el GLB real; si ruta, extras, helper,
   material o geometría contradicen el contrato, `cargo check` falla nombrando
   asset y regla. También hornea sockets/colliders a datos puros.
3. **Bevy al instanciar:** remapea la paleta, aplica LOD, oculta helpers y
   comprueba `GltfExtras`. Si la escena no carga conserva el proxy y loguea el
   error. Verla bien en pantalla completa la validación.

Registro automático no significa spawn automático. Soltar el GLB lo incorpora
al catálogo; todavía se elige explícitamente qué identidad de gameplay lo usa y
dónde aparece en `world/layout.rs`. Ese binding menciona claves semánticas
(`prop_barrel`, `prop_barrel_body`), nunca paths, meshes ni handles.

## Sistema de coordenadas y escena Blender

- Unidades `Metric`, `Unit Scale = 1`; una unidad equivale a un metro.
- Blender: `+Z` arriba y frente del asset `-Y`; el exportador produce Bevy
  `+Y` arriba y frente `-Z`.
- Escala y rotación aplicadas en render meshes. Pivote/origen en suelo para
  estáticos y entre los pies para personajes.
- Una raíz `ROOT_<asset_key>`, donde `asset_key` es el nombre del archivo sin
  extensión. Ejemplo: `ROOT_tree_pine_a`.
- Nombres ASCII, únicos y sin sufijos automáticos `.001`.

## Nomenclatura

### Archivos

`<categoria>_<nombre>[_<variante>].glb`, todo `lower_snake_case`.

Ejemplos: `char_ranger_female`, `tree_pine_a`, `prop_barrel`,
`weapon_sword_short`.

### Render y LOD

- Estático: `SM_<Parte>_LOD0`; skinned: `SK_<Parte>_LOD0`.
- `Parte` usa PascalCase ASCII: `SM_Trunk_LOD0`, `SK_Body_LOD1`.
- `LOD0` es obligatorio; LODs opcionales son contiguos hasta `LOD2`.
- Cada node y su mesh datablock comparten exactamente el mismo nombre.
- Bandas default: LOD0 0–30 m, LOD1 20–58 m, LOD2 50–70 m, con margen de
  transición mediante `VisibilityRange`. El perfil móvil puede acotar el final.

### Materiales

- `M_<ClavePaleta>`: `M_Bark`, `M_FoliagePine`, `M_Steel`.
- La clave debe existir en la paleta del engine. El loader reemplaza el material
  importado por el único `Handle<StandardMaterial>` canónico.
- Baseline: `metallic = 0`, `roughness ≥ 0.8`, sin textura salvo excepción
  medida y aprobada. Una clave desconocida invalida el asset.
- Mismo look implica misma clave; no se crean variantes por malla.

### Sockets

- Empty `SKT_<Slot>`: `SKT_MainHand`, `SKT_OffHand`, `SKT_Canopy`.
- Su transform local se hornea al manifiesto espacial. Attach de simulación lee
  ese dato puro; attach visual puede seguir el node instanciado.

### Colisión

| Prefijo | Forma |
|---|---|
| `UCX_` | convex hull |
| `UBX_` | box |
| `UCP_` | capsule |
| `USP_` | sphere |
| `UCY_` | cylinder; extensión propia para troncos/pilares baratos |

Los helpers llevan nombre de propósito (`UCY_Trunk`, `UBX_Body_00`), no material
renderizable. `UCX_` puede leer vértices sólo de su mesh de colisión explícito;
nunca deriva un hull/trimesh del `SM_`/`SK_`.

### Animaciones

- `AN_<Accion>[_<Variante>]`: `AN_Idle`, `AN_Walk`,
  `AN_AttackLight_01`.
- Un asset puramente `SM_` no exige ni admite clips. Un asset `SK_` requiere
  clips nombrados; los catálogos de animación conservan identidad propia.

## Custom properties

Blender exporta propiedades `bof_*` a `extras`; Bevy las observa mediante
`GltfExtras`. En la raíz:

- `bof_license`: SPDX o licencia/procedencia declarada; obligatorio.
- `bof_profile`: clave espacial estable si hay sockets o colisión.
- `bof_material_kind`: superficie semántica (`wood`, `stone`, etc.).
- `bof_climbable`: booleano; default `true` para geometría de mundo.

El import build-time es la autoridad para simulación. La lectura runtime de
`GltfExtras` verifica consistencia y alimenta presentación/debug; nunca modifica
un collider en respuesta a una escena que terminó de cargar.

## Import en Bevy

1. `build.rs` escanea recursivamente `assets/game/authored/`.
2. Valida archivo/directorio, raíz, nodes, materiales, LOD, animaciones, extras
   y geometría de colisión.
3. Genera en `OUT_DIR` un manifiesto con paths de presentación y descriptores
   espaciales puros. Duplicados o convenciones inválidas fallan el build con el
   asset y la regla exacta.
4. `VisualCatalog` combina el manifiesto con recetas legacy.
5. Al instanciar, el loader remapea paleta, asigna rangos LOD, elimina render de
   helpers de colisión y comprueba extras.
6. La representación anterior permanece visible durante la carga; éxito hace
   swap atómico, fallo conserva fallback y loguea sin panic.

## Export reproducible

```text
timeout 120s blender -noaudio --background --factory-startup \
  --python tools/export_blender_asset.py -- \
  --source art/blender/<categoria>/<asset_key>.blend \
  --output assets/game/authored/<categoria>/<asset_key>.glb
```

Settings fijos: GLB, selección/colección de export, animaciones por actions,
conversión Y-up y sin datos ajenos al asset. El script valida antes de escribir y
sale distinto de cero ante una violación. En este host el `timeout` también
evita el cuelgue de cierre por PipeWire.

Validación de entrega:

```text
cargo fmt --package breath-of-freedom
cargo clippy --all-targets -- -D warnings
cargo test
```

El usuario hace el checkpoint Wayland. Para assets visuales ejecuta además F1 →
material breakdown, flythrough y watchdog de triángulos. Sólo entonces se
retira la dependencia runtime placeholder reemplazada; fuente, licencia y
catálogo de procedencia permanecen.
