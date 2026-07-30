# Texturas — reglas, contrato y hoja de ruta

Cómo se autoran, se cargan y se pagan las texturas de este juego: suelo, hojas,
cielo, horizonte, props, personajes y agua. El orden importa: el contrato
primero y la autoría/import después.

> **Cómo se usa este documento.** Un paso por vez. Cada paso se cierra con su
> entregable **jugado** y medido con el hub F1 antes de abrir el siguiente. Un
> paso que no se puede validar no se implementa. Este documento define el
> destino; código y `AHORA.md` muestran el estado vivo.
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
| El suelo cambia de material y no se ve el truco | Splatting: N suelos en **un** material, elegidos por vértice | Evita partir la malla y multiplicar lotes por tipo de suelo |
| Un parche de roca aparece al pintarlo, al instante | El índice de `TerrainKind` viaja en el vértice | Presentación lee el dato; no hay paso de horneado |
| El follaje no se come el frame | Albedo **opaco**, silueta en la geometría | Evita ordenar transparencias y preserva early-Z |
| Props y construcciones se ven nítidos sin saturar VRAM | Texturas fuente de 512² y runtime KTX2 transcodificable; MR/occlusion sólo cuando aporten | Reduce memoria y archivos sin prometer muestras que `StandardMaterial` no elimina |
| El cielo cambia con la hora sin cargar nada | Gradiente derivado de la paleta de luz | Un cubemap no se puede interpolar por hora sin pagar dos |
| El horizonte no termina en un filo | La niebla converge al color del cielo a esa altura | Si el color ya coincide, el borde no existe |
| Todo esto entra en una placa de 2 GB de 2016 | KTX2 transcodificable y resolución elegida por dirección | La GPU recibe BC en Polaris y ETC2/ASTC donde corresponda |

---

## Las cinco leyes de este sistema

### 1. El terreno no se parte por material

Un material extra implica otro lote/cambio de estado y puede sumar draws; una
textura extra dentro del mismo material agrega memoria y una muestra, pero no
obliga a partir la geometría.

De ahí sale la regla dura: **un tipo de suelo nuevo nunca es un
`StandardMaterial` nuevo.** Los suelos van en un `texture_2d_array` dentro de un
único material, y se eligen por atributo de vértice.

Si se viola, el terreno tiene que partirse en una malla por material. El array
permite cuatro suelos dentro del mismo material y la misma malla.

### 2. La opacidad es el enemigo

El baseline opaco preserva early-Z y evita ordenar superficies transparentes.
La silueta de vegetación se resuelve en geometría.

- **Alpha blending** obliga a ordenar y apaga el early-Z.
- **Alpha test (`Mask`)** también rompe el early-Z: el hardware no puede
  descartar el fragmento antes de correr el shader que hace el `discard`.

**Regla:** una textura del baseline **no lleva canal alfa** salvo excepción
escrita. La silueta va en la geometría. Las transiciones se hacen con
**crecimiento** o **dithering**, nunca con mezcla.

**Excepción objetivo:** fuego/humo de `PARTICLES.md`, con `Blend`, resolución
256² y cota global; no habilita alfa en vegetación ni materiales persistentes.

### 3. El array manda el contrato

Un `texture_2d_array` **exige por hardware** que todas sus capas tengan el mismo
tamaño y el mismo formato. No es una regla de estilo que se pueda relajar cuando
apure: es la condición para que el sistema funcione.

Por eso el contrato se escribe **antes** de bajar texturas, y se valida en
`build.rs`, con el mismo patrón del presupuesto de polígonos: fallando el build y
**nombrando el archivo**, no con un `warn!` que se lee tres días después.

### 4. La resolución la decide la dirección artística, no la fuente

Bajar 4K porque estaba disponible es cómo se llega a 88 MB de texturas para un
bosque. La dirección es **low-poly, flat-shaded** (ver `NORTE.md`): el detalle
fino de un albedo fotográfico lo tira la propia estética.

**Default: 512² albedo.** Se sube a 1024² solo donde el ojo se posa y con el
delta medido al lado.

### 5. Arte propio primero, procedencia siempre

