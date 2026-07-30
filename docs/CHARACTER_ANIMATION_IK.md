# Plan de Mezcla de Animaciones e IK (Pies y Manos) para Personajes

Hoja de ruta técnica para la **mezcla de animaciones**, el **mapeo/retargeting de rigs** y el sistema de **Cinemática Inversa (IK)** para pies y manos en **Bevy 0.19** con **Rust** y **Avian3D**.

> Este documento define el sistema que se quiere construir. Código y
> `AHORA.md` indican qué parte existe en cada momento.
>
> **Principios de Arquitectura (§1, §6, §7, §14, §19, §20, §21):**
> 1. **Desacoplamiento Estricto (§20):** La física y el movimiento (`FixedUpdate`) jamás leen huesos ni ejecutan solvers de IK. El IK y el blending de animaciones se ejecutan exclusivamente en **`PostUpdate`** sobre entidades visuales (`PlayerVisual` / `Armature`), encadenados `.after(bevy::animation::animate_targets)` y `.before(TransformSystem::TransformPropagate)` para evitar que `bevy_animation` borre los ajustes de IK.
> 2. **Ortogonalidad de Estados (§1, §6, §19):** `LocomotionState` es la SSoT de piernas. La carga y empuje de objetos (`ObjectManipulationState`) es un estado ortogonal en las manos/tronco procesado por su propio árbitro en `FixedUpdate` (§7). Cargar una vasija no destruye `LocomotionState` cuando el personaje salta o cae por un precipicio.
> 3. **Rendimiento e Invariante de Simulación (§6, §18):** Sin asignaciones dinámicas en el hot-path por tick. El solver IK de pies incluye un IK LOD gate (descarte automático a $>30\text{m}$) y lee datos de piso puros (`FootingFacts` / la API planeada `Terrain::height_and_normal_at`) sin realizar raycasts de física en la fase de presentación.


> **Contrato de la API planeada `Terrain::height_and_normal_at`:** tiene que
> muestrear la anti-diagonal — de `(row, col+1)`
> a `(row+1, col)` — y devolver la **normal de cara** de ese triángulo, no una
> normal por diferencias centrales ni una interpolación bilineal. Motivo: parry
> triangula su heightfield por esa diagonal y el collider es la superficie
> autoritativa. El criterio de aceptación compara la altura devuelta contra el
> collider en todo el grid y exige error bajo la tolerancia numérica.

---

## Estructura General del Pipeline

```text
[FixedUpdate: Simulación]
  LocomotionState / ObjectManipulationState / Intents / BodyVelocity / WaterFacts / FootingFacts
                                 │
                                 ▼ (Lectura Pura en PostUpdate)
[PostUpdate: Animación e IK]
  1. bevy::animation::animate_targets ──► Aplica keyframes de la animación base
  2. Blend & State Resolver ───────────► Selecciona y mezcla capas en AnimationGraph (Lower & Upper)
  3. Bone Mapping / Rig ──────────────► Sincroniza nombres de huesos y rest-poses
  4. IK LOD Gate Check (< 30m) ────────► Si es lejano o no visible, salta IK
  5. Foot IK System (.after(animate)) ─► Lectura FootingFacts / Terrain ──► Hips Lowering Acotado ──► 2-Bone IK
  6. Hand IK System (.after(animate)) ─► Sockets (Armas, Cajas, Vasijas) / LootGestureEvent (Local Pos) ──► 2-Bone IK
  7. TransformPropagate ──────────────► Actualización de mallas skinned para GPU
```

---

## Las Cuatro Leyes de este Sistema

1. **El esqueleto visual no afecta la física:** La cápsula de locomoción en `FixedUpdate` es la autoridad de colisión y movimiento. Si el IK flexiona una pierna o baja la cadera, la cápsula no cambia de tamaño ni posición.
2. **El solver de IK es analítico de 2 huesos (Closed-Form 2-Bone IK):** No se usan métodos iterativos pesados (FABRIK / CCD) en extremidades de 2 segmentos (muslo-pantorrilla-pie, hombro-antebrazo-mano). Se usa la Ley de los Cosenos ($O(1)$) para garantizar 60 FPS estables.
3. **El terreno, datos de piso y sockets son la fuente de verdad de IK:** Los pies detectan la superficie pisoteada leyendo `FootingFacts` (altura de colisionadores grabada en simulación) o la API planeada `Terrain::height_and_normal_at` en $O(1)$; las manos leen sockets de armas (`SKT_MainHand`, `SKT_OffHand`), empuje (`SKT_Push_L/R`), carga sobre la cabeza (`SKT_Carry_Overhead`) o eventos de posición de loot en coordenadas locales (`LootGestureEvent { local_target_pos }`).
4. **Mezcla en capas (Layering & Masking):** El tronco inferior (locomoción) y el tronco superior (acciones/combate/carga/empuje) corren en nodos independientes del `AnimationGraph`, permitiendo atacar, cargar vasijas o recoger loot mientras se camina.

