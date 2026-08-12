# Texturas — reglas, contrato y hoja de ruta

Cómo se autoran, se cargan y se pagan las texturas de este juego: suelo, hojas,
cielo, horizonte, props, personajes y agua. Reescrito el **2026-08-05**: el
contrato y las leyes eran correctos, pero el documento no decía contra qué
máquina se justifica el presupuesto, daba por implementado en Bevy algo que este
build no tiene compilado, y describía como plan futuro tres pasos que ya están
hechos. Ver *Los errores que este documento ya cometió*.

> **Cómo se usa este documento.** Un paso por vez. Cada paso se cierra con su
> entregable **jugado** y medido con el hub F1 antes de abrir el siguiente. Un
> paso que no se puede validar no se implementa. Este documento define el
> destino; código y `AHORA.md` muestran el estado vivo.
>
> **Tres tipos de número, y no se mezclan.** *(a)* **Medición nuestra**: sale
> del hub F1, con fecha y escena. *(b)* **Aritmética verificable**: dimensiones
> × formato, o una propiedad del hardware objetivo. *(c)* **Decisión**: un
> presupuesto elegido, marcado como tal. Ninguna afirmación de rendimiento de
> nuestro juego entra sin caer en (a) — la misma regla que salió de
> `BOTWGrass.md`.

---

## El target manda: qué le hace un tiler a una textura

`NORTE.md` fija el piso —Android de gama media ~2021, tile-based— y
`BOTWGrass.md` desarrolla el marco. Acá sólo lo que es de este documento:

**1. Una textura no se paga en VRAM, se paga en muestras.** En el target la
memoria es LPDDR4X compartida con la CPU *(tipo b)*. Lo que duele no es que la
imagen ocupe 2 MB, es cuántos bytes cruzan el bus por píxel dibujado. Por eso el
formato comprimido importa más que la resolución: ETC2/ASTC bajan el tráfico de
cada muestra a un cuarto, en cada acceso, para siempre.

**2. Sin mipmaps, la textura del suelo es el peor caso posible.** Un suelo visto
en perspectiva minifica muchísimo: píxeles vecinos caen en texels lejanos, la
caché de textura falla en casi todos, y cada fallo es un viaje a memoria. Los
mips no son calidad de imagen: son **el mecanismo por el cual una superficie
grande deja de leer memoria al azar**. El terreno es el objeto de mayor
cobertura de pantalla del juego; es donde esto se cobra primero.

**3. La anisotropía se paga por muestra.** `anisotropy_clamp: 16` autoriza hasta
dieciséis lecturas por muestra en superficies inclinadas — y un terreno en
tercera persona está inclinado siempre. En escritorio es casi gratis; en el
target es fill multiplicado. Es un dial, y hoy está en su valor más caro.

**Y nada de esto está medido en el target.** Los milisegundos salen de la AMD
Polaris 11 del dev. La aritmética de bytes sí transfiere; los milisegundos no.

---

## Lo que se quiere ver, y qué lo produce

| Lo que se ve | Técnica que lo produce | Por qué |
|---|---|---|
| El suelo cambia de material y no se ve el truco | Splatting: N suelos en **un** material, elegidos por vértice | Evita partir la malla y multiplicar lotes por tipo de suelo |
| Un parche de roca aparece al pintarlo, al instante | El índice de `TerrainKind` viaja en el vértice | Presentación lee el dato; no hay paso de horneado |
| El follaje no se come el frame | Albedo **opaco**, silueta en la geometría | Evita ordenar transparencias y preserva early-Z |
| Props y construcciones se ven nítidos sin saturar memoria | Fuentes de 512² y runtime KTX2 transcodificable; MR/occlusion sólo cuando aporten | Reduce memoria y archivos sin prometer muestras que `StandardMaterial` no elimina |
| El cielo cambia con la hora sin cargar nada | Gradiente derivado de la paleta de luz | Un cubemap no se puede interpolar por hora sin pagar dos |
| El horizonte no termina en un filo | La niebla converge al color del cielo a esa altura | Si el color ya coincide, el borde no existe |
| Todo esto entra en un teléfono de gama media | KTX2 transcodificable y resolución elegida por dirección | La GPU recibe ETC2/ASTC en móvil y BC en la Polaris |