Coherente con `NORTE.md`: el proyecto construye arte propio low-poly y no
depende de catálogos CC0.

- **Default:** fuentes creadas por el proyecto en Krita/Blender/GIMP, con una
  licencia SPDX declarada y compatible con la distribución del repositorio.
- **Fallback opcional:** una fuente externa CC0/dominio público puede servir de
  materia prima si su licencia se verifica por archivo. No se diseña una fase
  que necesite descargarla para poder avanzar.
- **Prohibición:** cero assets de pago, propietarios, freemium con DRM o
  licencias que exijan login/registro para reconstruir el artefacto.
- **Procedencia:** Las texturas standalone no tienen `GltfExtras`, así que no
  pueden usar `bof_license`. El import mantendrá un manifiesto RON
  `assets/textures/SOURCES.ron` con archivo, autor, origen/URL y licencia. Sólo una
  textura incorporada a un GLB hereda además el `bof_license` de su raíz.

---

## Nomenclatura y Empaquetado Canónico (glTF / Bevy)

```text
Fuente:  T_{Category}_{Name}_{Type}.png
Runtime: T_{Category}_{Name}_{Type}.ktx2
```

PNG es la fuente editable; **BC1/BC5 no son formatos PNG**, sino formatos GPU.
El import produce KTX2/Basis Universal y Bevy lo transcodifica al formato
soportado: BC en la Polaris, ETC2/ASTC en móvil. Un KTX2 con BC rígido no es el
artefacto portable del proyecto.

| Categoría | Fuente máxima | Runtime objetivo | Mapas | Razón técnica |
|---|---|---|---|---|
| `T_Ground` | 512² RGB | KTX2, 4 bpp por capa | `_Albedo` | Un `texture_2d_array`; incluye suelo, roca y arena |
| `T_Wood` / `T_Prop` | 512² RGB | KTX2 transcodificable | `_Albedo`, `_MR`, `_Occlusion` opcionales | Coincide con los slots reales de `StandardMaterial` |
| `T_Char` | 1024² RGB | KTX2 transcodificable | `_Albedo` por defecto | Visto de cerca; atlas de ropas/piel |
| `T_Water` | 512² RG | KTX2, normal de dos canales | `_Normal` | Normales tileables; BC5/ETC2 RG/ASTC según GPU |
| `T_FX` | 256² RGBA | KTX2 con alfa | `_Albedo` | Excepción sólo para fuego/humo bajo `VfxBudget` |

---

## Aritmética de referencia

Cuatro capas de suelo, según cómo se guarden:

| forma | VRAM |
|---|---|
| Cuatro suelos, albedo 512² RGBA8 | 5,3 MB |
| **Cuatro suelos, albedo 512² a 4 bpp (objetivo runtime)** | **0,7 MB** |

Aritmética, no medición: 512² a 4 bpp son 128 KB por capa. La compresión no es
una optimización para después — lleva el array completo a ~0,7 MB con mips,
contra 5,3 MB en RGBA8, y no se retrofitea barato.

---

## Presupuesto (decisión, no medición)

Igual que `lod0_triangle_budget`: son **conteos**, así que son deterministas,
testeables y pueden romper el build. Los milisegundos van por el otro carril.

| rubro | tope | por qué |
|---|---|---|
| Suelo del terreno (array entero) | **2 MB** | Es un fondo tileado; ver la cuenta de arriba |
| Una textura de suelo | **512², 4 bpp runtime** | Subir a 1024² es una decisión con delta medido |
| Mapas por suelo | **solo albedo** | Normal/rough/AO se agregan de a uno, con motivo |
| Texturas con alfa | **cero baseline** | Sólo la excepción fuego/humo de `PARTICLES.md`, bajo cota global |
| VRAM de texturas por escena | **≤ 64 MB** | Placa de destino: 2 GB, 2016 (`AHORA.md`) |

---

## Vista diagnóstica semántica del terreno

El aspecto artístico no puede ser la única forma de saber qué datos fueron
pintados. El hub F1 ofrece un selector persistente
`TerrainDebugView::{Off, Kind, Climbable, Flammable, Cuttable}`; no consume una
tecla global nueva.

