# Audio y SFX — dirección técnica y eventos

Especificación del sistema objetivo de efectos de sonido (SFX), cues discretos,
modulación de audio continuo, combate, entorno y UI.

> **Cómo se usa este documento.** Es la referencia técnica para la emisión de cues de audio,
> mapeo de superficies a sonido de pisadas, combate, entorno y modulación de audio continuo.
> Describe lo que se quiere construir y sus criterios; código y `AHORA.md`
> indican el estado vivo. Todo el sistema cumple con el desacoplamiento de
> simulación (§20).

---

## Lo que se escucha y qué lo produce

| Lo que se escucha | Técnica que lo produce | Por qué |
|---|---|---|
| Sonido de pisadas según el terreno | `CueMessage { id: CueId::Step, kind: CueKind::Audio }` que lee `GroundFacts::surface` | La superficie (`Grass`, `Stone`, `Wood`, `Dirt`, `Sand`) cambia el sonido sin tocar la simulación |
| Pasos a ritmo de zancada | `StrideAccumulator` emite `CueId::Step` cada `STRIDE_LEN = 2.0m` de avance terrestre | Sostiene el ritmo hasta tener eventos de pisada (*foot-plant*) en las animaciones |
| Sonido de barrido al golpear con arma | `CueMessage { id: CueId::Swing, kind: CueKind::Audio }` emitido al iniciar el ataque | Feedback sonoro inmediato en ataques |
| Sonido de impacto al acertar un golpe | Lectura de `HitImpactMessage` en `Update` seleccionando sonido de impacto | Feedback de impacto auditivo diferenciado por blanco |
| Sonido de disparo y tensor de arco | Lectura de `BowFiredMessage` emitiendo zumbido de soltar cuerda | Sensación de tensión y disparo en combate a distancia |
| Despliegue de paravela / Aterrizaje | `CueMessage` en transiciones de `LocomotionState` (`Jump`, `Land`, `GlideOpen`) | Sensación de física de movimiento en traversal |
| Crepitación de fogata o antorcha | Emisor de audio espacializado `SpatialAudioSink` acoplado al objeto | Ambiente vivo en campamentos y construcciones |
| Modulación de respiración/esfuerzo | `ContinuousSfxTracker` monitorea deltas de velocidad ($\ge 0.5\text{ m/s}$) y stamina ($\ge 1.0$) | Permite ajustar el volumen/pitch del sonido continuo solo cuando el cambio es audible |

---

## Las cinco leyes de este sistema

### 1. El receptor posee la tabla de sonidos (§20)

La simulación emite un `CueMessage` abstracto o un mensaje de gameplay (`HitImpactMessage`, `BowFiredMessage`). El sistema de audio lee los datos y selecciona el sonido correspondiente. La simulación **nunca conoce rutas de archivos `.ogg` ni nombres de sonido**.

### 2. Modulación por umbrales audibles

La stamina y la velocidad cambian en cada tick de `FixedUpdate` (60 Hz). El tracker de audio continuo solo emite actualizaciones si el cambio supera `SPEED_DELTA_THRESHOLD = 0.5` o `STAMINA_DELTA_THRESHOLD = 1.0`, evitando saturar el bus de audio.

### 3. Silencio en reposo

Si el personaje no está tocando suelo (`grounded = false`) o su velocidad planar es $\le \text{MIN\_STEP\_SPEED } (0.6\text{ m/s})$, la distancia recorrida en `StrideAccumulator` se resetea a 0.0, evitando la acumulación de pasos al deslizarse o tropezar.

### 4. Atenuación espacial 3D acotada

Todos los efectos del mundo 3D usan `PlaybackSettings::with_spatial(true)`;
Bevy/rodio aplica paneo y caída cuadrática inversa, saturada dentro de ~1 m. Su
`SpatialAudioSink` no ofrece `max_distance`: `sfx` debe aplicar la cota de 25 m
como dueño, silenciando/no creando emisores fuera de rango. El límite no se
finge como un campo del motor.