**Tres de estas filas todavía no son ciertas, y conviene saber cuáles:**

- **El follaje no es opaco.** Los troncos sí —`foliage.rs:96` fuerza
  `Mask → Opaque` porque sus texturas son alfa 255 en todas partes— pero las
  hojas **conservan `Mask` y `double_sided` a propósito**: sus texturas son
  70-82% no opacas, son cartas recortadas de verdad. Y `FoliageCard` monta
  `T_GrassCard_Albedo.png` con `AlphaMode::Mask(0.4)`
  (`asset_pipeline/materials.rs:139`). El baseline opaco es el destino, no el
  presente; la deuda tiene nombre y archivo (Paso 7).
- **El horizonte no se cierra.** La niebla tiene un techo de mezcla del 30%
  (`FOG_MAX_ALPHA`), así que el terreno lejano nunca se funde del todo. Es una
  decisión pendiente de `LIGHTING.md`, Fase 3, y el Paso 10 de acá depende de
  ella.
- **El KTX2 todavía no puede transcodificar.** Ver *Nomenclatura y empaquetado*.

---

## Las seis leyes de este sistema

### 1. El terreno no se parte por material

Un material extra implica otro lote/cambio de estado y puede sumar draws; una
textura extra dentro del mismo material agrega memoria y una muestra, pero no
obliga a partir la geometría.

De ahí la regla dura: **un tipo de suelo nuevo nunca es un `StandardMaterial`
nuevo.** Los suelos van en un `texture_2d_array` dentro de un único material y
se eligen por atributo de vértice. Implementado en
`visuals/terrain_material.rs` + `assets/shaders/terrain.wgsl`.

**Y hoy hay dos sistemas de suelo, no uno.** Además del array, la paleta expone
cuatro `StandardMaterial` (`GroundDirt`, `GroundGrass`, `GroundLeaves`,
`GroundPath`) con sus propias texturas, y `world/layout.rs:21` usa `GroundGrass`
como piso de las escenas graybox. No es una violación de la ley mientras se
entienda qué es cada cosa:

- **El terreno esculpido** (heightfield con semántica) es de la ley 1: array,
  un material, validado en `build.rs`.
- **El piso plano de graybox** es un prop, no terreno: no tiene `TerrainKind`,
  ni pendiente, ni semántica. Se retira cuando esas escenas tengan relieve.

Lo que sí es deuda: esos cuatro materiales usan texturas que **nadie valida**, y
una de ellas (`T_GroundGrass_Albedo`, 1024² RGBA + su `_Normal` de 1024²) rompe
las leyes 2 y 4 sin que nada lo detecte. Ver Paso 12.

### 2. La opacidad es el enemigo, y el dithering es la opacidad disfrazada

El baseline opaco preserva early-Z y evita ordenar superficies transparentes. La
silueta de vegetación se resuelve en geometría.

- **Alpha blending** obliga a ordenar y apaga el early-Z.
- **Alpha test (`Mask`)** también lo rompe: el hardware no puede descartar el
  fragmento antes de correr el shader que hace el `discard`.
- **Y el dithering igual.** Es alpha test con el umbral sacado de un patrón de
  ruido en pantalla: lleva `discard` y apaga LRZ/FPK/HSR exactamente igual. Su
  única ventaja real sobre el blending es que escribe depth correcto y no
  depende del orden de dibujo. Corregido desde `BOTWGrass.md`, ley 3.

**Regla:** una textura del baseline **no lleva canal alfa** salvo excepción
escrita. Las transiciones se hacen con **crecimiento**, que no necesita
`discard`.

**Excepciones vigentes, las dos escritas:**

- Fuego/humo de `PARTICLES.md`, con `Blend`, 256² y cota global.
- **La banda de `VisibilityRange`.** `foliage.rs` cruza `LOD_FADE = 12 m`
  dithereando, porque aparecer de golpe se lee peor que el costo. Es `discard`
  activo en el baseline actual: acotado a una banda estrecha y a los árboles,
  pero real, y hay que medirlo en el target antes de darlo por gratis.