| Vista | Color | Significado |
|---|---|---|
| `Kind` | café | `TerrainKind::Soil` / camino de tierra |
| `Kind` | gris | `TerrainKind::Rock` |
| `Kind` | verde | `TerrainKind::TallGrass` |
| `Kind` | ocre | `TerrainKind::Sand` |
| `Climbable` | rojo / casi negro | escalable / no escalable |
| `Flammable` | naranja / casi negro | inflamable / no inflamable |
| `Cuttable` | verde lima / casi negro | cortable / no cortable |

Las vistas de propiedad son separadas a propósito: una celda puede ser
inflamable y cortable a la vez, y una prioridad de colores ocultaría uno de los
datos. La leyenda visible del overlay muestra siempre el modo y ambos valores.

La presentación **lee** `TerrainKind::props()` y la autoridad de traversal; no
deduce gameplay a partir del color ni modifica simulación. `Off` restaura
exactamente los handles/materiales previos. Como toda vista diagnóstica, sus
materiales/draws se excluyen de muestras de rendimiento y del informe normal de
materiales.

Si “camino” adquiere propiedades distintas de `Soil`, primero se vuelve un
`TerrainKind` explícito con su fila de propiedades; un tinte café por sí solo no
crea una nueva semántica.

---

## Fase 1 — El suelo

### Paso 1: El contrato y la prueba mínima del array

- **Lógica.** Antes de bajar nada: `TerrainKind` → archivo de textura, en
  `schema.rs` como fuente única, y `build.rs` fallando si falta o si no mide lo
  que debe. Y una prueba mínima de que la técnica corre en la placa de destino:
  el terreno con `ExtendedMaterial<StandardMaterial, TerrainSplat>` muestreando
  un array de **dos** capas, usando fuentes temporales antes de importar arte
  definitivo. Es el primer riesgo técnico que debe despejar el plan: si el
  `ExtendedMaterial<StandardMaterial, TerrainSplat>` no funciona en los targets,
  se descubre antes de bajar veinte texturas.
- **Entregable & validación.** Con una capa el terreno conserva el aspecto del
  baseline plano; con la otra, cambia. Un draw call, un material — verificado en la sección
  `scene` del hub F1 (`draws`, `mats`), no a ojo.

### Paso 2: Los cuatro suelos, con el índice en el vértice

- **Lógica.** El índice de `TerrainKind` de la celda se hornea como atributo de
  vértice en `src/visuals/terrain.rs`, junto al color de vértice. El
  shader muestrea la capa. Bordes **duros** en el límite de celda: cada triángulo
  ya tiene vértices propios por el flat-shading, así que el borde cae natural y
  es coherente con la dirección visual.
- **Entregable & validación.** Pintar roca en el editor y ver roca, en el mismo
  draw call. `mats` no sube.

### Paso 3: Vista diagnóstica semántica

- **Lógica.** Incorporar `TerrainDebugView` al hub F1 y renderizar cada modo
  desde los datos semánticos, con paleta y leyenda canónicas.
- **Entregable & validación.** Pintar cada `TerrainKind` y alternar todas las
  vistas sin alterar el archivo de nivel, `TerrainKind`, colliders ni
  materiales restaurados al volver a `Off`.

### Paso 4: Compresión en el import

- **Lógica.** Crear fuentes 1K propias, reducir a 512² y convertir a
  KTX2/Basis Universal en el import automatizado, no a mano. En la Polaris se
  valida la transcodificación a BC; el build Android y un dispositivo de
  destino validan ETC2/ASTC. Una fuente externa CC0 verificada es un fallback,
  no una dependencia del paso.
- **Entregable & validación.** El array entero bajo el tope de 2 MB, con la
  imagen indistinguible del PNG a distancia de juego.

### Paso 5: Bordes difusos — **solo si molesta**

- **Lógica.** Pesos por esquina mirando las hasta 4 celdas que tocan cada punto
  de la grilla; la GPU interpola dentro del triángulo. Con 4 suelos entra en un
  `vec4` de pesos por vértice.
