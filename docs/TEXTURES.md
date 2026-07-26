# Texturas — reglas, contrato y hoja de ruta

Cómo se autoran, se cargan y se pagan las texturas de este juego: suelo, hojas,
cielo y horizonte. Escrito el **2026-07-26**, antes de bajar un solo archivo
nuevo, porque el orden importa: el contrato primero y las texturas después.

> **Cómo se usa este documento.** Un paso por vez. Cada paso se cierra con su
> entregable **jugado** y medido con el hub F1 antes de abrir el siguiente. Un
> paso que no se puede validar no se implementa.
>
> **Honestidad de fuentes.** Todo número de este documento es **aritmética
> verificable** (dimensiones × formato) o **una medición con su fecha**. Los
> presupuestos son *decisiones* y están marcados como tales. Ninguna afirmación
> de rendimiento entra sin un número medido al lado — la misma regla que salió
> de `BOTWGrass.md`.

---

## Lo que se quiere ver, y qué lo produce

| Lo que se ve | Técnica que lo produce | Por qué |
|---|---|---|
| El suelo cambia de material y no se ve el truco | Splatting: N suelos en **un** material, elegidos por vértice | El costo es el material, no la textura |
| Un parche de roca aparece al pintarlo, al instante | El índice de `TerrainKind` viaja en el vértice | Presentación lee el dato; no hay paso de horneado |
| El follaje no se come el frame | Albedo **opaco**, silueta en la geometría | Alpha test rompe el early-Z (ya nos costó 13 FPS) |
| El cielo cambia con la hora sin cargar nada | Gradiente derivado de la paleta de luz | Un cubemap no se puede interpolar por hora sin pagar dos |
| El horizonte no termina en un filo | La niebla converge al color del cielo a esa altura | Si el color ya coincide, el borde no existe |
| Todo esto entra en una placa de 2 GB de 2016 | Compresión BCn y resolución elegida por dirección | RGBA8 sin comprimir es 8× lo que hace falta |

---

## Las cuatro leyes de este sistema

### 1. El costo es el **material**, no la textura

El medidor de este proyecto ya lo dice: el perfil móvil da "medio, **por
materiales**". Un material extra es un draw call y un cambio de estado; una
textura extra *dentro del mismo material* es casi gratis.

De ahí sale la regla dura: **un tipo de suelo nuevo nunca es un
`StandardMaterial` nuevo.** Los suelos van en un `texture_2d_array` dentro de un
único material, y se eligen por atributo de vértice.

Lo que pasa si se viola: el terreno tiene que partirse en una malla por material.
Es exactamente por lo que hoy existen cuatro suelos en la paleta y el terreno usa
**uno solo** — los otros tres no tienen forma de entrar sin partir la malla.

### 2. La opacidad es el enemigo

Ya pagamos esta lección: pasar el follaje de `Mask` a `Opaque` fue lo que llevó
el bosque de 13 a 60 FPS. `visuals/foliage.rs:96` todavía *fuerza* la conversión
al cargar, porque los assets vienen con `Mask` de fábrica.

- **Alpha blending** obliga a ordenar y apaga el early-Z.
- **Alpha test (`Mask`)** también rompe el early-Z: el hardware no puede
  descartar el fragmento antes de correr el shader que hace el `discard`.

**Regla:** una textura de este juego **no lleva canal alfa** salvo excepción
escrita. La silueta va en la geometría. Las transiciones se hacen con
**crecimiento** o **dithering**, nunca con mezcla.

**Excepción viva hoy:** `T_GrassCard_Albedo.png` se carga con
`AlphaMode::Mask(0.4)` (`asset_pipeline/materials.rs:138`). Es una deuda
declarada, no un permiso.

### 3. El array manda el contrato

Un `texture_2d_array` **exige por hardware** que todas sus capas tengan el mismo
tamaño y el mismo formato. No es una regla de estilo que se pueda relajar cuando
apure: es la condición para que el sistema funcione.

Por eso el contrato se escribe **antes** de bajar texturas, y se valida en
`build.rs` como ya se valida el presupuesto de polígonos — fallando el build y
**nombrando el archivo**, no con un `warn!` que se lee tres días después.

Hoy los assets violan esto: el pasto es 1024² y los otros tres suelos 256².

### 4. La resolución la decide la dirección artística, no la fuente

Bajar 4K porque estaba disponible es cómo se llega a 88 MB de texturas para un
bosque. La dirección es **low-poly, flat-shaded** (ver `NORTE.md`): el detalle
fino de un albedo fotográfico lo tira la propia estética.

**Default: 512² albedo.** Se sube a 1024² solo donde el ojo se posa y con el
delta medido al lado.

---