### 3. El array manda el contrato

Un `texture_2d_array` **exige por hardware** que todas sus capas tengan el mismo
tamaño y el mismo formato. No es una regla de estilo: es la condición para que
el sistema funcione.

Por eso el contrato se escribe **antes** de bajar texturas y se valida en
`build.rs`, con el mismo patrón del presupuesto de polígonos: fallando el build
y **nombrando el archivo**. Implementado:
`crates/domain/build.rs::validate_terrain_textures` contra
`TERRAIN_TEXTURES`/`TERRAIN_TEXTURE_EDGE`.

### 4. La resolución la decide la dirección artística, no la fuente

Bajar 4K porque estaba disponible es cómo se llega a 88 MB de texturas para un
bosque. La dirección es **low-poly, flat-shaded** (`NORTE.md`): el detalle fino
de un albedo fotográfico lo tira la propia estética.

**Default: 512² albedo.** Se sube a 1024² sólo donde el ojo se posa y con el
delta medido al lado.

### 5. Una textura sin mips es una textura a medio hacer

Corolario del target, punto 2: la resolución elegida no sirve de nada si la
minificación lee memoria al azar. **Toda textura que cubra área grande o se vea
en perspectiva nace con su cadena de mips**, y el sampler que declara
`mipmap_filter: Linear` tiene que tener mips que filtrar.

Hoy el array del terreno **no los tiene**: `array_image` construye la imagen con
`Image::new`, que deja un solo nivel, aunque el sampler pida filtrado de mip y
anisotropía 16×. Es el hueco más caro del sistema de texturas y es invisible en
escritorio. Ver Paso 4.

### 6. Arte propio primero, procedencia siempre

Coherente con `NORTE.md`: el proyecto construye arte propio low-poly y no
depende de catálogos CC0.

- **Default:** fuentes creadas por el proyecto en Krita/Blender/GIMP, con
  licencia SPDX declarada y compatible con la distribución del repositorio.
- **Fallback opcional:** una fuente externa CC0/dominio público puede servir de
  materia prima si su licencia se verifica por archivo. Ninguna fase se diseña
  necesitando descargarla.
- **Prohibición:** cero assets de pago, propietarios, freemium con DRM o
  licencias que exijan login para reconstruir el artefacto.
- **Procedencia:** las texturas standalone no tienen `GltfExtras`, así que no
  pueden usar `bof_license`. El manifiesto es `assets/textures/SOURCES.ron`, con
  archivo, autor, origen/URL y licencia. Sólo una textura incorporada a un GLB
  hereda además el `bof_license` de su raíz.

**Estado: el manifiesto no existe.** Hay diez PNG en `assets/textures/` sin
procedencia declarada en ninguna parte del repo. Algunos casi con seguridad los
generó `tools/generate_terrain_textures.py` —o sea que son propios y la ley se
cumple de hecho— pero "casi con seguridad" no es una licencia. En un proyecto
GPL el manifiesto es la única forma de saber que se puede distribuir lo que se
está distribuyendo, y de distinguir lo generado de lo bajado. Ver Paso 0.

---

## Nomenclatura y empaquetado canónico (glTF / Bevy)

```text
Fuente:  T_{Categoria}{Nombre}_{Tipo}.png     ej. T_GroundSoil_Albedo.png
Runtime: T_{Categoria}{Nombre}_{Tipo}.ktx2
```

PNG es la fuente editable; **BC1/BC5 no son formatos PNG** sino formatos GPU. El
import produce KTX2 y Bevy lo entrega a la GPU en el formato que ésta soporte.

| Categoría | Fuente máxima | Runtime objetivo | Mapas | Razón técnica |
|---|---|---|---|---|
| `T_Ground` | 512² RGB | KTX2, 4 bpp por capa | `_Albedo` | Un `texture_2d_array`; incluye suelo, roca, pasto y arena |
| `T_Wood` / `T_Prop` | 512² RGB | KTX2 transcodificable | `_Albedo`, `_MR`, `_Occlusion` opcionales | Coincide con los slots reales de `StandardMaterial` |
| `T_Char` | 1024² RGB | KTX2 transcodificable | `_Albedo` por defecto | Visto de cerca; atlas de ropas/piel |
| `T_Water` | 512² RG | KTX2, normal de dos canales | `_Normal` | Normales tileables; BC5/ETC2 RG/ASTC según GPU |
| `T_FX` | 256² RGBA | KTX2 con alfa | `_Albedo` | Excepción sólo para fuego/humo bajo `VfxBudget` |