- **Decisión:** es deliberadamente opcional y último. Se hace sólo si el borde
  de 2,5 m se ve mal jugando. No toca ni el dato ni el editor.
- **Entregable & validación.** El borde deja de leerse como grilla.

---

## Fase 2 — Hojas, Follaje y Props

### Paso 6: Que las hojas no reintroduzcan alfa

- **Lógica.** Cualquier textura de hoja nueva nace sin alfa: la forma de la hoja
  va en la geometría de la carta y el albedo sólo aporta color y variación. Una
  conversión `Mask` → `Opaque` al cargar puede ser red de seguridad, no diseño.
- **Entregable & validación.** Un árbol con textura de hoja propia que no suba el
  frame time en la caja del bosque, y que el watchdog no marque materiales
  nuevos.

### Paso 7: Retirar la deuda de `Mask`

- **Lógica.** Toda carta heredada con `AlphaMode::Mask` debe mover la silueta a
  geometría o retirarse. `T_GrassCard_Albedo` es el caso de migración nominal.
- **Entregable & validación.** Cero `AlphaMode::Mask` en el proyecto, fijado por
  un test que recorra la paleta.

### Paso 8: Texturas de Props y Personajes

- **Lógica.** Props y estructuras reusan paletas compartidas (`M_Wood`,
  `M_Stone`, `M_Steel`). Albedo 512² primero; personajes usan atlas 1024².
  `StandardMaterial` ya combina metallic+roughness en una muestra, pero
  occlusion ocupa otro slot y otra muestra aunque reutilice la misma imagen:
  empaquetar ORM ahorra archivos/memoria, **no convierte tres búsquedas en una**.
  Como el baseline es mate/no metálico, MR/occlusion entran sólo con una mejora
  visible medida.
- **Entregable & validación.** Un prop texturado mantiene el material compartido
  y demuestra el costo de cada mapa opcional por separado.

---

## Fase 3 — Cielo, Agua y Horizonte

### Paso 9: El cielo es un gradiente, no una textura

- **Lógica.** Esto es lo que hay que decidir **antes** de buscar imágenes de
  cielo: un cubemap cuesta VRAM y **no se puede interpolar por hora del día** sin
  cargar y mezclar dos. El ciclo día/noche publica una paleta de luz y el cielo
  sale de esa misma paleta: gradiente vertical en una cúpula sin sombra, con
  discos de sol y luna encima.
- **Entregable & validación.** El amanecer se ve como un amanecer y `mats` sube
  como mucho en uno.

### Paso 10: El horizonte se cierra con niebla, no con geometría

- **Lógica.** El mismo principio de convergencia de `BOTWGrass.md`: lo lejano no se
  tapa, se hace **converger en color** con lo que tiene detrás. La `DistanceFog`
  toma el color del gradiente **a la altura del horizonte**, no el color medio.
- **Entregable & validación.** Desde el punto más alto que se pueda esculpir, no
  se distingue dónde termina el terreno.

### Paso 11: Olas de Agua tileables (Normal Map)

- **Lógica.** La presentación futura del agua consume la profundidad que publique
  el dueño de `WaterVolume` y combina color procedural con
  `T_Water_Normal.png`, 512², tileable y desplazado por UV. La normal perturba
  iluminación/especular; no se promete un reflejo plano que Bevy no aporta.
- **Entregable & validación.** Olas legibles bajo la iluminación existente, con
  delta de frame medido.

---

## Fuera de alcance (a propósito)

Triplanar mapping (el terreno no tiene paredes verticales: es un heightfield),
parallax/POM, tessellation, texturas de detalle a dos escalas, atlas virtual,
streaming de texturas, materiales PBR completos por suelo, y **cielo por
cubemap** — descartado con motivo en el Paso 9, no por costo de implementación.

## Cómo se mide

Hub **F1**, sección `scene`: `mats` y `draws` son los números que este documento
gobierna. La VRAM se calcula de dimensiones y formato y puede fijarse con un
test. El frame time se mide con secuencia A/B/A desde el mismo punto, warmup
excluido y al menos tres muestras por condición. Se reportan mediana y p95; un
delta que no supera la variación observada entre baselines se trata como ruido.
