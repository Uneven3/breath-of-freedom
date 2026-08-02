# Plan Maestro de Movimiento, Motores, Animación estilo BOTW e IK

Documento de diseño y plan de implementación táctico para lograr la paridad completa de movimiento, física, mezcla de animaciones y Cinemática Inversa (IK) de **Breath of the Wild** en **Bevy 0.19** con **Rust** y **Avian3D**.

---

## 1. Arquitectura del Sistema de Motores de Locomoción

### Leyes de Arquitectura Aplicadas (§1, §6, §7, §14, §19, §20, §21)

1. **Multi-actor por `Actor` + `Intents` (§19):** Todo personaje (Player, NPCs, enemigos, caballo) es un `Actor`. La IA, el hardware de input o la red escriben **únicamente** sobre componentes `Intents` (`PlanarIntent`, `JumpIntent`, `ClimbIntent`). Jamás modifican directamente `Transform`, `BodyVelocity` o `LocomotionState`.
2. **Ortogonalidad Estricta de Estados (§1, §6, §19):** 
   - **`LocomotionState` (SSoT de Piernas):** Define la locomoción física (`Walk`, `Sprint`, `Jump`, `Fall`, `Swim`, `Dive`, `Climb`, `Glide`, `Stairs`, `Mount`).
   - **`ObjectManipulationState` (SSoT de Manos/Tronco):** Cargar una vasija (`Carry`) o empujar una caja (`PushPull`) son estados ortogonales a la locomoción de las piernas. Si el jugador carga una vasija y camina por un precipicio, `LocomotionState` pasa a `Fall` sin destruir el estado de carga de la vasija ni soltarla en el aire.
3. **Árbitro Central y Arbitraje de Manipulación (§7):**
   - La locomoción de las piernas se arbitra mediante `ProposalBuffer` (`arbitrate_locomotion`).
   - Los cambios de `ObjectManipulationState` son procesados por un árbitro autoritativo (`arbitrate_manipulation`) en `FixedUpdate`, asegurando un único escritor por tick.
4. **Desacoplamiento Estricto de Simulación y Presentación (§20):**
   - **Simulación (`FixedUpdate`):** La entidad de colisión física del objeto cargado (`RigidBody::Kinematic`) actualiza su posición puramente mediante física matemática respecto a la cápsula del `Actor` (`actor_pos + Vec3::Y * 1.8m`). **Jamás depende de un hueso visual de `Update`**, garantizando 60 Hz de determinismo aunque la animación o la cámara estén culled/pausadas.
   - **Presentación (`PostUpdate`):** La malla visual renderizable del objeto sigue la posición del nodo `SKT_Carry_Overhead` únicamente en la fase visual.
   - **Lanzamiento (`FixedUpdate`):** Al lanzar el objeto, el cuerpo cinemático pasa a `RigidBody::Dynamic` y recibe el impulso vectorial de física partiendo de su posición matemática de simulación.
5. **Orden de Ejecución de Sistemas (Schedule Ordering, §20):**
   - Los solvers de IK y la mezcla de animaciones se ejecutan en **`PostUpdate`**, encadenados `.after(bevy::animation::animate_targets)` y `.before(TransformSystem::TransformPropagate)`.
6. **Restricciones por Mensajes (`LocomotionConstraintMessage`):** La carga de objetos emite restricciones de velocidad (`SpeedLimit(1.2)`, `ForbidSprint`) al motor de locomoción activo.

### Subsistema de Agua (`WaterVolume` / `WaterFacts`)
Para soportar los motores `Swim` y `Dive` sin falsos positivos en puentes o volando sobre abismos:
- **Datos (`world/water.rs`):** Entidades delimitadas con componente `WaterVolume` (volúmenes de colisión/sensores AABB en Bevy/Avian3D).
- **Percepción (`movement/mod.rs`):** Durante `MovementSet::SenseWorld`, un sistema evalúa si la cápsula del `Actor` intersecta un `WaterVolume`:
  - `water_plane_y`: Altura de la superficie del volumen de agua intersectado.
  - `immersion_depth = (water_plane_y - (transform.translation.y - capsule_half_height)).max(0.0)`.
  - `can_swim`: Verifico intersección real con `WaterVolume` AND `immersion_depth >= 1.0m` (agua al pecho). Evita activar `Swim` en charcos o cuando el personaje vuela o camina por un puente.
- **Consumo:** Los motores `Swim` y `Dive` leen `WaterFacts` para proponer transiciones a `ProposalBuffer`.