### Lo que este build puede leer hoy, verificado

`cargo tree -p bevy -f "{p} {f}"` el 2026-08-05: están activas `ktx2` y
`zstd_rust`; **`basis-universal` no**. Consecuencia exacta:

- Un KTX2 cuyo payload ya sea un formato GPU nativo (BC7, ETC2, ASTC), incluso
  supercomprimido con zstd, **carga bien**. Pero un archivo así ya no es
  portable: hay que producir uno por familia de GPU.
- Un KTX2 en UASTC/ETC1S —el formato "universal" que se transcodifica al vuelo
  al que la GPU soporte, que es lo que este documento promete— **no carga**.

Y el sabor de Bevy viene del workspace compartido de `uneven`
(`bevy = { version = "0.19", features = [...] }`), que es de todos los juegos.
La forma correcta de arreglarlo sin tocar a los demás es agregar la feature en
el `Cargo.toml` de este juego:

```toml
bevy = { workspace = true, features = ["basis-universal"] }
```

Hasta que eso ocurra, todo el plan de KTX2 está bloqueado, y los PNG actuales
—que Bevy descomprime a RGBA8 sin comprimir en GPU— son lo que realmente se
paga. Ver Paso 4.

---

## Aritmética de referencia *(tipo b)*

Cinco capas semánticas de terreno, según cómo se guarden:

| forma | memoria GPU |
|---|---|
| Cinco capas, albedo 512² RGBA8, **sin mips** (lo que corre hoy) | 5,2 MB |
| Cinco capas, albedo 512² RGBA8, con mips | 7,0 MB |
| **Cinco capas, albedo 512² a 4 bpp con mips (objetivo runtime)** | **0,8 MB** |

512² a 4 bpp son 128 KB por capa; la cadena de mips suma un tercio. La
compresión no es una optimización para después: es la diferencia entre 0,8 y
7,0 MB, y no se retrofitea barato. Y los mips **no son opcionales** aunque
cuesten un tercio más de memoria: compran accesos coherentes, que es el recurso
escaso (ley 5).

---

## Presupuesto *(tipo c: decisión, no medición)*

Igual que `lod0_triangle_budget`: son **conteos**, deterministas, testeables y
pueden romper el build. Los milisegundos van por el otro carril.

| rubro | tope | por qué |
|---|---|---|
| Suelo del terreno (array entero) | **2 MB** | Es un fondo tileado; ver la cuenta de arriba |
| Una textura de suelo | **512², 4 bpp runtime, con mips** | Subir a 1024² es una decisión con delta medido |
| Mapas por suelo | **sólo albedo** | Normal/rough/AO se agregan de a uno, con motivo |
| Texturas con alfa | **cero baseline** | Sólo la excepción fuego/humo, bajo cota global |
| Memoria de texturas por escena | **≤ 64 MB** | Ver abajo |

El tope de 64 MB estaba justificado contra la placa del dev (2 GB, 2016). Es la
máquina donde se **mide**, no la que hay que **aguantar**: el target es un
teléfono con 4-6 GB compartidos entre sistema, juego y GPU, del que una app
puede contar con bastante menos. 64 MB sigue siendo el número correcto, pero por
la razón correcta — y lo que lo hace alcanzable no es la capacidad sino la
compresión, porque en móvil ese presupuesto se llena cuatro veces más rápido si
las texturas viajan sin comprimir.

---

## Vista diagnóstica semántica del terreno

El aspecto artístico no puede ser la única forma de saber qué datos fueron
pintados. El hub F1 ofrece un selector persistente
`TerrainDebugView::{Off, Kind, Climbable, Flammable, Cuttable}`; no consume una
tecla global nueva. **Implementado** (`visuals/terrain_material.rs`, uniform
`debug.x` leído por `terrain.wgsl`).

