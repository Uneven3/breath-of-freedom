# Partículas y VFX — dirección técnica y efectos

Especificación del sistema objetivo de partículas, efectos visuales (VFX) y
"juice" (feedback táctil/visual).

> **Cómo se usa este documento.** Referencia de diseño y parámetros para efectos
> visuales transitorios (chispas de hit, fuego, humo, briznas de pasto voladoras, arcos de barrido, hit flashes, squash & stretch).
> Código y `AHORA.md` indican el estado vivo. Cada efecto respeta las leyes de
> rendimiento y el desacoplamiento de simulación (§20).

---

## Lo que se ve y qué lo produce

| Lo que se ve | Técnica que lo produce | Por qué |
|---|---|---|
| Chispas blancas al golpear un blanco (Hit VFX) | Burst de 8 partículas esféricas en abanico dorado con amortiguación exponencial (`BURST_DRAG_PER_SEC = 6.0`) | Da impacto visual inmediato sin aleatoriedad (RNG) en el hot-path |
| Llamas de fuego / fogatas flameantes (Fuego VFX) | Quads/billboards con `T_FXFire_Albedo.png` (`unlit: true`), desplazamiento ascendente + escalado | Fuego de bajo costo en GPU que no requiere simulación volumétrica |
| Columnas de humo en fogatas o antorchas (Humo VFX) | Billboards de humo con rotación angular, expansión por tiempo y disipación de opacidad | Simula humo realista con 3-5 quad particles por emisor |
| Briznas de pasto volando al cortar con la espada (Pasto VFX) | Quads de 2 tris teñidos de verde raíz/punta disparados con impulso parabólico al cortar celdas `cuttable` | Presentación responde al evento de combate sin alterar la simulación física |
| El personaje o enemigo se ilumina en blanco al recibir daño | `HitFlash` en la malla visual (`HIT_FLASH_COLOR = srgb(2.5, 2.5, 2.5)`, 0.12 s) | Feedback inequívoco de que el golpe conectó |
| El cuerpo se estira al saltar y se aplasta al aterrizar | `Jelly` squash & stretch en `VisualOf` (+28% estiramiento en salto, -24% aplastamiento al aterrizar) | Sensación de peso y elasticidad con compensación aproximada de volumen |
| Arco de barrido al atacar con arma | `SwingVfx` usando `CircularSector` traslúcido (`unlit: true`, `AlphaMode::Blend`) | Indica el alcance y ángulo del ataque en presentación |
| Daño flotante en pantalla | `DamageText` anclado a mundo en pantalla UI (20 px normal, 30 px amarillo dorado en crítico) | Lectura clara del daño causado sin ensuciar la simulación |
| Sacudida de cámara y destello rojo al recibir daño | `ScreenFlash` (alpha 0.3) + `CameraShake` trauma (`0.55`) | Comunica el peligro físico al jugador |

---

## Las cinco leyes de este sistema

### 1. Presentación pura: los VFX jamás escriben en simulación (§20)

Las partículas, los efectos de fuego, humo, briznas de pasto cortado, el destello de golpe y el texto de daño leen eventos de simulación
(`HitImpactMessage`, `GrassCutMessage`, `BowFiredMessage`, `CueMessage`), pero **nunca modifican componentes de gameplay**.

### 2. Amortiguación exponencial, no lineal

Toda desaceleración de partículas usa decaimiento exponencial (`smooth_nudge` / `exp`), nunca `(1.0 - k * dt)`: esa forma lineal llega a freno total en 1 frame si `k * dt >= 1.0`, arruinando el suavizado a bajos FPS.

### 3. Presupuesto de partículas acotado

- **Hit burst:** Máximo 8 partículas por impacto.
- **Fire & Smoke:** Emisores fijos de máximo 4-6 quad particles activas simultáneamente.
- **Grass Cut Debris:** Máximo 6 briznas voladoras por corte de pasto.
- **Cota global:** `VfxBudget` admite como máximo **128 entidades VFX
  transitorias** simultáneas entre partículas, arcos y textos. Los emisores de
  geometría reutilizan un pool fijo; si no queda capacidad, se conserva el
  efecto más cercano entre el nuevo y los ya activos, descartando el más
  lejano. Modificadores sobre entidades persistentes (`HitFlash`, `Jelly`) no
  consumen una entrada adicional.
- **Transitorios:** Toda entidad de partícula tiene un timer de vida rígido (`BURST_SECS = 0.22s`, `FIRE_PARTICLE_SECS = 0.45s`, `GRASS_DEBRIS_SECS = 0.6s`, `SWING_VFX_SECS = 0.16s`, `DAMAGE_TEXT_SECS = 0.8s`).
- Despawn automático al expirar.

### 4. Tolerancia a carreras de despawn (`try_insert` / `try_remove`)

Cuando un golpe letal despawnea un `Actor` en `FixedUpdate`, la entidad visual puede desaparecer en el mismo tick. Los sistemas de VFX usan operaciones tolerantes para evitar panics.

### 5. Alfa excepcional, explícita y acotada