### Pipeline de Ejecución en `FixedUpdate` y `PostUpdate`

```text
PreUpdate    Input Hardware ──► ActiveActions + ControlOrientation
             
FixedUpdate  Intents ──► [ SenseWorld (GroundFacts, WaterFacts) ──► GatherProposals ──► Arbitrate ──► TickActiveMotor ] ──► Body Velocity
                         - Walk / Sprint Motor                                   (ProposalBuffer)   (Único escritor       - Apply Velocity
                         - Swim / Dive Motor                                                        de LocomotionState)   - Math Carry Kinematic
                         - Climb Motor                                                                                    - Sync Colliders
             
Update       [ Presentación ] ──► Read LocomotionState/Velocity ──► UI / HUD / Camera Nudge

PostUpdate   [ Animación e IK ] ──► bevy::animation::animate_targets ──► Foot/Hand IK Solver ──► Visual Mesh Attach ──► TransformPropagate
```

---

## 2. Catálogo de Motores de Locomoción

### A. Motores Implementados y Validados (14)

| Motor | Archivo (`src/movement/motors/`) | `LocomotionState` | Propósito |
|---|---|---|---|
| **Walk** | `walk.rs` | `Walk` | Caminata plana guiada por aceleración e inercia. |
| **Sprint** | `sprint.rs` | `Sprint` | Carrera rápida con consumo de Stamina. |
| **Sneak** | `sneak.rs` | `Sneak` | Sigilo/agachado con cápsula reducida y bonificador de daño. |
| **Jump** | `jump.rs` | `Jump` | Impulso vertical con tolerancia *coyote time* y buffer de entrada. |
| **Fall** | `fall.rs` | `Fall` | Caída libre bajo gravedad con cálculo de daño por impacto. |
| **Glide** | `glide.rs` | `Glide` | Planeo en paravela con velocidad terminal acotada y consumo de stamina. |
| **Climb** | `climbing.rs` / `climb.rs` | `Climb` | Adherencia a paredes verticales con consumo de stamina por desplazamiento/salto. |
| **Ladder** | `ladder.rs` | `Ladder` | Escalada fija en escaleras de mano. |
| **Mantle** | `mantle.rs` | `Mantle` | Subida de borde/cornisa impulsando el cuerpo sobre la superficie. |
| **AutoVault** | `auto_vault.rs` | `AutoVault` | Salto automático de muros bajos ($\le 1.2\text{ m}$) sin perder inercia. |
| **WallJump** | `wall_jump.rs` | `WallJump` | Rebote en pared vertical. |
| **EdgeLeap** | `edge_leap.rs` | `EdgeLeap` | Impulso desde el borde de un precipicio. |
| **Stairs** | `stairs.rs` | `Stairs` | Adaptación de pasos para huella/contrahuella sin tropezar. |
| **Mounts** | `mounts/mod.rs` | `Mount` | Montado de caballo mediante enlace transaccional `ActorLink`. |

### B. Motores y Estados Ortogonales para el Feeling Objetivo

#### 1. Motor `Swim` (Nado en Superficie)
- **Activación:** `WaterFacts::can_swim == true` (intersección con `WaterVolume` e inmersión hasta el pecho).
- **Física:** Cancela la gravedad, aplica flotabilidad (*buoyancy*) manteniéndose en la superficie del agua y consume stamina al avanzar o realizar brazadas rápidas (*Swim Dash*).
- **Animación:** Asocia `AnimationRole::Swim` (`AN_Swim`). Fallback en `ROLE_TABLE`: `Walk` $\to$ `Idle`.

#### 2. Motor `Dive` (Buceo Submarino)
- **Activación:** Presionar sumergirse (`Crouch`/`Dive`) durante el nado profundo en `WaterVolume`.
- **Física:** Movimiento 3D completo (control de *pitch* y *yaw*). Activa el recurso de **Oxígeno** (separado de la Stamina). Al agotar el oxígeno o soltar la tecla, el cuerpo flota automáticamente hacia la superficie.
- **Animación:** Asocia `AnimationRole::Dive` (`AN_Dive`). Fallback en `ROLE_TABLE`: `Swim` $\to$ `Walk` $\to$ `Idle`.