## Estado actual (verificado el 2026-07-26)

Inventario real de `assets/textures/`, con la VRAM calculada (dimensión × formato
× 1,33 por mips):

| textura | tamaño | mapas | VRAM hoy |
|---|---|---|---|
| `T_GroundGrass` | **1024²** | albedo, normal, roughness, AO, height | **26,6 MB** |
| `T_GroundDirt` | 256² | solo albedo | 0,33 MB |
| `T_GroundPath` | 256² | solo albedo | 0,33 MB |
| `T_GroundLeaves` | 256² | solo albedo | 0,33 MB |
| `T_GrassCard` | 512² | albedo con alfa (`Mask`) | 1,33 MB |

- **El terreno usa solo `GroundGrass`.** Los otros tres son entradas de paleta
  para cajas del graybox; no hay forma de que el terreno los use sin partirse.
- **`assets/grass_textures_1k/` es un duplicado exacto**, no un huérfano:
  verificado por MD5, `grass_02_base_1k.png` es byte por byte
  `T_GroundGrass_Albedo.png`, igual normal y roughness. Son los archivos fuente
  ya renombrados dentro de `assets/textures/terrain/`, commiteados dos veces.
- **El cielo no tiene textura**: es un `ClearColor` que `world/day_night.rs`
  mueve por hora.
- **El horizonte es `DistanceFog`** (`camera/mod.rs:106`), siguiendo el color del
  cielo.
- Anotado en `AHORA.md` y **no verificado en este documento**: el bosque ronda
  los 88 MB de RGBA8 sin comprimir.

### La cuenta que ordena todo lo demás

Cuatro suelos, mismo contenido, según cómo se guarden:

| forma | VRAM |
|---|---|
| Hoy: un set PBR a 1024² RGBA8 (**un** suelo) | 26,6 MB |
| Cuatro suelos, albedo 512² RGBA8 | 5,3 MB |
| **Cuatro suelos, albedo 512² BC1 (el objetivo)** | **0,7 MB** |

Aritmética, no medición: 512² BC1 son 128 KB por capa. La compresión no es una
optimización para después — es la diferencia entre 0,7 y 26,6 MB, y no se
retrofitea barato.

---

## Presupuesto (decisión, no medición)

Igual que `lod0_triangle_budget`: son **conteos**, así que son deterministas,
testeables y pueden romper el build. Los milisegundos van por el otro carril.

| rubro | tope | por qué |
|---|---|---|
| Suelo del terreno (array entero) | **2 MB** | Es un fondo tileado; ver la cuenta de arriba |
| Una textura de suelo | **512² BC1** | Subir a 1024² es una decisión con delta medido |
| Mapas por suelo | **solo albedo** | Normal/rough/AO se agregan de a uno, con motivo |
| Texturas con alfa | **cero nuevas** | Ley 2; las que hay son deuda declarada |
| VRAM de texturas por escena | **≤ 64 MB** | Placa de destino: 2 GB, 2016 (`AHORA.md`) |

---

## Fase 1 — El suelo

### Paso 1: El contrato y la prueba mínima del array

- **Lógica.** Antes de bajar nada: `TerrainKind` → archivo de textura, en
  `schema.rs` como fuente única, y `build.rs` fallando si falta o si no mide lo
  que debe. Y una prueba mínima de que la técnica corre en la placa de destino:
  el terreno con `ExtendedMaterial<StandardMaterial, TerrainSplat>` muestreando
  un array de **dos** capas, sin bajar texturas nuevas (sirven las que hay).
- **Estado.** No implementado. **Riesgo declarado:** sería el primer
  `ExtendedMaterial` que el proyecto logra usar. Ya hay uno a medio hacer para el
  pasto (`visuals/grass_material.rs` + `assets/shaders/grass.wgsl`), registrado
  pero sin usar y con el shader roto — escribe `out.position = world_position`
  cuando debe ser clip space. Por eso este paso existe separado: si la técnica no
  corre, se entera acá y no después de bajar veinte texturas.
- **Entregable & validación.** Con una capa el terreno se ve **idéntico** a hoy;
  con la otra, cambia. Un draw call, un material — verificado en la sección
  `scene` del hub F1 (`draws`, `mats`), no a ojo.

### Paso 2: Los cuatro suelos, con el índice en el vértice

- **Lógica.** El índice de `TerrainKind` de la celda se hornea como atributo de
  vértice en `visuals/terrain.rs`, junto al color que ya se escribe ahí. El
  shader muestrea la capa. Bordes **duros** en el límite de celda: cada triángulo
  ya tiene vértices propios por el flat-shading, así que el borde cae natural y
  es coherente con la dirección visual.