| Vista | Color | Significado |
|---|---|---|
| `Kind` | café | `TerrainKind::Soil` / camino de tierra |
| `Kind` | verde claro | `TerrainKind::ShortGrass` |
| `Kind` | gris | `TerrainKind::Rock` |
| `Kind` | verde | `TerrainKind::TallGrass` |
| `Kind` | ocre | `TerrainKind::Sand` |
| `Climbable` | rojo / casi negro | escalable / no escalable |
| `Flammable` | naranja / casi negro | inflamable / no inflamable |
| `Cuttable` | verde lima / casi negro | cortable / no cortable |

Las vistas de propiedad son separadas a propósito: una celda puede ser
inflamable y cortable a la vez, y una prioridad de colores ocultaría uno de los
datos. La leyenda visible muestra siempre el modo y ambos valores.

La presentación **lee** `TerrainKind::props()` y la autoridad de traversal; no
deduce gameplay del color ni modifica simulación. `Off` restaura exactamente los
handles previos. Como toda vista diagnóstica, sus materiales/draws se excluyen
de las muestras de rendimiento.

Si "camino" adquiere propiedades distintas de `Soil`, primero se vuelve un
`TerrainKind` explícito con su fila de propiedades; un tinte café por sí solo no
crea una semántica nueva.

---

## Los errores que este documento ya cometió

1. **Dar por hecha una capacidad del motor sin verificar el build.** "Bevy lo
   transcodifica" es cierto de Bevy y falso de *este* Bevy: la feature no está
   compilada. Un plan entero apoyado en una línea de `Cargo.toml` que nadie
   había mirado.
2. **Calcular la memoria "con mips" de un array que se construye sin mips.** El
   número era correcto para el objetivo y no describía nada de lo que corre.
3. **Repetir que el dithering es la alternativa segura al blending.**
   `BOTWGrass.md` lo corrigió el 2026-08-04 y acá quedó la versión vieja. Un
   tema, un documento — pero cuando algo se comparte, se sincroniza.
4. **Presentar como plan futuro tres pasos ya implementados** (array, índice en
   el vértice, vista semántica). Un plan cuyo primer tercio ya ocurrió no se
   lee: se hojea.
5. **Justificar el presupuesto contra la placa del dev.** Es donde se mide, no
   lo que se apunta. Mismo error que `BOTWGrass.md` encontró en `NORTE.md`.
6. **No notar el segundo sistema de suelos.** Cuatro materiales de paleta con
   texturas de 256²/1024² y un normal map, fuera de toda validación, mientras la
   ley 1 declaraba que eso no podía existir.

---

## Fase 0 — Lo que falta antes de bajar una textura más

### Paso 0: El manifiesto de procedencia

- **Lógica.** Ley 6. Diez PNG en el repo sin licencia declarada, en un proyecto
  GPL, es una deuda que sólo se agranda.
- **Estado.** `assets/textures/SOURCES.ron` no existe.
- **Entregable & validación.** Una fila por archivo (ruta, autor, origen,
  licencia SPDX) y un test que falle si hay un PNG bajo `assets/textures/` sin
  fila. Barato, y hace imposible volver a acumular huérfanos.

---

## Fase 1 — El suelo

### Paso 1: El contrato y la prueba mínima del array

- **Lógica.** `TerrainKind` → archivo de textura como fuente única, `build.rs`
  fallando si falta o si no mide lo que debe, y el terreno con
  `ExtendedMaterial<StandardMaterial, TerrainExtension>` muestreando el array.
- **Estado.** Cinco capas 512² RGB, validadas en build, con fallback de color
  sólido de 1×1 mientras el arte no está. `ShortGrass` conserva una capa propia
  pero comparte temporalmente la fuente de `TallGrass`: falta autorar su albedo
  512² RGB en vez de intentar colar el antiguo `T_GroundGrass` 1024² RGBA.
- **Lo que enseñó.** El riesgo técnico que este paso existía para despejar
  —¿corre `ExtendedMaterial` con `texture_2d_array`?— está despejado en
  escritorio. En el target sigue sin probarse.