El baseline sigue siendo opaco. Fuego y humo son la excepción escrita:
`T_FXFire_Albedo`/`T_FXSmoke_Albedo` pueden llevar alfa porque son pocos,
transitorios y están bajo `VfxBudget`; usan blend, nunca `Mask`. Las briznas de
pasto volador usan geometría teñida con `ROOT_COLOR`/`TIP_COLOR`, sin textura ni
alfa. La excepción no habilita alfa en vegetación persistente.

---

## Especificación por Tipo de Partícula

### 1. Partículas de Impacto / Hit (`src/presentation/juice.rs`)
- **Conteo:** 8 partículas por golpe.
- **Comportamiento:** Disparadas en abanico dorado desde el punto de impacto.
- **Velocidad inicial:** 5.0 m/s con arrastre exponencial `BURST_DRAG_PER_SEC = 6.0`.
- **Duración:** 0.22 s con encogimiento progresivo de escala.

### 2. Partículas de Fuego y Fogatas (Fire VFX)
- **Activación:** Componente emisor `FireEmitter` en campamentos, antorchas o terreno inflamable encendido (`flammable`).
- **Textura & Material:** Quad billboard con la textura CC0 planeada
  `T_FXFire_Albedo.png` (Fase 2 import), `unlit: true`, `AlphaMode::Blend` y
  `base_color` con tinte anaranjado/amarillo cálido; excepción acotada por la
  Ley 5.
- **Comportamiento:** Nace en la base, asciende verticalmente con velocidad $1.2\text{ m/s}$, escala de $0.2\text{ m} \to 0.5\text{ m}$, y desvanece su opacidad en $0.45\text{ s}$.
- **Rendimiento:** 4 partículas por emisor, recicladas por timer.

### 3. Partículas de Humo (Smoke VFX)
- **Activación:** Componente emisor `SmokeEmitter` acoplado a fuego o fuentes de calor.
- **Comportamiento:** Quads que ascienden suavemente ($0.8\text{ m/s}$), rotan angularmente a $\pm 1.2\text{ rad/s}$ y expanden su escala ($0.3\text{ m} \to 0.9\text{ m}$) mientras se disipan.
- **Duración:** $1.2\text{ s}$ por partícula.

### 4. Partículas de Pasto Cortado / Debris (Grass Cut VFX)
- **Activación:** Al cortar pasto en una celda `cuttable` con barrido de espada en combate.
- **Geometría:** 4-6 quads de 2 triángulos teñidos con el gradiente verde del pasto (`ROOT_COLOR` $\to$ `TIP_COLOR`).
- **Comportamiento:** Impulso parabólico hacia arriba y afuera ($V_y = 2.5\text{ m/s}$, $V_{xz} = 1.8\text{ m/s}$) con gravedad ligera ($g = -9.81\text{ m/s}^2$) y rotación libre en 3D.
- **Duración:** $0.6\text{ s}$ hasta tocar el suelo y despawnear.

### 5. Hit Flash (`src/presentation/juice.rs`)
- **Duración:** 0.12 s.
- **Color:** Blanco brillante HDR `Color::srgb(2.5, 2.5, 2.5)`.

### 6. Jelly Squash & Stretch (`src/presentation/juice.rs`)
- **Salto:** `JELLY_JUMP_STRETCH = 0.28` (+28% altura Y, -16.8% XZ).
- **Aterrizaje:** `JELLY_LAND_SQUASH = -0.24` (-24% altura Y, +14.4% XZ).
- **Recuperación:** Amortiguada a `JELLY_RECOVERY_PER_SEC = 9.0`.
- **Volumen:** La fórmula objetivo compensa visualmente, pero no promete
  conservación exacta; no se documenta como invariante física.

### 7. Arco de Barrido (`src/visuals/vfx.rs`)
- **Duración:** 0.16 s.
- **Geometría:** `CircularSector` acorde al alcance (`step.reach`) y arco (`step.arc_deg`) del ataque (`unlit: true`, `AlphaMode::Blend`).

### 8. Texto de Daño Flotante (`src/presentation/juice.rs`)
- **Activación:** Al conectar un golpe (`HitImpactMessage`).
- **Anclaje:** Posición de mundo $P + (0, 1.2, 0)\text{ m}$ convertida a espacio de pantalla viewport (`world_to_viewport`).
- **Comportamiento:** Ascenso vertical ($1.1\text{ m/s}$), desvanecimiento progresivo de alfa en $0.8\text{ s}$.
- **Estilo:** 20px blanco en golpe normal; 30px amarillo dorado (`srgb(1.0, 0.85, 0.2)`) en golpe crítico.

### 9. Destello de Pantalla y Sacudida de Cámara (`src/presentation/juice.rs`)
- **Activación:** Cuando el jugador local es blanco de un golpe (`HitImpactMessage` en `Player`).
- **Screen Flash:** Superposición UI de pantalla completa (`GlobalZIndex(50)`) con destello blanco `alpha = 0.3` desvaneciendo a $2.2/\text{s}$.
- **Camera Shake:** Inyección de trauma de cámara `PLAYER_HIT_TRAUMA = 0.55` en `CameraShake` (o $0.08 \to 0.25$ según la carga del arco al disparar).

---

## Fuera de Alcance

GPU compute particle systems pesados, simuladores fluidos volumétricos, y mallas destructibles procedurales complejas.
