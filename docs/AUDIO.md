# Audio y SFX — dirección técnica y eventos

Especificación del sistema objetivo de efectos de sonido (SFX), cues discretos,
modulación de audio continuo, combate, entorno y UI. Revisado el
**2026-08-05**: las leyes eran correctas, pero el documento describía en presente
un catálogo de cues que en el código son dos, y no llevaba ninguna cota de costo.

> **Cómo se usa este documento.** Es la referencia técnica para la emisión de
> cues de audio, mapeo de superficies a sonido de pisadas, combate, entorno y
> modulación de audio continuo. Describe lo que se quiere construir y sus
> criterios; código y `AHORA.md` indican el estado vivo. Todo el sistema cumple
> con el desacoplamiento de simulación (§20).
>
> **Dónde está parado esto hoy.** No hay un solo archivo de audio en el
> repositorio. `play_audio_cues` **imprime el cue por log** — es lo que `NORTE.md`
> define como estado provisional, y lo que hace que todo lo de abajo sea plan y
> no descripción. Lo que sí corre: el acumulador de zancada, el tracker de
> modulación continua y dos `CueId`.

---

## Lo que se escucha y qué lo produce

| Lo que se escucha | Técnica que lo produce | Estado |
|---|---|---|
| Pasos a ritmo de zancada | `StrideAccumulator` emite `CueId::Step` cada `STRIDE_LEN = 2.0 m` de avance terrestre | **Hecho** (como log) |
| Modulación de respiración/esfuerzo | `ContinuousSfxTracker` observa deltas de velocidad (≥ 0.5 m/s) y stamina (≥ 1.0) | **Hecho** (como log) |
| Sonido de pisadas según el terreno | El consumidor lee `GroundFacts::surface` (`Grass`, `Stone`, `Wood`, `Dirt`, `Sand`) y elige el clip | Objetivo: falta la tabla y los clips |
| Sonido de barrido al golpear con arma | `CueId::Swing` emitido al iniciar el ataque | Objetivo: el `CueId` no existe |
| Sonido de impacto al acertar | Lectura de `HitImpactMessage` en `Update` | Objetivo |
| Disparo y tensión de arco | Lectura de `BowFiredMessage` | Objetivo |
| Despliegue de paravela / aterrizaje | Cues en transiciones de `LocomotionState` | Objetivo: `CueId::Jump` existe pero nadie lo emite |
| Crepitación de fogata o antorcha | Emisor espacializado acoplado al objeto | Objetivo; depende de `FireEmitter` (`PARTICLES.md`) |

---

## Las siete leyes de este sistema

### 1. El receptor posee la tabla de sonidos (§20)

La simulación emite un `CueMessage` abstracto o un mensaje de gameplay
(`HitImpactMessage`, `BowFiredMessage`). El sistema de audio lee los datos y
elige el sonido. La simulación **nunca conoce rutas `.ogg` ni nombres de
sonido**.

### 2. Modulación por umbrales audibles

Stamina y velocidad cambian en cada tick de `FixedUpdate` (60 Hz). El tracker
continuo sólo emite actualizaciones si el cambio supera
`SPEED_DELTA_THRESHOLD = 0.5` o `STAMINA_DELTA_THRESHOLD = 1.0`. Sin esto, el
bus recibe sesenta actualizaciones por segundo de algo que el oído no puede
distinguir.

### 3. Silencio en reposo

Si el actor no toca suelo (`grounded = false`) o su velocidad planar es
≤ `MIN_STEP_SPEED` (0.6 m/s), la distancia acumulada se resetea a 0. Evita
acumular pasos al deslizarse, tropezar o resbalar.

### 4. La cota espacial es nuestra, no del motor

Todos los efectos del mundo 3D usan `PlaybackSettings::with_spatial(true)`;
Bevy/rodio aplica paneo y caída cuadrática inversa, saturada dentro de ~1 m. Su
`SpatialAudioSink` **no ofrece `max_distance`**: `sfx` debe aplicar la cota de
25 m como dueño, no creando o silenciando emisores fuera de rango. El límite no
se finge como un campo del motor.

### 5. Tolerancia a despawn en el mismo tick (`try_insert`)

Los componentes de seguimiento (`StrideAccumulator`, `ContinuousSfxTracker`)
usan `try_insert` porque el `Actor` origen puede ser despawneado en el mismo
tick por muerte o cambio de escena.

### 6. Las voces tienen presupuesto

Bevy mezcla audio en **CPU** (rodio), no en un DSP dedicado: cada voz activa es
trabajo de CPU por frame, y espacializar cuesta más que no hacerlo. En un
teléfono esa CPU compite con la simulación y con el hilo de render.

**Cota:** **16 voces simultáneas** *(decisión, no medición)*, de las cuales como
mucho 12 espacializadas. Al llenarse se descarta la más lejana, igual que
`VfxBudget` en `PARTICLES.md`. Se valida cuando existan clips reales, midiendo
CPU con y sin audio en la misma caja.

### 7. Un `CueId` nuevo es un compromiso de autoría

Cada entrada del catálogo implica grabar o sintetizar un sonido, declarar su
licencia y mantenerlo. Un enum lleno de variantes que nadie emite y para las que
no hay archivo no es un plan: es una lista de deseos que hace parecer que el
sistema está más avanzado de lo que está.