- **Estado.** No implementado. El tinte plano por celda que existe hoy
  (`kind_tint` en `visuals/terrain.rs`) es el andamio que este paso reemplaza.
- **Entregable & validación.** Pintar roca en el editor y ver roca, en el mismo
  draw call. `mats` no sube.

### Paso 3: Compresión en el import

- **Lógica.** Bajar 1K de ambientCG o Poly Haven (CC0), bajar a 512² y convertir
  a KTX2/BC1 en el paso de import, no a mano. Es la única forma de que la ley 4
  se cumpla sola.
- **Estado.** No implementado. Hoy todo es PNG RGBA8.
- **Entregable & validación.** El array entero bajo el tope de 2 MB, con la
  imagen indistinguible del PNG a distancia de juego.

### Paso 4: Bordes difusos — **solo si molesta**

- **Lógica.** Pesos por esquina mirando las hasta 4 celdas que tocan cada punto
  de la grilla; la GPU interpola dentro del triángulo. Con 4 suelos entra en un
  `vec4` de pesos por vértice.
- **Estado.** No implementado, y **deliberadamente último**. Se hace solo si el
  borde de 2,5 m se ve mal jugando. No toca ni el dato ni el editor.
- **Entregable & validación.** El borde deja de leerse como grilla.

---

## Fase 2 — Hojas y follaje

### Paso 5: Que las hojas no reintroduzcan alfa

- **Lógica.** El follaje ya es opaco por la fuerza (`foliage.rs:96` convierte
  `Mask` → `Opaque` al cargar). Cualquier textura de hoja nueva tiene que nacer
  sin alfa: la forma de la hoja va en la geometría de la carta, y el albedo solo
  aporta color y variación.
- **Estado.** El bosque usa proxies procedurales; el tier detallado es opt-in
  (`tree-detail`). La conversión forzada es una red de seguridad, no un diseño.
- **Entregable & validación.** Un árbol con textura de hoja propia que no suba el
  frame time en la caja del bosque, y que el watchdog no marque materiales
  nuevos.

### Paso 6: Retirar la deuda de `Mask`

- **Lógica.** `T_GrassCard_Albedo` es hoy la única textura con alfa viva. O su
  silueta pasa a la geometría, o el prop se retira.
- **Estado.** Vivo en `materials.rs:138`. La pradera ya no usa esos props (ver
  `BOTWGrass.md`), así que puede ser una eliminación y no un rediseño.
- **Entregable & validación.** Cero `AlphaMode::Mask` en el proyecto, fijado por
  un test que recorra la paleta.

---

## Fase 3 — Cielo y horizonte

### Paso 7: El cielo es un gradiente, no una textura

- **Lógica.** Esto es lo que hay que decidir **antes** de buscar imágenes de
  cielo: un cubemap cuesta VRAM y **no se puede interpolar por hora del día** sin
  cargar y mezclar dos. El ciclo día/noche ya existe y ya calcula una paleta de
  luz; el cielo tiene que salir de esa misma paleta. Un gradiente vertical en una
  cúpula sin sombra, con los discos de sol y luna que ya existen encima.
- **Estado.** Hoy es un `ClearColor` plano por hora — el gradiente es el
  siguiente escalón, y sigue sin costar una textura.
- **Entregable & validación.** El amanecer se ve como un amanecer y `mats` sube
  como mucho en uno.

### Paso 8: El horizonte se cierra con niebla, no con geometría

- **Lógica.** El mismo principio que el Paso 3 de `BOTWGrass.md`: lo lejano no se
  tapa, se hace **converger en color** con lo que tiene detrás. La `DistanceFog`
  ya sigue el cielo; falta que siga el color del gradiente **a la altura del
  horizonte**, no el color medio.
- **Estado.** `DistanceFog` lineal 45→240 m, ≤30%, siguiendo el `ClearColor`.
- **Entregable & validación.** Desde el punto más alto que se pueda esculpir, no
  se distingue dónde termina el terreno.

---

## Fuera de alcance (a propósito)

Triplanar mapping (el terreno no tiene paredes verticales: es un heightfield),
parallax/POM, tessellation, texturas de detalle a dos escalas, atlas virtual,
streaming de texturas, materiales PBR completos por suelo, y **cielo por
cubemap** — descartado con motivo en el Paso 7, no por costo de implementación.

## Cómo se mide

Hub **F1**, sección `scene`: `mats` y `draws` son los números que este documento
gobierna. La VRAM se calcula de las dimensiones y el formato — es aritmética, y
por eso puede ser un test en vez de una medición. El frame time se mide con la
secuencia A/B desde el mismo punto, dos corridas, quedándose con la limpia; un
delta menor a la deriva entre baselines (~0,05 ms) es ruido.