### Paso 2: Las capas semánticas, con el índice en el vértice

- **Lógica.** El índice de `TerrainKind` se hornea como atributo de vértice
  junto con la semántica de traversal; el shader muestrea la capa. Bordes duros
  en el límite de celda: cada triángulo ya tiene vértices propios por el
  flat-shading.
- **Estado.** **Hecho** (`semantic_vertex_data`, `terrain.wgsl`). El canal rojo
  del color de vértice lleva la capa; verde/azul/alfa llevan
  escalable/inflamable/cortable.

### Paso 3: Vista diagnóstica semántica

- **Estado.** **Hecho.** Ver la sección de arriba.

### Paso 4: Mips y compresión en el import

- **Lógica.** Es el paso que quedó, y ahora son tres cosas encadenadas:
  1. **Habilitar `basis-universal`** en el `Cargo.toml` del juego. Sin esto no
     hay KTX2 portable.
  2. **Generar la cadena de mips** al empacar el array (ley 5). Hoy
     `array_image` produce un solo nivel; el empaquetado tiene que producir los
     niveles o cargarlos ya hechos desde el KTX2, que es la vía natural: un
     KTX2 trae sus mips.
  3. **Convertir en el import automatizado**, no a mano: fuentes 1K propias →
     512² → KTX2. En la Polaris se valida la transcodificación a BC; el build
     Android y un dispositivo real validan ETC2/ASTC.
- **Estado.** No implementado. **Es el primer paso de este documento que puede
  cambiar un número en el target.**
- **Entregable & validación.** El array entero bajo el tope de 2 MB, con mips, e
  imagen indistinguible del PNG a distancia de juego. Y una lectura del dial de
  overdraw antes/después: el suelo es la superficie de mayor cobertura, así que
  si los mips valen algo, se ve ahí.

### Paso 5: Bajar la anisotropía a lo que se note

- **Lógica.** `anisotropy_clamp: 16` es el valor más caro posible y se eligió
  sin medir. En un terreno siempre inclinado, cada muestra puede costar hasta 16
  lecturas.
- **Entregable & validación.** A/B entre 16×, 4× y 1× **después** de que existan
  mips (antes no hay nada que comparar). Se elige el menor que no se vea peor
  jugando, y queda escrito acá con el número.

### Paso 6: Bordes difusos — **sólo si molesta**

- **Lógica.** Pesos por esquina mirando las hasta 4 celdas que tocan cada punto
  de la grilla; la GPU interpola dentro del triángulo. Con 4 suelos entra en un
  `vec4` de pesos por vértice.
- **Decisión:** deliberadamente opcional y último. Se hace sólo si el borde de
  2,5 m se ve mal jugando. No toca ni el dato ni el editor.

---

## Fase 2 — Hojas, follaje y props

### Paso 7: Que las hojas no reintroduzcan alfa

- **Lógica.** Cualquier textura de hoja nueva nace sin alfa: la forma va en la
  geometría de la carta y el albedo sólo aporta color y variación. Una
  conversión `Mask → Opaque` al cargar es red de seguridad, no diseño.
- **Entregable & validación.** Un árbol con textura de hoja propia que no suba
  el frame time en la caja del bosque, y que el watchdog no marque materiales
  nuevos.

### Paso 8: Retirar la deuda de `Mask`

- **Lógica.** Toda carta heredada con `AlphaMode::Mask` mueve su silueta a
  geometría o se retira. La deuda concreta, hoy: `FoliageCard` con
  `T_GrassCard_Albedo.png` (512² RGBA) en `Mask(0.4)`, más las hojas de los
  árboles Quaternius, que son cartas recortadas genuinas.
- **Entregable & validación.** Cero `AlphaMode::Mask` en el proyecto, fijado por
  un test que recorra la paleta. **Y el delta medido**: este cambio quita
  `discard`, o sea que devuelve early-Z; es de los pocos de este documento que
  debería moverse solo en el medidor.

### Paso 9: Texturas de props y personajes