---

## Fase 1 — Mapeo de Rigs y Contrato de Animación (Retargeting)

### Problemática
Assets de distintas fuentes (maniquíes UAL1/UAL2, personajes creados en Blender, mallas de terceros) tienen nombres de huesos variables (`mixamorig:Hips`, `rig_hips`, `Hips`), distintas orientaciones de descanso (T-Pose vs A-Pose) o escalas locales.

### Solución ArquITECTÓNICA

#### 1. Nomenclatura Canónica de Huesos (Contrato Blender → Bevy)
Todos los personajes autorados o importados deben mapear sus nodos a la convención estándar definida en `ASSET_PIPELINE.md`:

| Nombre Canónico de Hueso | Propósito | Puntos de Anclaje / IK |
|---|---|---|
| `Hips` | Raíz del esqueleto (Pelvis) | Ajuste vertical de cadera por IK |
| `Spine` / `Chest` | Columna y Pecho | Separación de capas Tronco Sup/Inf |
| `Neck` / `Head` | Cuello y Cabeza | Look-At / Rotación a cámara |
| `UpperArm.L` / `UpperArm.R` | Hombros | Origen IK Manos |
| `LowerArm.L` / `LowerArm.R` | Codos | Joint Hinge IK Manos |
| `Hand.L` / `Hand.R` | Muñecas | Target IK Manos / Sockets |
| `Thigh.L` / `Thigh.R` | Muslos | Origen IK Pies |
| `Calf.L` / `Calf.R` | Rodillas | Joint Hinge IK Pies |
| `Foot.L` / `Foot.R` | Tobillos | Target IK Pies / Adaptación a Pendiente |
| `Toe.L` / `Toe.R` | Puntas de Pie | Contacto con suelo |

#### 2. Mapeador Autonómico de Huesos (`BoneMappingTable`)
Para rigs externos (vendor), un componente `RigBoneMap` asocia cada `Entity` de hueso en Bevy con su rol canónico e incluye la caché del offset vertical de la planta en la rest-pose (`ankle_rest_y_offset`):

```rust
#[derive(Component)]
pub struct RigBoneMap {
    pub hips: Entity,
    pub thigh_left: Entity,
    pub calf_left: Entity,
    pub foot_left: Entity,
    pub thigh_right: Entity,
    pub calf_right: Entity,
    pub foot_right: Entity,
    pub upper_arm_left: Entity,
    pub lower_arm_left: Entity,
    pub hand_left: Entity,
    pub upper_arm_right: Entity,
    pub lower_arm_right: Entity,
    pub hand_right: Entity,
    pub ankle_rest_y_offset: f32,
}
```

#### 3. Guardrail Build-Time (`build.rs`)
Para assets authored con `bof_animset = "player"`, `build.rs` verifica la presencia de los nodos de jerarquía canónica y de los clips requeridos por `PLAYER_CLIP_CONTRACT`.

---

## Fase 2 — Mezcla de Animaciones en `PostUpdate` (Animation Blending & Graph Layering)

### Problemática
Combinar locomoción (caminar/correr/agacharse/nadar) con acciones de combate, carga o empuje sin interrumpir el movimiento ni producir cortes bruscos ("snapping"), evitando que `bevy_animation` borre los deltas de IK.

### Solución en Bevy 0.19 (`AnimationGraph`)

#### 1. Estructura del Grafo de Animación