#### 3. Estado Ortogonal `ObjectManipulationState::PushPull` (Empujar y Tirar Bloques)
- **Diseño Ortogonal:** No altera `LocomotionState` (las piernas siguen en `Walk`). Emite un `LocomotionConstraintMessage::SpeedLimit(0.8)` y fija la dirección de avance a 1D/2D.
- **Limpieza de Entidades:** Si la entidad empujada es despawneada o destruida, un sistema de limpieza en `FixedUpdate` resetea `ObjectManipulationState` a `None`.
- **Animación e IK:** Activa la capa superior `AN_Push` / `AN_Pull`. Las manos del personaje se acoplan mediante 2-Bone Hand IK a los sockets del objeto (`SKT_Push_L`, `SKT_Push_R`).

#### 4. Estado Ortogonal `ObjectManipulationState::Carry` (Cargar y Lanzar Vasijas/Barriles)
- **Matemática Físico-Cinemática (§20):** La colisión de la vasija (`RigidBody::Kinematic`) se actualiza matemáticamente en `FixedUpdate` respecto al `Actor`. La malla visual sigue `SKT_Carry_Overhead` en `PostUpdate`.
- **Restricción:** Emite `LocomotionConstraintMessage::SpeedLimit(1.2)` y `ForbidSprint`. Si el personaje cae por un borde, las piernas entran en `LocomotionState::Fall`, mientras el objeto se mantiene en `Carry`.
- **Lanzamiento:** Al presionar `Attack`/`Throw`, se convierte en `RigidBody::Dynamic` y se le aplica un impulso parabólico de física partiendo de la posición física de simulación.

#### 5. Looting Rápido en Movimiento (LootGestureEvent)
- **Coordenada Espacial Local:** Al recoger un objeto del suelo en `FixedUpdate`, `InteractionPlugin` emite `LootGestureEvent { local_target_pos: Vec3 }` calculando la posición en **coordenadas locales del Actor**.
- **Presentación:** La capa superior del `AnimationGraph` en `PostUpdate` desplaza la mano hacia `local_target_pos` relativo al personaje en movimiento, evitando que el brazo se estire hacia atrás al caminar.

#### 6. Motor `ShieldSurf` (Deslizamiento en Escudo)
- **Activación:** Saltar en el aire sobre una pendiente inclinada sosteniendo el escudo desenvainado.
- **Física:** Fricción reducida en nieve/hierba, desgaste de durabilidad del escudo en roca/camino plano, conservación de momentum.

#### 7. Motor `SlopeSlide` (Resbalón en Pendiente)
- **Activación:** Pararse en pendientes orgánicas que superan el ángulo crítico ($\ge 45^\circ$) sin stamina para escalar.

---

## 3. Animaciones sin Deslizamiento (Foot-Sliding Elimination & Safe Per-Node Speed Scaling)

### A. Escalado Dinámico de Velocidad Protegido contra División por Cero
Cada animación de locomoción tiene una velocidad de zancada autorada de referencia ($V_{autorada}$):
- `AN_Walk`: $1.5 \text{ m/s}$
- `AN_Run`: $4.0 \text{ m/s}$
- `AN_Sneak`: $1.0 \text{ m/s}$
- `AN_Push`: $0.8 \text{ m/s}$
- `AN_CarryWalk`: $1.2 \text{ m/s}$

En `Update`, el sistema lee la velocidad planar real del cuerpo ($V_{real} = \|\text{BodyVelocity.xz}\|$) y ajusta la velocidad de reproducción de forma independiente en cada nodo activo del `AnimationPlayer`:

$$k_{speed\_node} = \begin{cases} 
\frac{V_{real}}{V_{autorada\_node}} & \text{si } V_{autorada\_node} \ge 0.05 \text{ m/s} \\ 
1.0 & \text{si } V_{autorada\_node} < 0.05 \text{ m/s} \text{ (Idle, Poses estáticas)}
\end{cases}$$

Esta protección evita `NaN` o divisiones por cero en clips estáticos y garantiza que durante los crossfades cada clip ajuste su velocidad a su propia zancada autorada.

### B. Capas del Grafo de Animación (`AnimationGraph` Upper/Lower Split)

Ver `CHARACTER_ANIMATION_IK.md` Fase 2 para la estructura del grafo con máscaras
de huesos. Los clips de locomoción (`AN_Walk`, `AN_Run`, etc.) corren en la capa
inferior; los de combate/carga/empuje en la superior.

---

## 4. Arquitectura Modular del Mesh y Sockets

