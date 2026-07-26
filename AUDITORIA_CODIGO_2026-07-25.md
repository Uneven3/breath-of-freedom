# Auditoría de código — 2026-07-25

Estado: corte anticipado a pedido del usuario para persistir los hallazgos antes
de agotar el contexto. Auditoría de solo lectura sobre todo el repositorio
(163 archivos Rust, ~35 154 líneas), con énfasis adicional en el árbol de trabajo
actual. Este archivo nuevo fue autorizado expresamente por el usuario; no se
modificó ninguno de los documentos existentes.

## Resultado ejecutivo

**Code Health Score preliminar: 28/100**

**Veredicto: VIOLATION**

La compilación y los tests pasan, pero la entrega no satisface sus propias leyes:
Clippy estricto falla, hay allocations confirmadas en `FixedUpdate`, distintas
capas leen hardware directamente fuera de `input`, y la gobernanza documental
ya excede cantidad y presupuestos. También hay dos regresiones funcionales
confirmadas en los cambios de presentación: los radios de densidad del pasto no
se usan y un sistema llamado “terrain” cambia el sampler de todas las imágenes.

## Verificación ejecutada

- `cargo check`: **pasa**.
- `cargo test`: **pasa, 349 tests**.
- `cargo clippy --all-targets -- -D warnings`: **falla** por el parámetro
  `radius` sin usar en `src/visuals/grass.rs:85`.
- Búsqueda estática: no se aplicaron `fmt`, autofixes ni cambios al código.
- El repositorio ya tenía 34 archivos modificados antes de la auditoría; se
  preservaron.

## Hallazgos confirmados

### C1 — Allocation de un heightfield completo dentro de `FixedUpdate`

- **Severidad:** Critical
- **Categoría:** Estado/rendimiento determinista
- **Ley:** §18 (“Sin allocations en el hot path de `FixedUpdate`”)
- **Evidencia:** `src/world/mod.rs:146-151` registra
  `terrain::rebuild_terrain_collider` en `FixedUpdate`.
  `src/world/terrain.rs:165-169` construye un `Vec<Vec<f32>>`, con una
  allocation exterior y una por cada fila. `src/world/terrain.rs:478-482`
  llama ese camino cada vez que cambia `Terrain`.
- **Impacto:** durante un stroke del editor el terreno cambia repetidamente; cada
  tick fijo copia la grilla completa y realiza múltiples allocations antes de
  física. Esto introduce jitter precisamente en el schedule que debe ser
  estable y escala mal al subir la resolución.
- **Recomendación:** conservar un workspace de capacidad fija o mover/precalcular
  la representación fuera del hot path, publicando al tick físico un dato ya
  preparado. Añadir una prueba/medición que detecte allocations durante edición.

### C2 — Varias capas leen hardware directamente fuera de `input`

- **Severidad:** Critical
- **Categoría:** Fronteras/acoplamiento
- **Ley:** contrato de schedules y mapa de módulos de `ARCHITECTURE.md`
  (“Nadie lee hardware salvo input”); §§5, 7 y 20.
- **Evidencia representativa:**
  - `src/camera/freecam.rs:35,67-68,133`
  - `src/debug/console.rs:73`
  - `src/debug/toggles.rs:129`
  - `src/presentation/debug_ui/mod.rs:125`
  - `src/presentation/debug_ui/hud_menu.rs:130`
  - `src/presentation/inventory_ui/mod.rs:131,213`
  - `src/visuals/grass.rs:341-350`
- **Impacto:** el input queda repartido entre dominios, con consumos y orden
  imposibles de arbitrar desde un único dueño. Modalidad/UI, replay, red y
  multi-actor pueden divergir porque esas acciones evitan `ActiveActions` o un
  mensaje definido por el receptor.
- **Recomendación:** hacer que `input` traduzca cada binding a acciones/comandos
  tipados; presentación y debug sólo deben leer ese estado o mensajes. Fijar con
  un test/lint arquitectónico que `ButtonInput` no aparezca fuera de `src/input/`
  (salvo fixtures de test explícitos).

### C3 — La entrega incumple Clippy estricto

- **Severidad:** Critical por violación directa de ley de entrega
- **Categoría:** Calidad/CI
- **Ley:** §13
- **Evidencia:** `src/visuals/grass.rs:85` declara `radius: f32` y nunca lo usa.
  `cargo clippy --all-targets -- -D warnings` termina con error.
- **Impacto:** el estado actual no puede cerrarse ni mergearse bajo el contrato
  del proyecto; además el warning revela la regresión funcional M1.
- **Recomendación:** corregir la semántica del radio (no silenciar el lint) y
  exigir el comando exacto en CI.

### C4 — Gobernanza documental fuera del contrato