### 5. Tolerancia a despawn en el mismo tick (`try_insert`)

Los componentes de seguimiento (`StrideAccumulator`, `ContinuousSfxTracker`) usan `try_insert` porque el `Actor` origen puede ser despawneado en el mismo tick por eventos de muerte o cambio de escena.

---

## Catálogo de Efectos de Sonido (SFX)

### 1. Cues Discretos de Movimiento

- **`CueId::Step`:** Emite sonido de pisada según
  `GroundFacts::surface` (`Grass`, `Stone`, `Wood`, `Dirt`, `Sand`).
- **`CueId::Jump`:** Sonido de impulso/despegue al saltar.
- **`CueId::Land` / `CueId::GlideOpen`:** Aterrizaje pesado y despliegue de
  paravela.

### 2. Cues de Combate
- **`CueId::Swing`:** Barrido de espada/garrote en el aire.
- **`CueId::Hit`:** Impacto contra enemigo (leído desde `HitImpactMessage`). Sonido de carne/madera/metal.
- **`CueId::BowDraw` / `CueId::BowRelease`:** Tensión de cuerda y disparo de flecha (leído desde `BowFiredMessage`).
- **`CueId::ArrowImpact`:** Impacto de flecha en blanco o madera/roca.

### 3. Audio de Entorno y Objetos
- **Fogata / Antorcha:** Crepitación continua en `FireEmitter` con `SpatialAudioSink` (alcance 8m).
- **Viento:** Audio continuo de fondo modulado según la altura de la cámara.
- **Agua:** Sonido de flujo en volúmenes `WaterVolume`.

### 4. Audio de UI e Inventario
- **UI:** Clic de menú, selección de objeto en HUD.
- **Pickup:** Sonido al recoger un material/manzana (`PickupMessage`).
- **Consumo:** Sonido al comer o curar salud.

---

## Fuentes y licencia de SFX

Fiel a `NORTE.md`, el sistema no depende de un catálogo externo:

- **Default:** grabación, foley o síntesis propia, con licencia SPDX y
  procedencia declaradas.
- **Fallback opcional:** archivos explícitamente CC0/dominio público de
  OpenGameArt o Freesound, verificados uno por uno. Que un sitio o bundle sea
  gratuito no lo vuelve CC0. Los bundles GDC de Sonniss quedan fuera: usan una
  EULA royalty-free propia.
- **Reproducibilidad:** ninguna fuente necesaria exige una cuenta, pago o
  descarga no automatizable.
- **Formatos:** Archivos `.ogg` para efectos de sonido comprimidos (sample rate 44.1 kHz, mono para espacializados 3D, estéreo para UI/Música).

---

## Orden de implementación

### Fase 1 — Infraestructura y Cues de Pisadas

- Mensajes `CueMessage`, `CueId`, `CueKind`.
- Emisión de pasos por `StrideAccumulator` cada 2.0 m.
- Lectura de `GroundFacts::surface` y seguimiento por umbrales de
  velocidad/stamina.

### Fase 2 — Carga de Clips .ogg y Audio Espacial 3D

- Cargar tabla de audio en `SfxPlugin` mapeando `SurfaceKind` a
  `Handle<AudioSource>`.
- Reproducir sonidos 3D usando `AudioPlayer` y `SpatialAudioSink`.
- Aplicar la cota de 25 m antes de crear/reproducir emisores.

### Fase 3 — SFX de Combate, Entorno y UI

- Enchufar `HitImpactMessage` y `BowFiredMessage` al reproductor de SFX.
- Sonido de fogatas (`FireEmitter`) y ruidos de UI al recoger objetos.

---

## Fuera de Alcance

Sistemas de oclusión de audio 3D por raycast geométrico continuo (reverberación de cueva diferida), síntesis procedural de audio en tiempo real.