**Regla:** un `CueId` entra cuando hay un emisor que lo dispara **y** un clip (o
un placeholder sintetizado propio) que lo suena. Hoy son dos: `Step` y `Jump`.

---

## Catálogo objetivo de SFX

Ninguno de estos existe todavía como `CueId`, salvo donde se indica.

### 1. Movimiento

- **`Step`** — pisada según `GroundFacts::surface`. *(Existe; falta el clip.)*
- **`Jump`** — impulso de despegue. *(El `CueId` existe; nadie lo emite.)*
- **`Land`**, **`GlideOpen`** — aterrizaje pesado y despliegue de paravela.

### 2. Combate

- **`Swing`** — barrido de arma en el aire.
- **`Hit`** — impacto contra enemigo, leído de `HitImpactMessage`; carne, madera
  o metal según el blanco.
- **`BowDraw`** / **`BowRelease`** — tensión de cuerda y disparo, desde
  `BowFiredMessage`.
- **`ArrowImpact`** — flecha contra blanco, madera o roca.

### 3. Entorno y objetos

- **Fogata / antorcha:** crepitación continua espacializada en `FireEmitter`,
  alcance 8 m (dentro de la cota de 25 m de la ley 4).
- **Viento:** capa continua modulada por altura de cámara.
- **Agua:** flujo en volúmenes `WaterVolume` — depende de que `WaterVolume`
  exista (`BOTWMovements.md`, Fase 1).

### 4. UI e inventario

- Clic de menú y selección en HUD.
- `Pickup` al recoger un material (`PickupMessage`).
- Consumo al comer o curar.

---

## Fuentes y licencia de SFX

Fiel a `NORTE.md`, el sistema no depende de un catálogo externo:

- **Default:** grabación, foley o síntesis propia, con licencia SPDX y
  procedencia declaradas.
- **Fallback opcional:** archivos explícitamente CC0/dominio público de
  OpenGameArt o Freesound, verificados uno por uno. Que un sitio o bundle sea
  gratuito no lo vuelve CC0. Los bundles GDC de Sonniss quedan fuera: usan una
  EULA royalty-free propia.
- **Reproducibilidad:** ninguna fuente necesaria exige cuenta, pago o descarga no
  automatizable.
- **Formatos:** `.ogg` a 44.1 kHz — mono para espacializados 3D, estéreo para
  UI y música. Las features `vorbis` y `wav` de Bevy ya están activas en este
  build, así que no hay nada que habilitar.
- **Procedencia:** el manifiesto de audio sigue el mismo patrón que el de
  texturas (`TEXTURES.md`, ley 6): una fila por archivo con autor, origen y
  licencia. Se crea con el primer clip, no después.

---

## Los errores que este documento ya cometió

1. **Escribir el catálogo completo en presente.** Ocho `CueId` descritos como si
   existieran, cuando en el código hay dos y uno no lo emite nadie.
2. **No tener ninguna cota de costo.** Es un documento de sistema en tiempo real
   sin un solo número de presupuesto: ni voces, ni CPU, ni memoria de clips.
   Bevy mezcla en CPU y el target es un teléfono.
3. **No decir que hoy el audio es un `info!`.** Está en `NORTE.md`, pero quien
   abre este documento para saber qué falta merecía leerlo acá.

---

## Orden de implementación

### Fase 1 — Infraestructura y cues de pisadas

- Mensajes `CueMessage`, `CueId`, `CueKind`. **Hecho.**
- Emisión de pasos por `StrideAccumulator` cada 2.0 m. **Hecho.**
- Lectura de `GroundFacts::surface` y seguimiento por umbrales. **Hecho.**

### Fase 2 — El primer sonido de verdad

- **Paso 2.1 — Un clip, una superficie.** Cargar un `.ogg` propio para
  `Step` sobre `Grass` y reproducirlo con `AudioPlayer`. Es el paso que convierte
  todo lo anterior de log en juego, y el que descubre lo que no se sabe: latencia,
  volumen relativo, si el ritmo de 2 m se siente bien al correr.
- **Paso 2.2 — La tabla completa de superficies.** `SurfaceKind →
  Handle<AudioSource>` en `SfxPlugin`.
- **Paso 2.3 — Espacialización y cota.** `with_spatial(true)` más la cota de
  25 m aplicada por `sfx` (ley 4) y el presupuesto de voces (ley 6).
- **Entregable & validación.** Caminar por terrenos distintos y escuchar el
  cambio; alejarse hasta que el emisor deje de crearse. Y una lectura de CPU en
  el hub F1 con y sin audio, que es el primer número que este documento va a
  tener.

### Fase 3 — Combate, entorno y UI

- Enchufar `HitImpactMessage` y `BowFiredMessage`, un `CueId` por vez y con su
  clip (ley 7).
- Fogatas (`FireEmitter`) y sonidos de UI al recoger objetos.

---

## Fuera de alcance

Oclusión de audio por raycast continuo (la reverberación de cueva queda
diferida), síntesis procedural en tiempo real, y música adaptativa por capas —
el norte sonoro de `NORTE.md` es música ambiental minimalista, que no la
necesita.
