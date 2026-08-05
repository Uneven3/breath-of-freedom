# Partículas y VFX — dirección técnica y efectos

Especificación del sistema objetivo de partículas, efectos visuales (VFX) y
"juice" (feedback táctil/visual). Reescrito el **2026-08-05**: las leyes de
desacoplamiento y suavizado eran correctas, pero el documento presentaba como
vigente una cota que no existe en el código, y no decía nada del único costo que
las partículas tienen en el target. Ver *Los errores que este documento ya
cometió*.

> **Cómo se usa este documento.** Referencia de diseño y parámetros para efectos
> visuales transitorios (chispas de hit, fuego, humo, briznas voladoras, arcos
> de barrido, hit flashes, squash & stretch). Código y `AHORA.md` indican el
> estado vivo. Cada efecto respeta las leyes de rendimiento y el desacoplamiento
> de simulación (§20).
>
> **Tres tipos de número.** *(a)* medición nuestra, con fecha; *(b)* aritmética
> verificable o propiedad del hardware objetivo; *(c)* decisión de diseño. Misma
> disciplina que `BOTWGrass.md`.

---

## El target manda: dónde se paga un VFX

`NORTE.md` fija el piso: Android de gama media ~2021, tile-based. Para
partículas, eso cambia dos intuiciones de escritorio *(tipo b)*:

**1. El blending es barato; el fill no.** En un tiler el framebuffer del tile
vive on-chip, así que mezclar un fragmento translúcido no cruza el bus — es la
diferencia más grande respecto de un GPU de escritorio, y significa que
`AlphaMode::Blend` **no es el enemigo acá** como sí lo es en vegetación densa.
Lo que se paga es el **fragment shader ejecutado**: una llamarada que cubre
media pantalla cuesta media pantalla de fill por capa, esté o no mezclada. Por
eso el presupuesto de partículas se cuenta en **área cubierta × capas**, no en
número de partículas.

**2. Un evento de combate no puede crear assets.** Cada `meshes.add` o
`materials.add` en respuesta a un golpe crea un buffer nuevo que hay que subir a
GPU, un pipeline potencialmente nuevo y una entrada más en el contador de
materiales. Es CPU y stalls de driver, justo en el frame que tiene que sentirse
instantáneo. En escritorio se disimula; en un teléfono es un hitch en el momento
más visible del juego.

Y como siempre: **acá no se mide el target**. Los milisegundos salen de la
Polaris del dev. El conteo de draws, materiales y triángulos sí transfiere.

---

## Lo que se ve y qué lo produce

| Lo que se ve | Técnica que lo produce | Estado |
|---|---|---|
| Chispas blancas al golpear (Hit VFX) | Burst de 8 partículas en abanico dorado con amortiguación exponencial (`BURST_DRAG_PER_SEC = 6.0`) | **Hecho** (`presentation/juice.rs`), con la deuda del Paso 1 |
| El personaje o enemigo se ilumina en blanco al recibir daño | `HitFlash` sobre el material del visual (`HIT_FLASH_COLOR = srgb(2.5, 2.5, 2.5)`, 0.12 s) | **Hecho** |
| El cuerpo se estira al saltar y se aplasta al aterrizar | `Jelly` squash & stretch en `VisualOf` (+28% al saltar, −24% al aterrizar) | **Hecho** |
| Arco de barrido al atacar | `SwingVfx` con `CircularSector` traslúcido (`unlit`, `AlphaMode::Blend`) | **Hecho**, placeholder hasta que haya animación |
| Daño flotante en pantalla | `DamageText` anclado a mundo y proyectado a UI (20 px normal, 30 px dorado en crítico) | **Hecho** |
| Sacudida de cámara y destello rojo al recibir daño | `ScreenFlash` (alpha 0.3) + trauma de `CameraShake` (`0.55`) | **Hecho** |
| Llamas de fuego / fogatas (Fuego VFX) | Quads billboard con `T_FXFire_Albedo`, `unlit`, ascenso y escalado | Objetivo |
| Columnas de humo (Humo VFX) | Billboards con rotación angular, expansión y disipación | Objetivo |
| Briznas volando al cortar pasto (Pasto VFX) | Quads de 2 tris teñidos raíz/punta, impulso parabólico al cortar celdas `cuttable` | Objetivo |

---

## Las siete leyes de este sistema

### 1. Presentación pura: los VFX jamás escriben en simulación (§20)