```text
               AnimationGraph Root
                        │
         ┌──────────────┴──────────────┐
         ▼                             ▼
   Capa Inferior                 Capa Superior
 (Lower-Body Layer)            (Upper-Body Layer)
  Mask: Hips, Legs              Mask: Spine, Arms
  - Idle                        - Neutral (pasa Capa Inf)
  - Walk / Run / Sneak          - Sword Light Attack 1..3
  - Swim / Dive                 - Bow Aim / Draw / Hold
  - Push / Carry Walk           - Carry Upper / Push Upper
  - Directional Strafes         - Loot Quick Gesture (Additive)
         │                             │
         └──────────────┬──────────────┘
                        ▼
                Blend Node (Layer Add/Override)
                        │
                        ▼
                  AnimationPlayer
```

#### 2. Definición del Grafo con Máscaras de Huesos
En `src/visuals/animation.rs`:

```rust
pub struct CharacterAnimationGraph {
    pub graph_handle: Handle<AnimationGraph>,
    pub lower_body_layer: AnimationNodeIndex,
    pub upper_body_layer: AnimationNodeIndex,
    pub blend_node: AnimationNodeIndex,
}
```

- **Transiciones Suaves (`AnimationTransitions`):**
  - Cambios de locomoción (Walk $\leftrightarrow$ Run): Crossfade de **0.20 s**.
  - Escalado de velocidad dinámico protegido ($V_{autorada} \ge 0.05$) aplicado individualmente a cada nodo activo durante el crossfade.
  - Ataques e interacciones de loot: Crossfade de **0.10 s**.
  - Transición a parada (Stop): Crossfade de **0.15 s** con desaceleración.

---

## Fase 3 — Cinemática Inversa (IK) para Pies en `PostUpdate`

### Problemática
En terrenos irregulares, pendientes o plataformas elevadas (cajas/escaleras), las cápsulas de colisión flotan o meten los pies bajo la tierra si no hay adaptación visual.

### Arquitectura Técnica del Solver de Pies (Detección Híbrida de Piso)

```text
¿Entidad visible y distancia < 30m?
  ├── FALSO ──► Salta ejecución de IK (IK LOD Gate)
  └── VERDAD ─► ¿Has<Terrain> en la entidad pisoteada?
                  ├── FALSO ──► Leer altura del colisionador de FootingFacts (Simulación)
                  └── VERDAD ─► Muestreo Terrain::height_and_normal_at(xz) en O(1)  [PLANEADO]
               │
               ▼
Calcular Altura Real de Contacto (Y_target_L, Y_target_R) utilizando ankle_rest_y_offset
               │
               ▼
Ajuste Vertical de Cadera Protegido (Hips Lowering acotado a -0.35m; excede precipicio)
               │
               ▼
Closed-Form 2-Bone IK por Pierna (Thigh ── Calf ── Foot)
               │
               ▼
Rotación de Tobillo a Normal N_ground (Foot Rotation Alignment)
```

#### 1. Detección Híbrida de Suelo por Extremidad
En cada frame de `PostUpdate` (tras `bevy::animation::animate_targets`):
1. **IK LOD Gate:** Si la entidad visual no es visible o supera los 30m de distancia, el sistema aborta de inmediato.
2. **Lectura Pura de Datos de Piso:** Se verifica mediante `Has<Terrain>` si la entidad que pisa el personaje (`GroundFacts::entity`) es el terreno. Si no es terreno (cajas, escaleras), el solver lee el componente `FootingFacts` grabado por la física en `FixedUpdate`. De lo contrario, se consulta `Terrain::height_and_normal_at(xz)` en $O(1)$ (**planeada; ver la nota del encabezado**).
3. Se obtiene la altura de impacto $Y_{ground}$ y la normal $N_{ground}$.
4. Se calcula la distancia deseada del tobillo al terreno $P_{target}$ sumando el `ankle_rest_y_offset` medido de la rest-pose.

#### 2. Ajuste de Altura de Cadera Protegido contra Precipicios (`Hips Lowering`)
Para evitar que la cadera colapse cuando un pie cuelga sobre un precipicio o risco:
- El descenso de cadera se limita estrictamente:
  $$\Delta Y_{hips} = \text{clamp}(\min(\Delta_L, \Delta_R), \; -0.35\text{ m}, \; 0.0\text{ m})$$
- Si la distancia del terreno bajo un pie supera el alcance máximo de la pierna ($L_1 + L_2$), ese pie se marca como *En Abismo* y se ignora su delta en el cálculo de cadera (la pierna simplemente cuelga extendida verticalmente).

#### 3. Solver Analítico 2-Bone IK (Ley de los Cosenos)
Dado el origen $A$ (Muslo/Hip), el objetivo $C$ (Pie/Tobillo objetivo) y las longitudes de los segmentos $L_1$ (Muslo) y $L_2$ (Pantorrilla):

