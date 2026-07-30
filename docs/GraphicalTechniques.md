# Técnicas gráficas — estándar y hoja de ruta

Contrato objetivo para LOD, culling, batching, shading y presupuesto gráfico de
**Breath of Freedom** en Bevy 0.19. El código y `AHORA.md` describen el estado
vivo; este documento define lo que se quiere construir y cómo se acepta.

Los contratos especializados siguen teniendo un único dueño:

- `ASSET_PIPELINE.md`: nombres, jerarquía Blender→GLB y bandas LOD.
- `BOTWGrass.md`: representación, densidad y respuesta de la pradera.
- `TEXTURES.md`: formatos, alfa, procedencia y presupuesto de texturas.
- `LIGHTING.md`: luces, sombras y atmósfera.

---

## Resultado buscado

| Resultado visible | Técnica | Criterio |
|---|---|---|
| La silueta se conserva al alejarse | LOD solapado con `VisibilityRange` | No hay *popping* legible a distancia de juego |
| Bosque y pradera no saturan CPU | Handles compartidos y geometría agrupada | El número de entidades/draws no escala por brizna |
| El follaje no pierde el frame | Geometría opaca y cota de sombras | Sin alpha-test en vegetación baseline |
| Los objetos fuera de alcance desaparecen sin afectar gameplay | Frustum/distance culling sólo visual | Colliders y resultados de `FixedUpdate` no cambian |
| La escena cabe en el target móvil | Guardrails estáticos + inventario runtime | ≤100k tris, ≤100 draws, ≤64 materiales |
| El estilo sigue siendo legible y barato | `StandardMaterial` mate | Sin toon shader ni outline fullscreen baseline |

---

## Las seis leyes

### 1. El presupuesto es un contrato, no una advertencia

Cada GLB authored se rechaza en build si su LOD0 supera el presupuesto de su
categoría:

| Categoría | LOD0 máximo |
|---|---:|
| `prop` | 1.500 tris |
| `weapon` | 2.000 tris |
| `tree` | 3.000 tris |
| `structure` | 6.000 tris |
| `char` / `creature` | 15.000 tris |

La escena completa apunta además a `MOBILE_TRIANGLES = 100_000`,
`MOBILE_DRAWS = 100` y `MOBILE_MATERIALS = 64`. `SceneInventory` expone el
resultado en el hub F1 y sólo avisa al cruzar o recuperar un límite.

### 2. `ASSET_PIPELINE.md` es el único contrato de LOD

`LOD0` es obligatorio y cubre `0–30 m`; `LOD1` y `LOD2` son opcionales, pero
contiguos, con bandas default `20–58 m` y `50–70 m`. El loader aplica márgenes
de solapamiento dentro de esas bandas.

No se inventan rangos por categoría en otro documento. Si una medición exige
variantes, se agregan al catálogo como política explícita y se validan en build.

### 3. El culling de presentación no cambia simulación

Frustum culling y `VisibilityRange` pueden ocultar mallas; nunca eliminan
colliders, cambian `SurfaceKind` ni alteran un resultado de `FixedUpdate`.
El sensing LOD sí puede espaciar trabajo costoso de IA, pero conserva el último
hecho válido y calcula distancia al jugador más cercano, no a una entidad
singular asumida.

### 4. Compartir material no garantiza batching

Reusar `Handle<Mesh>` y `Handle<StandardMaterial>` reduce variantes y permite a
Bevy agrupar entidades compatibles. El batching exige además geometría,
pipeline y estado compatibles.

La pradera no promete hardware instancing: `BOTWGrass.md` agrupa miles de
briznas en una malla por chunk. Ese diseño paga una entidad/draw por chunk, no
por brizna.

### 5. El baseline es opaco y mate

El estilo usa `StandardMaterial`, `perceptual_roughness ≥ 0.8`,
`metallic = 0.0` y reflectancia baja. No hay toon shader, ramp shading ni
outline fullscreen baseline. Las excepciones de alfa pertenecen a
`PARTICLES.md`/`TEXTURES.md` y siempre tienen cota.

Un shader de rim/transmisión vegetal sólo entra como experimento opt-in,
medido, y bajo el documento dueño `BOTWGrass.md`.

### 6. Toda optimización conserva una comparación atribuible

Los cambios se prueban A/B con el mismo recorrido, cámara, hora y escena. El
modo de diagnóstico que agrega pases —wireframe u overdraw— nunca contamina la
muestra de rendimiento que pretende explicar.

---

## Orden de implementación

### Fase 1 — Guardrails y observabilidad

- Validar nombres, LOD contiguos y presupuestos LOD0 en `build.rs`.
- Mantener un `SceneInventory` con tris, draws y materiales visibles.
- Exponer el inventario y los diales de comparación en el hub F1.
- Mantener pruebas deterministas del costo estático de escenas declaradas.

### Fase 2 — LOD y culling visual

- Aplicar las bandas de `ASSET_PIPELINE.md` mediante `VisibilityRange`.
- Usar margen de transición (`LOD_FADE = 12 m`) sin convertirlo en alpha blend.
- Acotar la distancia de mallas y shadow casters por separado.
- Confirmar que los colliders y la simulación son invariantes ante cada dial.

### Fase 3 — Batching de vegetación

- Reusar mesh/material en proxies de bosque compatibles con instancing.
- Construir la pradera por chunks de geometría, según `BOTWGrass.md`.
- Mantener hojas y briznas baseline opacas; la silueta vive en geometría.
- Marcar briznas finas como `NotShadowCaster` cuando la medición confirme que
  sus cascadas compran ruido y no profundidad.

### Fase 4 — LOD de sensing y animación

- Sustituir cualquier ancla `Single<Player>` por la distancia mínima a todos
  los jugadores relevantes.
- Espaciar `SenseWorld` de actores lejanos sin borrar el último dato válido.
- Escalar reproducción de locomoción con
  `k_speed_node = V_real / V_autorada_node`, protegido cuando
  `V_autorada_node < 0.05`.
- Medir animación/IK con el presupuesto definido por
  `CHARACTER_ANIMATION_IK.md`.

### Fase 5 — Validación de target

- Ejecutar un flythrough reproducible con el perfil móvil, warmup excluido y
  secuencia A/B/A con al menos tres muestras por condición.
- Verificar ≤100k tris, ≤100 draws y ≤64 materiales en los puntos críticos.
- Registrar mediana/p95 de frame time; FPS sólo como traducción secundaria.
- Jugar el savepoint: una optimización que rompe silueta, lectura o control no
  se acepta aunque reduzca milisegundos.

---

## Fuera de alcance

Occlusion culling complejo, Nanite/virtualized geometry, impostores de alta
memoria, toon shading global y optimizaciones sin una medición A/B atribuible.