Partículas, fuego, humo, briznas cortadas, destello de golpe y texto de daño
leen eventos de simulación (`HitImpactMessage`, `GrassCutMessage`,
`BowFiredMessage`, `CueMessage`) pero **nunca modifican componentes de
gameplay**.

### 2. Amortiguación exponencial, no lineal

Toda desaceleración usa decaimiento exponencial (`smooth_nudge` / `exp`), nunca
`(1.0 - k * dt)`: esa forma llega a freno total en un frame si `k * dt >= 1.0`,
y arruina el suavizado justo cuando el juego va lento.

### 3. Presupuesto acotado — y hoy no hay nadie que lo haga cumplir

- **Hit burst:** máximo 8 partículas por impacto. *(Se cumple: es una constante.)*
- **Fire & Smoke:** emisores fijos, máximo 4-6 quads activos por emisor.
- **Grass Cut Debris:** máximo 6 briznas por corte.
- **Cota global:** `VfxBudget` admite como máximo **128 entidades VFX
  transitorias** simultáneas entre partículas, arcos y textos; los emisores
  reutilizan un pool fijo, y si no queda capacidad se conserva el efecto más
  cercano, descartando el más lejano. Los modificadores sobre entidades
  persistentes (`HitFlash`, `Jelly`) no consumen entrada.
- **Transitorios:** toda partícula lleva timer de vida rígido (`BURST_SECS =
  0.22`, `FIRE_PARTICLE_SECS = 0.45`, `GRASS_DEBRIS_SECS = 0.6`,
  `SWING_VFX_SECS = 0.16`, `DAMAGE_TEXT_SECS = 0.8`) y despawnea al expirar.

**`VfxBudget` no existe en el código.** No hay recurso, ni cota, ni política de
descarte: lo único que acota hoy es que cada efecto se apaga solo por su timer.
Con un enemigo y golpes espaciados alcanza; con una pelea de seis y fuego en
escena, nadie está mirando. Es el Paso 2.

### 4. Tolerancia a carreras de despawn (`try_insert` / `try_remove`)

Cuando un golpe letal despawnea un `Actor` en `FixedUpdate`, la entidad visual
puede desaparecer en el mismo tick. Los sistemas de VFX usan operaciones
tolerantes para no entrar en panic.

### 5. El alfa es excepción escrita, y son tres

El baseline sigue siendo opaco. Las excepciones vigentes:

1. **Fuego y humo** (`T_FXFire_Albedo` / `T_FXSmoke_Albedo`): pueden llevar
   alfa porque son pocos, transitorios y están bajo cota. Usan `Blend`, nunca
   `Mask`.
2. **El arco de barrido** (`SwingVfx`): `srgba(0.95, 0.95, 0.7, 0.45)` con
   `Blend` y `cull_mode: None`. Existe hoy y estaba fuera de esta lista.
3. **Los overlays de UI** (`ScreenFlash`, `DamageText`): alfa de pantalla, no de
   escena.

Las briznas de pasto volador usan geometría teñida con `ROOT_COLOR`/`TIP_COLOR`,
sin textura ni alfa. **La excepción no habilita alfa en vegetación
persistente**, y `Mask` no entra en ninguna de las tres: el problema del
alpha-test es el `discard` (ver `BOTWGrass.md`, ley 3), y ninguno de estos
efectos lo necesita.

**Cuidado con el fullscreen.** `ScreenFlash` es una capa translúcida a pantalla
completa: cuesta una pantalla entera de fill en el frame del impacto, que es
justo el frame más cargado. Es aceptable por ser un frame aislado y hay que
mantenerlo así — nunca una superposición permanente.

### 6. Un VFX no crea assets por evento

Mesh y material de un efecto se crean **una vez** y viven en un recurso; el
evento sólo spawnea entidades que los referencian. Un `meshes.add` o
`materials.add` dentro de un handler de impacto es CPU, subida a GPU y una
entrada más en el contador de materiales, en el peor frame posible (ver *El
target manda*, punto 2).

Hoy se viola en dos lugares: `burst_on_hit` crea una esfera y un material por
impacto, y `spawn_swing_vfx` crea un `CircularSector` y un material por swing.
Es el Paso 1.

### 7. Lo que parpadea necesita material propio

`HitFlash` guarda el `base_color` del material y lo sobrescribe. Si ese
`Handle<StandardMaterial>` está compartido entre entidades, **golpear a una las
tiñe a todas**, y dos flashes solapados dejan el material blanco de forma
permanente: el segundo guarda como "original" el color de flash del primero.