### Mallas Modulares (Skinned Meshes)
Para soportar cambios de equipamiento y armaduras, la malla visual del personaje se divide en submallas parentadas a la misma armadura (`PlayerVisual`):
- `SK_Body_LOD0`: Torso / Cuerpo base.
- `SK_Head_LOD0`: Cabeza y cabello.
- `SK_Legs_LOD0`: Pantalones y calzado.
- `SK_Armor_LOD0`: Armadura / Piezas de equipamiento.

### Sockets de Armas y Objetos (`SKT_*`)
Las armas y objetos cargados no forman parte del mesh del personaje. Son GLBs independientes instanciados en nodos vacíos (*Empty*) en la armadura:
- `SKT_MainHand`: Espada / Garrote en mano derecha.
- `SKT_OffHand`: Escudo / Agarre secundario del arco.
- `SKT_Bow`: Arco colgado en la espalda o listo para disparar.
- `SKT_Back_Weapon`: Espada enfundada en la espalda.
- `SKT_Carry_Overhead`: Punto de acople para vasijas, barriles y piedras sobre la cabeza.

---

## 5. Escala y Altura Ideal del Personaje (Blender $\to$ Bevy)

### Estándar de Altura (1 unidad = 1 metro)
- **Altura del Personaje:** **$1.85 \text{ m} - 2.00 \text{ m}$** ($1.83\text{ m}$ de cuerpo + calzado/cabello).
- **Ancho de Hombros:** $\sim 0.50 \text{ m}$.
- **Pivote / Origen en Blender:**
  - El pivote debe estar exactamente entre los pies en el suelo: $(X=0, Y=0, Z=0)$ en Blender ($Y$-Up en Bevy).
  - Rotación en Blender: Mirando hacia $-Y$ (al exportar a GLTF se convierte en $-Z$ en Bevy).

### Coincidencia con la Cápsula Física
- **Física (`FixedUpdate`):** Cápsula Avian3D de radio $R = 0.35\text{ m}$ y altura $H = 1.85\text{ m}$ (centro en $Y = 0.925\text{ m}$).
- **Presentación (`Update`):** La entidad `PlayerVisual` se alinea al pivote inferior de la cápsula física con escala $(1.0, 1.0, 1.0)$.

---

## 6. Scripts Python de Blender y División de Trabajo (Humano vs Agente IA)

### A. Scripts de Automatización Python en Blender (`tools/`)
Para garantizar la precisión de infraestructura sin tareas repetitivas manuales, se utilizarán scripts en Python (`bpy`):
1. **`tools/export_blender_asset.py` (Existente):** Exportador GLB reproducible con coordenadas $Y$-Up, aplicación de transformaciones y exportación de acciones `AN_*`.
2. **`tools/blender_setup_rig.py` (Planeado):**
   - Aplica `All Transforms` (`Ctrl+A`) y fija el pivote en $(0,0,0)$ entre los pies.
   - Genera programáticamente los nodos `SKT_*` (`SKT_MainHand`, `SKT_OffHand`, `SKT_Bow`, `SKT_Push_L/R`, `SKT_Carry_Overhead`).
   - Inyecta `bof_*` custom properties (`bof_license`, `bof_profile`, `bof_material_kind`, `bof_animset = "player"`).
   - Renombra acciones de proveedores a la convención `AN_<Rol>` y activa `use_fake_user = True`.
   - Genera mallas `LOD1` y `LOD2` mediante el modificador `Decimate`.

### B. Matriz de División de Trabajo (Humano vs Agente IA)

| Tarea | Humano (Artista) | Agente IA (Python / Rust) |
|---|---|---|
| **Modelado Low-Poly** | Escultura de mallas, topología limpia alrededor de articulaciones, estilo visual BotW | Monitoreo del presupuesto poligonal (`budget.rs`), rechazo en `build.rs` |
| **Weight Painting** | Retoque fino manual de pesos en codos, hombros y entrepierna en modo Weight Paint | Asignación automática inicial con `ARMATURE_AUTO`, validación de grupos de vértices |
| **Autoría de Animaciones** | Creación de poses clave, ritmo, peso y curvas Bezier en el Action Editor | Renombrado a `AN_<Rol>`, `use_fake_user`, verificación de loops sin root motion |
| **Sockets e Infraestructura** | Definición de puntos de agarre en el arte | Generación programática de Empties `SKT_*` y propiedades `bof_*` vía Python |
| **Generación de LODs y Export** | Inspección visual de siluetas | Generación automática `Decimate` (LOD1/2) y exportación GLB con `export_blender_asset.py` |

---

## 7. Cinemática Inversa (IK) Analítica para Pies y Manos

