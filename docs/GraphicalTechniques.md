# Técnicas Gráficas, Auditoría de Implementación y Estándares de la Industria

Documento de referencia técnica sobre **LODs, Culling, Instancing/Batching, Shading y Optimización Gráfica** para **Breath of Freedom** en **Bevy 0.19 (Vulkan/WebGL2)**, incluyendo la auditoría de estado de implementación en el código fuente (`src/`).

---

## 1. Auditoría de Estado de Implementación en el Proyecto

A continuación se detalla qué técnicas gráficas y de optimización están **ya aplicadas e integradas** en el motor del juego, verificando su cumplimiento estricto con el ECS y las Leyes de Arquitectura (§1–§21).

### A. Vegetación de Pradera sin Alpha por Chunks (`src/visuals/grass.rs`)
- **Estado:** **APLICADO Y VALIDADO EN CÓDIGO.**
- **Detalle Técnico ([grass.rs:L1-L100](file:///home/francisco/Programming/uneven/breath-of-freedom/src/visuals/grass.rs#L1-L100)):**
  - **Sin Alpha Overdraw:** Cada brizna de hierba es una primitiva 3D de 2 triángulos teñida con gradiente de color de vértices (`ROOT_COLOR` a `TIP_COLOR`), eliminando texturas transparentes y el overdraw en GPU.
  - **Chunk Mesh Grouping:** En lugar de crear un `Entity` de Bevy por brizna (lo cual saturaría el gestor de entidades a $45/\text{m}^2$), se hornean miles de briznas en **un solo mesh por chunk** de $5\text{m} \times 5\text{m}$ (`GrassChunk`).
- **Cumplimiento de Arquitectura (§20 - Desacoplamiento Estricto):** **100% Correcto.** La simulación en `FixedUpdate` ignora la existencia del mesh de hierba. El suelo subyacente reporta `SurfaceKind::Grass` a través de datos puros. La pradera es $100\%$ visual en `Update`.

### B. Culling por Distancia con Margen Progresivo (`src/visuals/foliage.rs`)
- **Estado:** **APLICADO Y VALIDADO EN CÓDIGO.**
- **Detalle Técnico ([foliage.rs:L140-L165](file:///home/francisco/Programming/uneven/breath-of-freedom/src/visuals/foliage.rs#L140-L165)):**
  - Utiliza el componente nativo de Bevy `VisibilityRange` asignando un margen de desvanecimiento suave (`LOD_FADE = 12.0` metros):
    `end_margin: (max - LOD_FADE).max(0.0)..max`.
- **Cumplimiento de Arquitectura (§20 & ECS):** **100% Correcto.** Se ejecuta en el pipeline visual `Update` sobre componentes `VisibilityRange`. Jamás altera las cápsulas de colisión ni los colliders de física en `FixedUpdate`.

### C. Guardrail de Presupuesto Poligonal Compile-Time (`src/asset_pipeline/schema.rs` & `build.rs`)
- **Estado:** **APLICADO Y VALIDADO EN CÓDIGO.**
- **Detalle Técnico ([schema.rs:L253-L280](file:///home/francisco/Programming/uneven/breath-of-freedom/src/asset_pipeline/schema.rs#L253-L280)):**
  - La función `lod0_triangle_budget` bloquea el comando `cargo check` / `cargo build` si un archivo `.glb` supera su presupuesto de triángulos por categoría:
    - `prop`: $1,500$ triángulos.
    - `weapon`: $2,000$ triángulos.
    - `tree`: $3,000$ triángulos.
    - `structure`: $6,000$ triángulos.
    - `char` / `creature`: $15,000$ triángulos.
- **Cumplimiento de Arquitectura (§18 & Build Guardrails):** **100% Correcto.** La validación ocurre en tiempo de compilación mediante `build.rs`, garantizando 0 sobrecosto en runtime.

### D. Monitoreo de Escena y Watchdog de Triángulos (`src/perf/budget.rs`)
- **Estado:** **APLICADO Y VALIDADO EN CÓDIGO.**
- **Detalle Técnico ([budget.rs:L5-L50](file:///home/francisco/Programming/uneven/breath-of-freedom/src/perf/budget.rs#L5-L50)):**
  - El recurso `SceneInventory` monitorea los presupuestos móviles máximos en tiempo real:
    - Triángulos visibles: $\le 100,000$.
    - Draw calls: $\le 100$.
    - Materiales únicos: $\le 64$.
  - Evalúa la calificación de rendimiento (`Good`, `Medium`, `Bad`, `Critical`) y emite advertencias al presionar **F1** o **F8**.
- **Cumplimiento de Arquitectura (§6 & §20):** **100% Correcto.** Es un sistema de observabilidad pura encapsulado en recursos del ECS.

### E. Mapeo de Animaciones e Invariante de Roles (`src/visuals/animation.rs`)
- **Estado:** **APLICADO Y VALIDADO EN CÓDIGO.**
- **Detalle Técnico ([animation.rs:L23-L82](file:///home/francisco/Programming/uneven/breath-of-freedom/src/visuals/animation.rs#L23-L82)):**
  - Enums `AnimationRole` y resolvedor `ROLE_TABLE` que mapea `LocomotionState` con los clips del contrato `PLAYER_CLIP_CONTRACT` (`AN_<Rol>`). Degrada suavemente hacia `Idle` si falta un clip específico.
- **Cumplimiento de Arquitectura (§20 & §21):** **100% Correcto.** Lee el `LocomotionState` producido en `FixedUpdate` y actualiza el `AnimationPlayer` de Bevy en `Update` / `PostUpdate`.

### F. Sensing LOD para Física e IA (`src/movement/lod.rs`)
- **Estado:** **APLICADO Y VALIDADO EN CÓDIGO.**
- **Detalle Técnico ([lod.rs:L1-L67](file:///home/francisco/Programming/uneven/breath-of-freedom/src/movement/lod.rs#L1-L67)):**
  - Aplica LOD a la **simulación** (no solo a los gráficos): desactiva ticks de escaneo pesados (`SenseWorld`) para actores distantes de la cámara.
- **Cumplimiento de Arquitectura (§19 & ECS):** **100% Correcto.** Aplica exclusivamente sobre componentes de `MovementSet` en `FixedUpdate`.

---

## 2. Puntos de Atención e Invariantes a Corregir en Código

Durante la sexta ronda de auditoría técnica cruzada, se identificaron 3 áreas puntuales de refactorización en `src/` para garantizar la compatibilidad multijugador y la seguridad contra panics:

1. **Sensing LOD Multijugador ([lod.rs:L73-L80](file:///home/francisco/Programming/uneven/breath-of-freedom/src/movement/lod.rs#L73-L80)):**
   - *Hallazgo (corregido el 2026-07-26):* `PlayerAnchor` utiliza `Option<Single<...>>`. Con 2 o más jugadores **no hay panic** — `Single` que no matchea exactamente una entidad hace que el `Option` dé `None`, y el sistema corre igual. El problema real es más silencioso, y por eso peor: el LOD de sensado se queda **sin ancla** y degrada sin avisar a nadie.
   - *Acción Planeada:* Sustituir `Single` por `Query<&Transform, With<Player>>` e iterar buscando la distancia mínima al jugador más cercano. **La conclusión sigue en pie**; lo que estaba mal era el mecanismo, no la acción.

2. **Consulta Segura de Terreno en Hierba ([grass.rs:L112](file:///home/francisco/Programming/uneven/breath-of-freedom/src/visuals/grass.rs#L112)):**
   - *Hallazgo retirado el 2026-07-26: era incorrecto por partida doble.* En Bevy 0.19 `Query::single()` **devuelve `Result`** — que es precisamente por qué el `.ok()` compila — así que `terrain_query.single().ok()` **no puede paniquear**: ya es el manejo seguro. Y `get_single` **no existe** en esta versión (0 usos en todo el repo; fue renombrado a `single`), de modo que aplicar la acción propuesta ni siquiera compilaría.
   - *Acción:* ninguna. El código ya es correcto.

3. **Escalado Dinámico de Velocidad en Animación ([animation.rs:L555](file:///home/francisco/Programming/uneven/breath-of-freedom/src/visuals/animation.rs#L555)):**
   - *Hallazgo:* `animate_player` utiliza actualmente multiplicadores de velocidad fijos (`1.0` / `-1.0`).
   - *Acción Planeada:* Implementar la fórmula del plan de movimiento $k_{speed\_node} = V_{real} / V_{autorada\_node}$ (protegida si $V_{autorada} < 0.05$).

---

## 3. Estándar de Nivel de Detalle (Level of Detail - LOD)

El sistema de LOD reduce la densidad poligonal y el costo de shading conforme los objetos se alejan de la cámara.

### A. Regla de Selección de LODs por Categoría
No todos los objetos requieren la misma cantidad de LODs. La regla se aplica mediante autodetección en Blender y se valida en compilación (`build.rs`):

| Categoría | Niveles de LOD | Rangos de Distancia | Razón Técnica |
|---|---|---|---|
| **Estructuras (`structure_`)** | **3 LODs** (`LOD0, LOD1, LOD2`) | `LOD0`: 0–30m, `LOD1`: 20–55m, `LOD2`: 50–90m | Dominan la silueta en el horizonte. 3 niveles evitan el "popping" de silueta. |
| **Vegetación Grande (`tree_`)** | **3 LODs** (`LOD0, LOD1, LOD2`) | `LOD0`: 0–30m, `LOD1`: 20–55m, `LOD2`: 50–80m | Gran masa poligonal. El paso $100\% \to 45\% \to 15\%$ suaviza la densidad en distancias medias. |
| **Props y Cajas (`prop_`)** | **2 LODs** (`LOD0, LOD1`) | `LOD0`: 0–25m, `LOD1`: 20–50m | Objetos de tamaño medio. A más de 25m ocupan pocos píxeles en pantalla. |
| **Armas (`weapon_`)** | **2 LODs** (`LOD0, LOD1`) | `LOD0`: 0–15m, `LOD1`: 10–35m | Altamente detalladas de cerca; en el suelo a distancia se simplifican drásticamente. |
| **Personajes (`char_`, `creature_`)** | **2 LODs** (`LOD0, LOD1`) | `LOD0`: 0–30m, `LOD1`: 25–60m | El jugador se enfoca en la silueta. `LOD1` simplifica dedos y ropajes secundarios. |

---

## 4. Técnicas de Culling (Descarte de Geometría Invisible)

El culling evita que la CPU o la GPU procesen geometría fuera de la pantalla.

### A. Frustum Culling (Descarte por Pirámide de Visión)
- **Funcionamiento:** Se calcula la caja delimitadora (AABB) de cada mesh. Si la AABB no intersecta los 6 planos del frustum de la cámara, Bevy descarta el asset en CPU antes de emitir el *Draw Call*.

### B. Distance Culling & Smooth Fade
- **Descarte por Alcance:** Objetos decorativos pequeños se ocultan completamente al superar su distancia máxima (ej. 70m).
- **Smooth Alpha Fade:** Utiliza `VisibilityRange` de Bevy con `end_margin` progresivo de 12 metros (`LOD_FADE`).

---

## 5. Batching e Instancing (Optimización de Draw Calls)

### A. GPU Hardware Instancing (Instanciación Indirecta)
- **Concepto:** Una sola malla se sube una sola vez a VRAM y se dibuja con una tabla de transformaciones por instancia.
- **Uso en el Juego:** Bosques de pinos (`visuals/forest.rs`) y praderas de hierba (`visuals/grass.rs`).

### B. Compartición de Materiales (`Handle<StandardMaterial>`)
- Todos los assets reusan un número reducido de materiales de paleta canónicos (`M_Bark`, `M_Wood`, `M_Steel`, `M_FoliagePine`). Al compartir el mismo material, Bevy agrupa las llamadas de renderizado automáticamente.

---

## 6. Shading y Estética Cel-Shading (Estilo BOTW)

### A. Cel-Shading / Ramp Shading
- Iluminación cuantizada mediante funciones de escalón o rampas para sombras planas marcadas en personajes y vegetación.

### B. Subsurface Scattering (SSS) / Translucidez Vegetal
- **Efecto Contraluz (Rim Light):** Difusión de luz que transmite un tono verde brillante en las puntas de la vegetación al estar a contraluz del sol.

---

## 7. Presupuesto y Optimización para Móviles (Vulkan / TBDR)

1. **Presupuesto Poligonal Máximo:** $\le 100,000$ triángulos en pantalla por frame (`MOBILE_TRIANGLES`).
2. **Cascaded Shadow Maps (CSM):** Máximo **2 cascadas de sombras**. Briznas finas de pasto llevan `NotShadowCaster`.
3. **Comandos de Diagnóstico:** Presionar **F1** para abrir el Hub de Diagnóstico (Flythrough benchmark, Watchdog de triángulos, Overdraw).