- **Lógica.** Props y estructuras reusan paletas compartidas (`M_Wood`,
  `M_Stone`, `M_Steel`). Albedo 512² primero; personajes con atlas 1024².
  `StandardMaterial` ya combina metallic+roughness en una muestra, pero
  occlusion ocupa otro slot y otra muestra aunque reutilice la misma imagen:
  empaquetar ORM ahorra archivos y memoria, **no convierte tres búsquedas en
  una**. Como el baseline es mate y no metálico, MR/occlusion entran sólo con
  una mejora visible medida.
- **Entregable & validación.** Un prop texturado mantiene el material compartido
  y demuestra el costo de cada mapa opcional por separado.

---

## Fase 3 — Cielo, agua y horizonte

### Paso 10: El cielo es un gradiente, no una textura

- **Lógica.** Esto se decide **antes** de buscar imágenes de cielo: un cubemap
  cuesta memoria y **no se puede interpolar por hora** sin cargar y mezclar dos.
  El ciclo día/noche publica una paleta y el cielo sale de ella: gradiente
  vertical en una cúpula sin sombra, con los discos de sol y luna encima.
- **Estado.** Hoy el cielo es un `ClearColor` plano que ya sigue la paleta
  (`LIGHTING.md`). Falta la cúpula con gradiente.
- **Entregable & validación.** El amanecer se ve como un amanecer y `mats` sube
  como mucho en uno.

### Paso 11: El horizonte se cierra con niebla, no con geometría

- **Lógica.** El mismo principio de convergencia de `BOTWGrass.md`: lo lejano no
  se tapa, se hace **converger en color**. La niebla es de `LIGHTING.md`; este
  paso sólo aporta el requisito de textura: cuando el cielo sea un gradiente, la
  niebla tendrá que tomar el color **a la altura del horizonte**, no el color
  medio.
- **Bloqueado por** la decisión de `LIGHTING.md` Fase 3 sobre `FOG_MAX_ALPHA`:
  con un techo del 30% el horizonte no puede cerrarse, y el criterio de
  aceptación de este paso sería inalcanzable.

### Paso 12: Reconciliar los materiales de suelo de la paleta

- **Lógica.** `GroundDirt`, `GroundGrass`, `GroundLeaves` y `GroundPath` viven
  fuera de toda validación y una de sus texturas es 1024² RGBA con normal map.
  Las opciones son dos, y hay que elegir: **(a)** que esos pisos pasen al
  terreno esculpido y los materiales se borren, o **(b)** que se declaren props
  de graybox, se bajen a 512² sin alfa y entren al manifiesto y al presupuesto.
  La recomendación es (a) — un piso plano con textura de suelo es exactamente lo
  que el heightfield hace mejor — pero (b) es aceptable si las escenas de
  prueba lo necesitan.
- **Entregable & validación.** Cero PNG bajo `assets/textures/` sin dueño, sin
  fila en `SOURCES.ron` y sin validación de dimensiones en build.

### Paso 13: Olas de agua tileables (normal map)

- **Lógica.** La presentación futura del agua consume la profundidad que
  publique el dueño de `WaterVolume` y combina color procedural con
  `T_Water_Normal.png`, 512², tileable y desplazado por UV. La normal perturba
  iluminación y especular; no se promete un reflejo plano que Bevy no aporta.
- **Entregable & validación.** Olas legibles bajo la iluminación existente, con
  delta de frame medido.

---

## Fuera de alcance (a propósito)

Triplanar mapping (el terreno no tiene paredes verticales: es un heightfield),
parallax/POM, tessellation, texturas de detalle a dos escalas, atlas virtual,
streaming de texturas, materiales PBR completos por suelo, y **cielo por
cubemap** — descartado con motivo en el Paso 10, no por costo de implementación.

## Cómo se mide

Hub **F1**, sección `scene`: `mats` y `draws` son los números que este documento
gobierna, y el dial de overdraw es el instrumento para todo lo que tenga que ver
con muestreo del suelo. La memoria se calcula de dimensiones y formato y puede
fijarse con un test. El frame time se mide con secuencia A/B/A desde el mismo
punto, warmup excluido y al menos tres muestras por condición; se reportan
mediana y p95, y un delta que no supera la deriva entre baselines es ruido.