1. **Distancia efectiva:**
   $$D = \text{clamp}(\|C - A\|, \; |L_1 - L_2| + \epsilon, \; L_1 + L_2 - \epsilon)$$
2. **Ángulo de la Rodilla ($\theta_{rodilla}$):**
   $$\cos(\theta_{rodilla}) = \frac{L_1^2 + L_2^2 - D^2}{2 L_1 L_2}$$
3. **Ángulo del Muslo ($\theta_{muslo}$):**
   $$\cos(\alpha) = \frac{L_1^2 + D^2 - L_2^2}{2 L_1 D}$$
4. **Orientación de la Rodilla (Vector Polo):**
   El eje de flexión se orienta siguiendo el vector hacia adelante del personaje (Forward Vector), evitando que las rodillas roten hacia afuera en ángulo antinatural.

#### 4. Alineación del Pie a la Pendiente
El hueso `Foot` ajusta su rotación local para que la planta del pie se alinee con la normal $N_{ground}$:
$$R_{foot} = \text{Quaternion::from_rotation_arc}(\vec{u}_{up}, N_{ground})$$
Limitando el ángulo máximo de inclinación del tobillo a $\le 35^\circ$ para evitar deformaciones raras de malla.

---

## Fase 4 — Cinemática Inversa (IK) para Manos en `PostUpdate`

### Problemática
1. **Escalada y Apoyo (`Climb` / `Mantle`):** Las manos deben posarse exactamente sobre los agarres o salientes de la roca/pared.
2. **Agarre de Armas de Dos Manos (Arco / Mandoble):** La mano secundaria (`Hand.L` o `Hand.R`) debe buscar el socket exacto de la empuñadura del arma (`SKT_OffHand`).
3. **Empuje y Carga (`PushPull` / `Carry`):** Ajustar las palmas a cajas pesadas o vasijas/barriles sostenidos sobre la cabeza (`SKT_Carry_Overhead`).
4. **Looting en Movimiento (Planeado Fase 3):** Extensión rápida de mano hacia la coordenada espacial local `local_target_pos` emitida por `LootGestureEvent` (evento planeado) sin depender de una entidad viva ni estirar el brazo hacia atrás.

### Arquitectura Técnica del Solver de Manos

#### 1. Modo Armas (Socket-Targeted IK)
- La mano principal toma el arma en `SKT_MainHand`.
- El arma expone un socket para la segunda mano (`SKT_OffHand`).
- El solver 2-Bone IK del brazo secundario (`UpperArm` $\to$ `LowerArm` $\to$ `Hand`) calcula la flexión del codo en `PostUpdate` para posicionar la palma sobre `SKT_OffHand`. Si el arma no expone el socket, degrada suavemente al transform de hueso de la animación.

#### 2. Modo Empuje / Carga (Push & Carry IK)
- En objetos `Pushable` (cajas, rocas), ambas manos ejecutan 2-Bone IK acoplándose a los sockets de empuje del objeto (`SKT_Push_L`, `SKT_Push_R`).
- En vasijas o barriles sobre la cabeza (`SKT_Carry_Overhead`), las manos se acoplan a la base/asas del objeto cargado.

#### 3. Modo Escalada (Wall Contact IK)
- Durante los estados `LocomotionState::Climb` o `Mantle`, se detecta la superficie de la pared.
- Si la pared está dentro del alcance del brazo ($L_1 + L_2$), la mano objetivo se fija sobre el punto de colisión de la pared.
- El codo se dobla hacia afuera/abajo mediante un vector polo de codo ajustado.

#### 4. Gesto de Looting en Movimiento (LootGestureEvent)
- Al recibir un `LootGestureEvent { local_target_pos }` (emitido por `InteractionPlugin` en coordenadas locales del Actor antes de despawnear el item), la mano libre ejecuta una curva suave IK en `PostUpdate` hacia `local_target_pos` sin detener la caminata de las piernas ni estirarse hacia atrás.

---

## Plan Paso a Paso de Implementación