**Documento dueño: `CHARACTER_ANIMATION_IK.md`.**

El sistema de IK es ortogonal a los motores de locomoción: los motores escriben
`LocomotionState` y velocidad en `FixedUpdate`; el IK lee esos datos y corrige
huesos en `PostUpdate`. Ver `CHARACTER_ANIMATION_IK.md` para el solver 2-bone,
el pipeline de foot IK, el IK de manos y los pasos de implementación.

Lo que este documento aporta al IK:
- `FootingFacts` se graba en `MovementSet::SenseWorld` (altura del colisionador).
- `ObjectManipulationState` define los sockets activos (`SKT_Push_L/R`,
  `SKT_Carry_Overhead`) que el IK de manos consume.
- Los motores Swim/Dive/Glide desactivan el IK de pies.

---

## 8. Plan Paso a Paso de Implementación Organizado en Fases

### Fase 1 — Volúmenes de Agua (`WaterVolume`), Motores (`Swim`/`Dive`), Estado de Carga Ortogonal y Árbitros
- [ ] **Definir Volúmenes de Agua (`world/water.rs`):** Crear el componente delimitado `WaterVolume` (sensor AABB/Collider) y calcular `WaterFacts` en `MovementSet::SenseWorld` (`pool_depth >= 1.2m` AND `immersion_depth >= 1.0m`).
- [ ] **Actualizar Enums y Guards Exhaustivos (`src/movement/state.rs`):**
  - Agregar `Swim`, `Dive`, `ShieldSurf`, `SlopeSlide` a `LocomotionState`.
  - Crear `ObjectManipulationState { Carry(Entity), PushPull(Entity) }` y el árbitro `arbitrate_manipulation` en `FixedUpdate` (§7).
  - Actualizar `LocomotionState::ALL` y `assert_all_is_exhaustive`.
- [ ] **Implementar Motores `Swim` y `Dive` (`src/movement/motors/`):** Flotabilidad en `WaterVolume`, stamina/oxígeno y ascenso automático.
- [ ] **Implementar Carga y Empuje Desacoplados con Matemática Física (§20):** `RigidBody::Kinematic` posicionado matemáticamente en `FixedUpdate` y acople de malla visual a `SKT_Carry_Overhead` en `PostUpdate`.
- [ ] **Crear Resolvedor de Animación (`src/visuals/animation.rs`):**
  - Agregar `AnimationRole::{Swim, Dive, Push, Pull, CarryIdle, CarryWalk}`.
  - Configurar las cadenas de fallback en `ROLE_TABLE`.

### Fase 2 — Eliminación de Foot-Sliding y Grafo de Animación Protegido
- [ ] Implementar `node_playback_speed = V_real / V_autorada` protegido para
  `V_autorada < 0.05`, y mezcla continua `Walk`↔`Run` por velocidad real sin
  agregar un estado discreto de trote.
- [ ] Configurar el `AnimationGraph` con dos capas (Lower-Body para movimiento, Upper-Body para armas/ataques/objetos).
- [ ] Agregar sincronización de fase entre animaciones de caminata y trote (*Phase Matching* por eventos de pisada).

### Fase 3 — Estandarización de Rigs, Mallas Modulares, Sockets y Scripts Python
- [ ] Implementar el script `tools/blender_setup_rig.py` para automatizar la inyección de sockets `SKT_*`, custom properties `bof_*` y generación de LOD1/2.
- [ ] Implementar el componente `RigBoneMap` para el mapeo de nombres de huesos externos a la nomenclatura canónica.
- [ ] Configurar la instanciación de armas y objetos cargados en los sockets `SKT_MainHand`, `SKT_OffHand`, `SKT_Bow` y `SKT_Carry_Overhead`.
- [ ] Actualizar la comprobación de `build.rs` para validar los nodos `SK_*` y sockets `SKT_*` en personajes authored.

### Fase 4-5 — IK de Pies y Manos

Ver `CHARACTER_ANIMATION_IK.md` Pasos 3-5. Los motores de este documento aportan
`FootingFacts` y `ObjectManipulationState`; el solver y los sistemas de IK viven
en el módulo de IK.

### Fase 6 — Profiling y Verificación de Invariantes
- [ ] Medir en el hub F1 que el costo de animación e IK sea $\le 0.15\text{ ms}$ en CPU.
- [ ] Ejecutar la suite completa de pruebas:
```bash
cargo fmt --package breath-of-freedom
cargo clippy --all-targets -- -D warnings
cargo test
```