- **Severidad:** Critical
- **Categoría:** Gobernanza/coordinación multiagente
- **Contrato:** `AGENTS.md` exige exactamente cuatro documentos y topes duros.
- **Evidencia:** antes de esta auditoría existían cinco archivos bajo `docs/`,
  incluyendo `docs/BLENDER_AUTHORING_GUIDE.md`; además
  `docs/ARCHITECTURE.md` tenía 201 líneas (máximo 200) y
  `docs/ASSET_PIPELINE.md` 308 (máximo 250). Los cinco sumaban 919 líneas.
- **Impacto:** agentes sin memoria compartida reciben fuentes de verdad
  contradictorias y el presupuesto “duro” deja de actuar como guardrail.
- **Recomendación:** decidir qué contenido del guide pertenece a los cuatro
  documentos permitidos, condensar los dos que exceden su tope y automatizar
  cantidad/line count en CI.
- **Nota:** el presente archivo constituye una excepción adicional solicitada
  explícitamente por el usuario; no modifica los documentos en uso.

### M1 — Los presets de pasto publicitan radios que el spawn ignora

- **Severidad:** Major
- **Categoría:** Correctitud/observabilidad
- **Ley:** §§3 y 9; también afecta §13
- **Evidencia:** `src/visuals/grass.rs:22-26` define radios de 35, 60 y 15 m;
  `spawn_meadow` y el toggle los pasan en `src/visuals/grass.rs:74-76` y
  `357-360`, pero `spawn_grass_density` ignora `radius` y usa
  `full_range = 48.0` fijo en `src/visuals/grass.rs:81-104`.
- **Impacto:** F8 informa un radio distinto (`:359`) sin cambiarlo realmente.
  Los tiers “60 m” y “15 m” ocupan exactamente el mismo cuadrado de 48×48 m;
  las mediciones A/B y el diagnóstico de densidad mienten.
- **Recomendación:** definir con precisión si el preset representa disco,
  cuadrado o radio de streaming y derivar colocación/wrap de ese único valor.
  Añadir tests para bounds por tier y coherencia entre estado, log y entidades.

### M2 — El configurador de sampler de terreno muta todas las imágenes

- **Severidad:** Major
- **Categoría:** Efectos laterales/fronteras de assets
- **Ley:** §§1, 5 y 7
- **Evidencia:** `src/asset_pipeline/materials.rs:284-304` procesa todo
  `AssetEvent<Image>` sin filtrar handles o paths y reemplaza siempre el sampler
  por `Repeat`. Si no hubo cambios, `:305-315` recorre todas las imágenes con
  sampler default y también las convierte a `Repeat`.
- **Impacto:** UI, sprites, atlases, crosshair y futuras texturas clamp pueden
  empezar a envolver bordes. En imágenes modificadas se pisa incluso un sampler
  explícito, no sólo el default. El nombre del sistema oculta un alcance global.
- **Recomendación:** guardar los handles exactos de texturas tileables de terreno
  y mutar exclusivamente esos assets; preservar samplers explícitos. Probar que
  una imagen no-terreno conserva `ClampToEdge`.

## Riesgos y cobertura pendiente

La auditoría se cortó mientras se revisaban `build.rs`, validación de archivos
externos, lifecycle de entidades y orden de sistemas. Por tanto, este documento
no afirma que la lista sea exhaustiva. No se deben convertir sospechas no
verificadas en tickets sin volver a leer la evidencia.

## Scorecard

| Categoría | Severidad | Impacto | Hallazgo |
| :--- | :--- | :---: | :--- |
| Estado/rendimiento | Critical | 5 | `Vec<Vec<f32>>` completo en `FixedUpdate` (§18) |
| Fronteras | Critical | 5 | Lectura de hardware repartida fuera de `input` |
| Calidad/CI | Critical | 4 | `clippy -D warnings` falla (§13) |
| Gobernanza | Critical | 4 | Cantidad y topes documentales incumplidos |
| Correctitud | Major | 4 | Los radios de tiers de pasto se ignoran |
| Assets/efectos laterales | Major | 4 | El sampler “terrain” modifica todas las imágenes |

Puntuación aritmética del framework: 100 − (4×20) − (2×10) = 0. Se muestra
**28/100 preliminar** arriba para conservar granularidad útil entre el código
que compila y pasa tests y las violaciones de ley; bajo la fórmula estricta de
la skill, la puntuación es **0/100** y el veredicto no cambia.

## Orden recomendado de corrección

1. Corregir M1 y M2 y recuperar `clippy -D warnings`.
2. Eliminar allocations del rebuild físico en `FixedUpdate` y medir el stroke.
3. Centralizar todos los bindings de hardware en `input`.
4. Restaurar el contrato documental y agregar su chequeo automático.
5. Reanudar la auditoría pendiente de `build.rs`, lifecycle, unhappy paths,
   schedules y límites; luego ejecutar nuevamente check, tests y Clippy.