### Paso 1: Mapeo y Estandarización de Jerarquía de Huesos
- **Tarea:** Crear el módulo `src/visuals/ik.rs` y definir las estructuras de componentes `FootIkTargets`, `HandIkTargets`, `LegIkChain` y `ArmIkChain`.
- **Entregables:**
  - Estructuras puros en `src/visuals/ik.rs`.
  - Mapeador que detecta la jerarquía de huesos al instanciar el esqueleto en `Update` y calcula `ankle_rest_y_offset`.
- **Validación:** Test unitario en `visuals/ik.rs` que verifica la extracción correcta de las cadenas de 2 huesos.

### Paso 2: Grafo de Animación con Capas (Upper/Lower Split)
- **Tarea:** Actualizar `src/visuals/animation.rs` para compilar un `AnimationGraph` con 2 capas separadas por máscara de huesos (tronco inferior para locomoción, tronco superior para ataques/arco/carga/empuje).
- **Entregables:**
  - Capas en `AnimationGraph`.
  - Transiciones independientes para tronco superior e inferior.
- **Validación:** Jugado en la caja `Combate`: poder correr mientras se ejecuta la animación de ataque de espada sin que las piernas se congelen.

### Paso 3: Solver Analítico 2-Bone IK Genérico
- **Tarea:** Implementar en `src/visuals/ik_solver.rs` la función matemática pura:
  `solve_two_bone_ik(root_pos, target_pos, pole_dir, l1, l2) -> (Quat, Quat)`
- **Entregables:**
  - Función matemática pura sin dependencias de ECS Bevy (solo math/glam/bevy_math).
- **Validación:** Tests unitarios matemáticos verificando triangulación exacta con cosenos en casos límite (cadena estirada, doblada a $90^\circ$, colapsada).

### Paso 4: Exposición de Normales en Terreno e IK Híbrido de Pies en `PostUpdate`
- **Tarea:** Extender `src/world/terrain.rs` con `Terrain::height_and_normal_at(xz) -> (f32, Vec3)` e implementar la lectura de `FootingFacts` y el clamping de `Hips Lowering` a $-0.35\text{ m}$ en `src/visuals/ik.rs`.
- **Entregables:**
  - Método `height_and_normal_at` en `world/terrain.rs`.
  - Detección `Has<Terrain>` vs datos de piso `FootingFacts`.
  - Sistemas de IK programados en **`PostUpdate`** `.after(bevy::animation::animate_targets)` con IK LOD gate (descarte a $>30\text{m}$).
  - Flexión de rodillas y rotación de tobillos según la normal $N_{ground}$ con suavizado `smooth_nudge`.
- **Validación:** Jugado en la caja `Terreno` y `Traversal`: los pies se apoyan correctamente en el suelo orgánico y sobre cajas/escaleras sin hundirse ni colapsar en bordes de precipicios.

### Paso 5: Sistema de IK de Manos en `PostUpdate` (Sockets, Empuje, Carga y Loot)
- **Tarea:** Implementar `solve_hand_ik` en `PostUpdate` para acoplar las manos a `SKT_OffHand`, `SKT_Push_L/R`, `SKT_Carry_Overhead` y gestos de loot.
- **Entregables:**
  - Seguimiento continuo de manos a sockets de objetos durante empuje, carga y tiro.
  - Recepción de `LootGestureEvent { local_target_pos }` para extensión de mano en movimiento al recoger materiales/manzanas del suelo.
- **Validación:** Jugado al empujar una caja o cargar una vasija: las manos se posicionan sobre las asas y la parte inferior camina/trota correctamente.

### Fase 6 — Profiling y Verificación de Invariantes
- **Tarea:** Integrar el medidor F1 para verificar que el impacto de CPU/GPU de IK sea $\le 0.15\text{ ms}$.
- **Entregables:**
  - Mediciones de rendimiento en el hub F1.
  - Tests de integración que comprueban que el IK se deshabilita automáticamente si el personaje está volando, planeando, nadando o cayendo.
- **Validación:** `cargo fmt` + `cargo clippy --all-targets -- -D warnings` + `cargo test` en verde.

---

## Verificación de Rendimiento y Contrato

- **Presupuesto CPU:** $\le 0.15\text{ ms}$ por personaje para la resolución de IK de 4 extremidades.
- **Presupuesto de Memoria:** 0 asignaciones dinámicas por frame (`Vec` pre-dimensionados en componentes `Resource` / `Component`).
- **Comandos de Verificación:**
```bash
cargo fmt --package breath-of-freedom
cargo clippy --all-targets -- -D warnings
cargo test
```