**Regla:** todo visual que pueda recibir un golpe se construye con
`palette.instance(...)`, no con `palette.handle(...)`. Los enemigos ya lo hacen
(`visuals/enemy.rs`) y el jugador también; el caballo comparte handle y hoy no
se nota porque hay uno solo. La regla se hace cumplir con un test, no con
memoria.

---

## Especificación por tipo

### 1. Partículas de impacto / hit (`src/presentation/juice.rs`)

- **Conteo:** 8 por golpe, abanico dorado determinista desde el punto de impacto
  — sin RNG en el hot path.
- **Velocidad inicial:** 5.0 m/s con arrastre exponencial
  `BURST_DRAG_PER_SEC = 6.0`.
- **Duración:** 0.22 s con encogimiento progresivo.
- **Geometría objetivo:** un quad billboard, o un icosaedro sin subdividir.
- **Geometría actual, y el problema:** `Sphere::new(0.07)` con el builder por
  defecto de Bevy, que es `SphereKind::Ico { subdivisions: 5 }` — **720
  triángulos y 362 vértices por chispa** *(tipo b: verificado en
  `bevy_mesh-0.19.0/src/primitives/dim3/sphere.rs`)*. Ocho por golpe son **5.760
  triángulos y ~2.900 vértices**, para bolitas de siete centímetros que viven
  0,22 s. Es más geometría que muchos props enteros del catálogo, y en el target
  el vértice es exactamente lo que se paga. Un icosaedro de subdivisión 0 son 20
  triángulos y se ve igual a ese tamaño: **36× menos**.

### 2. Hit flash

- **Duración:** 0.12 s. **Color:** blanco HDR `srgb(2.5, 2.5, 2.5)`.
- **Requisito:** material propio por entidad (ley 7).

### 3. Jelly squash & stretch

- **Salto:** `JELLY_JUMP_STRETCH = 0.28` (+28% en Y, −16.8% en XZ).
- **Aterrizaje:** `JELLY_LAND_SQUASH = -0.24` (−24% en Y, +14.4% en XZ).
- **Recuperación:** amortiguada a `JELLY_RECOVERY_PER_SEC = 9.0`.
- **Volumen:** la fórmula compensa visualmente; no promete conservación exacta
  ni se documenta como invariante física.

### 4. Arco de barrido (`src/visuals/vfx.rs`)

- **Duración:** 0.16 s. **Geometría:** `CircularSector` según el alcance
  (`step.reach`) y el arco (`step.arc_deg`) que publica el motor de ataque.
- **Material:** `unlit`, `Blend`, `cull_mode: None` (ley 5, excepción 2).
- **Deuda:** mesh y material por swing (ley 6). El mesh depende del arma, así
  que el pool se indexa por `(reach, arc_deg)`, no es uno solo.

### 5. Texto de daño flotante

- **Anclaje:** `P + (0, 1.2, 0)` proyectado con `world_to_viewport`.
- **Comportamiento:** asciende a 1.1 m/s y se desvanece en 0.8 s.
- **Estilo:** 20 px blanco; 30 px dorado (`srgb(1.0, 0.85, 0.2)`) en crítico.

### 6. Destello de pantalla y sacudida de cámara

- **Activación:** cuando el jugador local es blanco de un golpe.
- **Screen flash:** overlay UI a pantalla completa (`GlobalZIndex(50)`), alpha
  0.3, desvaneciendo a 2.2/s.
- **Camera shake:** `PLAYER_HIT_TRAUMA = 0.55`; al disparar el arco, entre 0.08
  y 0.25 según la carga.

### 7. Fuego y fogatas (objetivo)

- **Activación:** `FireEmitter` en campamentos, antorchas o terreno `flammable`
  encendido.
- **Material:** quad billboard con `T_FXFire_Albedo.png` (256², `T_FX` en
  `TEXTURES.md`), `unlit: true`, `Blend`, tinte cálido.
- **Comportamiento:** nace en la base, asciende a 1.2 m/s, escala de 0.2 a
  0.5 m, se desvanece en 0.45 s. Cuatro partículas por emisor, recicladas por
  timer.
- **Y lo que hay que vigilar:** una llamarada cerca de la cámara es el peor caso
  de fill del juego. El presupuesto se valida con el dial de overdraw, no
  contando partículas.

### 8. Humo (objetivo)

- **Activación:** `SmokeEmitter` acoplado a fuego o fuentes de calor.
- **Comportamiento:** quads que ascienden a 0.8 m/s, rotan a ±1.2 rad/s y
  expanden de 0.3 a 0.9 m mientras se disipan. Duración 1.2 s.

### 9. Pasto cortado / debris (objetivo)

- **Activación:** al cortar una celda `cuttable` con barrido de espada.
- **Geometría:** 4-6 quads de 2 triángulos teñidos con el gradiente
  `ROOT_COLOR → TIP_COLOR` del pasto.
- **Comportamiento:** impulso parabólico ($V_y = 2.5$, $V_{xz} = 1.8$ m/s),
  gravedad ligera, rotación libre; 0.6 s hasta tocar el suelo y despawnear.
- **Dependencia:** `BOTWGrass.md` es dueño de qué significa "cortar" y de los
  colores; acá sólo vive la presentación del debris.

---

## Los errores que este documento ya cometió

1. **Escribir una cota como si existiera.** `VfxBudget`, el pool y la política
   de descarte por distancia estaban redactados en presente y en detalle; en el
   código no hay nada. Un documento que describe un guardarraíl inexistente es
   peor que uno que no lo menciona: hace que nadie lo busque.
2. **No listar el arco de barrido como excepción de alfa**, cuando es la única
   excepción que realmente corre hoy.
3. **Detallar duraciones y velocidades y no mirar la geometría.** Ocho esferas
   de 720 triángulos pasaron por acá sin que ninguna fila de la tabla lo
   notara — porque el documento hablaba de "conteo de partículas", que era la
   unidad equivocada.
4. **Contar partículas en vez de área cubierta.** En el target el costo de un
   VFX es fill; ocho chispas de 7 cm y una llamarada de dos metros no se parecen
   en nada y el presupuesto las trataba igual.
5. **No decir que crear assets por evento es un problema.** Es la clase de cosa
   que sólo se ve como un hitch ocasional y nunca en una media.

---

## Orden de implementación

### Paso 1: Que un golpe no cree nada

- **Lógica.** Leyes 6 y 7. Mesh y material del burst pasan a un recurso creado
  una vez; el mesh de la chispa baja a un icosaedro de subdivisión 0 o a un
  quad. El pool del arco se indexa por `(reach, arc_deg)`.
- **Entregable & validación.** Golpear veinte veces seguidas y ver `mats` y
  `tris` del hub F1 planos, cuando hoy suben con cada impacto. Es el único paso
  de este documento cuyo resultado se lee sin cronómetro.

### Paso 2: `VfxBudget` de verdad

- **Lógica.** Ley 3. Un recurso que cuenta entidades transitorias vivas, con la
  política de descartar el efecto más lejano al llenarse.
- **Entregable & validación.** Un test que sature el presupuesto y afirme que la
  cuenta nunca lo supera y que lo descartado es lo más lejano; y una pelea
  jugada con seis enemigos sin que el frame se despegue.

### Paso 3: Fuego y humo

- **Lógica.** Primero el emisor y el reciclado, después la textura. Entra bajo
  las secciones 7 y 8, con la cota del Paso 2 ya puesta.
- **Entregable & validación.** Una fogata de noche con el dial de overdraw
  encendido, y el delta de frame de acercarse hasta llenar la pantalla de
  llamas. Ese es el número que decide cuántas partículas por emisor son
  aceptables — no la tabla.

### Paso 4: Debris de pasto

- **Lógica.** Depende de que exista el corte de pasto en `BOTWGrass.md`.
- **Entregable & validación.** Cortar pasto y ver briznas del color correcto
  volando, sin que la simulación sepa que existieron (ley 1).

---

## Fuera de alcance

Sistemas de partículas por GPU compute, simuladores de fluidos volumétricos y
mallas destructibles procedurales. También, deliberadamente: partículas con
sombras, y cualquier VFX que necesite leer el depth buffer (soft particles) —
en un tiler eso fuerza un resolve del depth a memoria, que es exactamente lo que
la arquitectura existe para evitar.

## Cómo se mide

Hub **F1**: `mats`, `draws` y `tris` para todo lo del Paso 1 y 2 —son conteos,
así que se leen directo y pueden fijarse con un test—, y el **dial de overdraw**
para todo lo que cubra área (fuego, humo, screen flash). El frame time se mide
con A/B/A desde el mismo punto, warmup excluido; mediana y p95, y un delta que
no supera la deriva entre baselines es ruido.
