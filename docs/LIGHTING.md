# Iluminación y atmósfera — dirección técnica y parámetros

Hoja de ruta y especificación del sistema objetivo de iluminación, ciclo
día/noche, sombras, antorchas/interiores y atmósfera. Reescrito el
**2026-08-05**: los parámetros estaban bien y siguen acá, pero el documento no
decía contra qué máquina se justifican, no llevaba la única medición grande de
iluminación que este proyecto tiene, y prometía dos cosas que el código no puede
cumplir. Ver *Los errores que este documento ya cometió*.

> **Cómo se usa este documento.** Es la referencia autoritativa de lo que se
> quiere construir para el ciclo día/noche, antorchas, sombras, niebla y clima.
> El estado vivo se consulta en código y `AHORA.md`; acá quedan decisiones,
> parámetros objetivo y criterios de aceptación. Cualquier cambio de
> iluminación se mide con el hub F1.
>
> **Filosofía (§ `NORTE.md`).** La belleza de este juego es **luz + color +
> atmósfera**, no detalle geométrico ni texturas complejas. Iluminación PBR
> estilizada sobre `StandardMaterial` plano y mate (`perceptual_roughness ≥
> 0.8`, `metallic = 0.0`).
>
> **Tres tipos de número, y no se mezclan.** *(a)* **Medición nuestra**: sale
> del hub F1, lleva fecha y escena. *(b)* **Propiedad del hardware objetivo**:
> cómo se comporta un GPU tile-based; es conocimiento de ingeniería, no
> medición nuestra. *(c)* **Estimación**: lleva la palabra *estimado* y el
> cálculo al lado. Es la misma disciplina de `BOTWGrass.md`.

---

## El target manda: qué le hace un tiler a la iluminación

`NORTE.md` fija el piso —Android de gama media ~2021, tile-based— y
`BOTWGrass.md` desarrolla el marco general. Acá sólo van las tres consecuencias
que son de este documento:

**1. Una cascada no es una textura: es la escena otra vez.** Cada cascada
rerenderiza toda la geometría que cae en su volumen, con su propio pase. Con 4
cascadas, un árbol cerca de la cámara se dibuja **cinco veces por frame**: una
para el ojo, cuatro para el sol. En un tiler *(tipo b)* cada pase además paga su
propio ciclo de binning y su store a memoria. El multiplicador de sombras cae
sobre draws y vértices, que es exactamente lo que el aparato menos tiene.

**2. Y encima se escribe y se lee.** Aritmética *(tipo c)*: 4 cascadas de 1024²
a 32 bits son 16 MB escritos por frame, ~1 GB/s a 60 fps, antes de contar el
muestreo. El perfil móvil —2 cascadas de 512²— baja eso a 2 MB por frame,
~126 MB/s: **ocho veces menos**, y es la razón por la que ese perfil existe. Los
`SHADOW_MAP_STEPS` son la palanca más directa que hay sobre el costo de sombras
porque el costo es cuadrático en el lado.

**3. Una luz puntual es fill, no geometría.** Bevy usa forward clustered: cada
`PointLight` suma iteraciones al fragment shader de cada píxel dentro de su
cluster. Cuesta por píxel cubierto, no por objeto iluminado — así que el
parámetro que gobierna su costo es el **rango**, no la intensidad. Una antorcha
de rango 6 m es barata; la misma antorcha con rango 30 m ilumina lo mismo a ojo
y cuesta veinticinco veces más área.

**Y nada de esto está medido en el target.** Todos los milisegundos de este
documento salen de la **AMD Polaris 11 (RX 460)** del dev, un renderer de modo
inmediato. Sirven para ordenar palancas entre sí; no autorizan a declarar que la
iluminación "entra". Lo que sí transfiere es el conteo de pases y la aritmética
de arriba.

---

## Lo que se ve y qué lo produce

| Lo que se ve | Técnica que lo produce | Por qué |
|---|---|---|
| El sol nace coral y muere magenta | Paleta separada para Dawn (`SUN_DAWN_COLOR`) y Dusk (`SUN_DUSK_COLOR`) | Le da identidad cromática distinta a la mañana y a la tarde |
| La noche no es negra ni aburrida | Luna direccional a 400 lux + ambiente azul frío (`brightness = 40.0`) | La noche se mantiene navegable sin aplanar el contraste |
| Antorchas y fogatas iluminan cálidamente los campamentos | `PointLight` locales (300-500 lm, rango 6-10 m, **sin sombras**) | El rango acotado es lo que acota el costo (ver ley 4) |
| Entrar en cuevas oscurece el ambiente | Factor de interior que entra en `lighting_palette`, no una segunda escritura del ambiente | Sensación de interior penumbroso sin baked lightmaps y sin dos dueños del mismo recurso (ley 7) |
| El sol y la luna no cambian de tamaño ni flotan al caminar | Órbita centrada en la **cámara** (`DISC_ORBIT_RADIUS = 420 m`) | Un astro en el infinito no debe tener paralaje al moverse por el mapa |
| El mediodía no paga cascadas invisibles | Corte por iluminancia (`SHADOW_CASTING_LUX = 1.0`) | Evita que la luna renderice sus cascadas a pleno sol, y el sol a medianoche |
| Las sombras lejanas no gastan resolución | `maximum_distance` ajustado al alcance real de los árboles | Concentra los texeles de sombra en la geometría cercana visible |
| El bosque no se come el frame por las sombras | Hojas sin `NotShadowCaster` fuera del baseline + presupuesto de casters por distancia | Es la mayor optimización medida del proyecto (ley 1) |
| El horizonte se desvanece en el color del cielo | `DistanceFog` lineal (45 m → 240 m) sincronizada con `atmosphere_color(hours)` | El color de niebla es el mismo `ClearColor` del cielo, así que el borde no cambia de tono |

**Lo que hoy *no* se ve, aunque este documento lo prometía:** el horizonte no se
funde del todo. `FOG_MAX_ALPHA = 0.3` (`camera/mod.rs:51`) es un techo duro sobre
la mezcla —en `bevy_pbr-0.19.0/src/render/fog.wgsl`, `linear_fog` multiplica el
factor de distancia por el alfa del color— así que **a cualquier distancia el
terreno lejano sigue siendo 70% él mismo**. Un velo del 30% no cierra un
horizonte; la niebla actual es atmósfera, no LOD. Ver Fase 3.

---

## Las siete leyes de este sistema

### 1. La sombra es el gasto dominante, y eso ya está medido

No es una intuición: es la única palanca de iluminación que se salió del ruido
en todas las cajas medidas *(tipo a)*.

| medición | fecha | resultado |
|---|---|---|
| Bosque, sol con hojas casteando vs. sin ellas + mapa a 1024 | 2026-07-21 | de ~70% del frame a **2,74 ms** — de 15 a 51 fps |
| Caja `Pasto`, 7 configuraciones A/B | 2026-07-25 | sombras **−0,66 ms**, la única palanca fuera del ruido |

De ahí salen los defaults que hoy shippea `PerfToggles`: `leaf_shadows: false` y
mapa de 1024 px. **No son preferencias, son el resultado**, y por eso el
proyecto arranca en ellos en vez de arrancar en el caso caro
(`crates/domain/src/perf.rs`).

**Regla:** cualquier decisión de iluminación que agregue geometría a un pase de
sombra —una luz direccional más, una cascada más, un caster más lejano— se mide
antes de entrar. El orden de las palancas, de más a menos efectiva: qué castea,
resolución del mapa, distancia de cascadas, número de cascadas.

### 2. Las sombras se cortan por iluminancia — y sólo un astro a la vez

Bevy evalúa `shadow_maps_enabled` sin mirar la iluminancia de la luz. Sin
protección, la luna renderiza sus cascadas completas en pleno mediodía (y el sol
a medianoche).

**Regla (§`src/world/day_night.rs`):** si `illuminance < SHADOW_CASTING_LUX`
(1.0 lux), la luz apaga sus sombras en GPU. Implementado y con test.

**Pero el corte por lux no alcanza, y hoy hay una ventana doble.** Sol y luna
deciden por separado, y en el crepúsculo los dos superan el umbral a la vez:
`sun_visibility` y `moon_visibility` interpolan en `elevation ∈ [-0.08, 0.08]`,
así que en el cruce ambos valen ~0,5 y **ambos rondan los 200 lux** — muy por
encima de 1. Aritmética *(tipo c)*: esa banda dura ~±0,32 h de reloj de juego, y
con el día de 24 minutos reales son **~1,3 minutos reales de cascadas dobles en
cada crepúsculo, dos veces por día de juego**. Ocho pases de sombra en desktop,
cuatro en móvil, justo cuando el sol rasante produce los volúmenes de cascada
más grandes que hay. El test `only_the_light_above_the_horizon_casts_shadows`
prueba mediodía y medianoche, así que no lo ve.

**Regla objetivo:** el corte es **comparativo**, no absoluto. Sólo el astro
dominante proyecta; el otro proyecta únicamente si el dominante está bajo el
umbral. Es un cambio local en `apply_sun` y un caso de test en la hora del
cruce. No implementado.

### 3. El número de cascadas se fija al arranque

Cambiar el número de cascadas en caliente desincroniza la contabilidad interna
de Bevy (`check_dir_light_mesh_visibility` dimensiona sus colas según los
frusta) y produce un **panic por índice fuera de rango**. Una perilla de debug
que puede colgar el juego es peor que no tenerla (§9).

**Regla:** el conteo (1 a 4) se fija al lanzar, con `BOF_CASCADES` o el perfil
de rendimiento. La *distancia* sí es un dial en vivo (`apply_cascade_config`),
porque dónde termina el disco sombreado se juzga mirando, no leyendo una tabla.

### 4. Cero sombras en luces puntuales, y el rango es el presupuesto

Las luces puntuales (antorchas, fogatas, cristales) iluminan geometría cercana
pero **tienen prohibido proyectar sombras dinámicas**. Una `PointLight` con
sombras necesita hasta seis caras de cubemap: seis pases más, por luz.

**Regla:** sólo el sol y la luna proyectan sombras. Y como el costo de una luz
puntual es área de pantalla cubierta (ver *El target manda*, punto 3), **el
rango es el número que se presupuesta**, no la intensidad: subir intensidad
cuesta cero, subir rango cuesta cuadráticamente.

### 5. Astro en el infinito = órbita centrada en la cámara

Centrar la órbita del sol/luna en el origen del mundo genera paralaje: cruzar
112 m de un mapa de 320 m desplaza el disco ~14° y lo acerca de 420 m a ~308 m,
o sea que el sol se mueve y crece mientras caminás.

**Regla:** `SunDisc` y `MoonDisc` se posicionan en
`camera_translation + dir * DISC_ORBIT_RADIUS`.

### 6. Iluminación estilizada sobre PBR mate, sin toon shader

`NORTE.md` lo declara: nada de toon shaders ni outlines fullscreen en baseline.
El look sale de otra parte:

- Materiales mate planos (`roughness ≥ 0.8`, `metallic = 0.0`).
- Sombras nítidas, 2 a 4 cascadas según perfil.
- Gradientes de atmósfera explícitos por hora.

La luz ambiente se mantiene baja a propósito (`AMBIENT_DAY = 90.0`,
`AMBIENT_NIGHT = 40.0`): demasiado ambiente rellena las caras en sombra y aplana
el volumen de mallas y terreno hasta que todo se funde.

### 7. La luz ambiente tiene un solo escritor

`apply_sun` escribe `GlobalAmbientLight.brightness` y `.color` **cada frame**
desde la paleta del ciclo. Cualquier otro sistema que escriba ese recurso es
pisado en el mismo frame, sin error ni advertencia: el síntoma sería una cueva
que no oscurece y ninguna pista de por qué.

**Regla:** el ciclo día/noche es el único escritor de la luz ambiente. Lo que
quiera modificarla —interiores, clima, tormenta— entra como **entrada de
`lighting_palette`**, no como una segunda escritura. Es lo que rediseña la
Fase 2.

---

## Parámetros del sistema objetivo

Verificados contra `src/world/day_night.rs` y `crates/domain/src/perf.rs` el
2026-08-05.

### 1. Sol y Luna (`src/world/day_night.rs`)

| Parámetro | Sol (Día) | Luna (Noche) |
|---|---|---|
| Iluminancia máxima | `10 000.0 lux` (mediodía) | `400.0 lux` |
| Color zenith / medianoche | `srgb(1.0, 0.98, 0.92)` | `srgb(0.55, 0.65, 0.9)` |
| Color Dawn (04:30 – 07:45) | `srgb(1.0, 0.68, 0.42)` (coral) | N/A |
| Color Dusk (16:15 – 20:00) | `srgb(1.0, 0.38, 0.2)` (magenta cálido) | N/A |
| Inclinación del arco | `SUN_ARC_TILT = 0.35`, evita sombras colapsadas en línea | **El mismo `+0.35`**, no el opuesto |
| Duración del día | **24.0 minutos reales** por día de juego (ritmo BOTW) | — |

La luna no es `-to_sun`: es `(-x, -y, +SUN_ARC_TILT)`. Se refleja en el plano
horizontal pero **conserva la inclinación del sol**, así que los dos arcos
corren inclinados hacia el mismo lado en vez de cruzarse. Es lo que hace el
código y se ve bien; queda escrito acá porque una lectura descuidada ("opuesta
al sol") invita a "arreglarlo" invirtiendo la z, y eso mueve el arco nocturno.

### 2. Luz ambiente y cielo

| Parámetro | Día | Aurora | Crepúsculo | Noche |
|---|---|---|---|---|
| `GlobalAmbientLight::brightness` | `90.0` | interpola | interpola | `40.0` |
| Color ambiente | `srgb(1,1,1)` | `srgb(1.0, 0.65, 0.52)` | `srgb(0.9, 0.42, 0.52)` | `srgb(0.38, 0.48, 0.78)` |
| Color de cielo (`ClearColor`) | `srgb(0.45, 0.68, 0.95)` | `srgb(0.95, 0.42, 0.38)` | `srgb(0.72, 0.2, 0.42)` | `srgb(0.055, 0.075, 0.17)` |

Hoy el cielo es un `ClearColor` plano, no un gradiente: por eso la niebla puede
tomar exactamente el mismo color y coincidir. Cuando llegue la cúpula con
gradiente vertical (`TEXTURES.md`, Paso 9), la niebla tendrá que tomar el color
**a la altura del horizonte** o reaparece el borde.

### 3. Luces puntuales (`PointLight`) — objetivo, no implementado

No existe ninguna `PointLight` en el código todavía. Estos valores son
decisiones iniciales de autoría y se afinan jugando.

| Tipo | Intensidad | Rango | Color | Sombras |
|---|---|---|---|---|
| Fogata (`Campfire`) | `500.0 lm` | `10.0 m` | `srgb(1.0, 0.55, 0.2)` | `false` (ley 4) |
| Antorcha (`Torch`) | `300.0 lm` | `6.0 m` | `srgb(1.0, 0.6, 0.25)` | `false` (ley 4) |
| Cristal mágico (`Crystal`) | `200.0 lm` | `4.0 m` | `srgb(0.2, 0.7, 1.0)` | `false` (ley 4) |

`PointLight::intensity` es **flujo luminoso en lúmenes**, no iluminancia en lux.

- **Cota:** máximo 8 `PointLight` activas dentro del volumen de visión. Es una
  decisión, no una medición: el número sale de que el costo es área cubierta, y
  ocho fuentes de 6-10 m no se solapan en pantalla en un campamento. Se valida
  cuando exista la primera fogata, con el dial de overdraw encendido.

### 4. Sombras y cascadas (`src/world/day_night.rs`, `src/perf/`)

| | desktop (default) | móvil (`BOF_PROFILE=mobile`) | pasos |
|---|---|---|---|
| Resolución del mapa | `1024²` | `512²` | `2048`, `1024`, `512` |
| Cascadas | `4` | `2` | `BOF_CASCADES=1..4` |
| Distancia de cascadas | `65 m` | `40 m` | `65`, `100`, `140`, `200`, `40` |
| Alcance de casters | `60 m` | `30 m` | `60`, `120`, `30`, sin límite |
| Sombras de hojas | `off` | `off` | on/off |

### 5. Niebla atmosférica (`src/camera/mod.rs`)

- **Tipo:** `DistanceFog` con `FogFalloff::Linear { start: 45.0, end: 240.0 }`.
- **Color:** `atmosphere_color(hours)` en vivo, el mismo del `ClearColor`.
- **Techo de mezcla:** `FOG_MAX_ALPHA = 0.3`. Con eso la niebla es un velo, no
  un cierre. Ver Fase 3, Paso 3.1.

---

## Los errores que este documento ya cometió

Van acá porque cada uno sonaba razonable, y porque un documento que sólo lista
sus conclusiones invita a repetirlos.

1. **Prometer que el horizonte "se funde" con un alfa de 0.3.** El objetivo y el
   parámetro se contradecían, y el test de `camera/mod.rs` congelaba el
   parámetro (`alpha <= 0.3`) con el nombre "velo translúcido". Nadie iba a
   descubrirlo leyendo: hubo que ir al shader de Bevy.
2. **Describir la luna como "opuesta al sol (`-to_sun`)".** El código conserva
   la z. Una corrección "obvia" habría movido el arco nocturno.
3. **Diseñar `InteriorLightingTrigger` como un segundo escritor de
   `GlobalAmbientLight`.** No falla ruidosamente: simplemente no pasa nada.
4. **No llevar la medición de sombras.** El resultado más grande de rendimiento
   del proyecto es de iluminación, y vivía en un comentario de `perf.rs` y en
   `AHORA.md`, no en el documento dueño del tema.
5. **Justificar parámetros sin nombrar la máquina.** "No se alenta en GPU" no
   dice nada: en el target el costo de una cascada es un pase entero de escena
   más su bandwidth, y de ahí sale por qué el perfil móvil baja a 2×512².
6. **Proponer una tecla nueva (F9) para algo que ya es un click.** El hub ya
   tiene `DebugAction::ToggleTimeSpeed`, y las teclas sueltas F7/F8 se
   retiraron a propósito.

---

## Orden de implementación

### Fase 1 — Ciclo base y sombras

- Transición continua sol/luna con iluminancia y colores por hora. **Hecho.**
- Niebla lineal adaptada al color del cielo. **Hecho** (con la salvedad del
  alfa, Fase 3).
- Corte de cascadas por lux (`SHADOW_CASTING_LUX`). **Hecho.**
- **Paso 1.1 — Cerrar la ventana crepuscular.** Corte comparativo entre sol y
  luna (ley 2). *Entregable & validación:* un test que barra la hora del cruce
  y afirme que nunca hay dos luces direccionales casteando a la vez; y una
  corrida A/B en el crepúsculo, que es donde el frame debería dejar de tener un
  escalón. No implementado.

### Fase 2 — Luces puntuales e interiores

- **Paso 2.1 — La primera fogata.** `PointLight` con `shadow_maps_enabled:
  false` y rango acotado. *Entregable & validación:* un campamento de noche con
  el dial de overdraw encendido y el delta de frame de encenderla, medido.
- **Paso 2.2 — Interiores sin segundo escritor.** El volumen de cueva publica un
  **factor de interior** que `lighting_palette` consume junto con la hora (ley
  7); `apply_sun` sigue siendo el único que escribe `GlobalAmbientLight`.
  *Entregable & validación:* entrar a una cueva a mediodía oscurece; salir
  restaura exactamente la paleta de la hora, sin histéresis ni salto.

### Fase 3 — Atmósfera que además es LOD

- **Paso 3.1 — Decidir qué es la niebla.** Hoy es un velo del 30% y el documento
  pedía un cierre. Son dos cosas distintas y hay que elegir una: si la niebla
  tiene que ocultar el borde del mundo y el final de las bandas de
  `VisibilityRange`, el alfa tiene que llegar a 1.0 y `start`/`end` se ajustan
  para que el juego cercano siga limpio. *Entregable & validación:* desde el
  punto más alto que se pueda esculpir, el terreno lejano no se distingue del
  cielo — o se acepta explícitamente que no es ese el objetivo y se borra la
  promesa. Es la decisión que `TEXTURES.md` Paso 10 está esperando.

### Fase 4 — Clima y tormentas

- Oscurecimiento del sol/luna durante la lluvia, por la vía de la ley 7.
- **Relámpago sin hitch.** Un destello de `50 000 lux` en una direccional cruza
  `SHADOW_CASTING_LUX` y **enciende las cascadas por un frame**, justo el frame
  que tiene que ser instantáneo. El destello va en una luz que no castea, o el
  corte de la ley 2 se hace explícito para él. *Entregable & validación:* el
  frame del relámpago no se despega de la mediana.

---

## Interfaz de diagnóstico

- **F1** abre el hub. Desde sus acciones se emiten
  `TimeOfDayRequest::{AdvanceHour, ToggleSpeed}` — ya existen como
  `DebugAction::{AdvanceHour, ToggleTimeSpeed}`.
- **No se agregan teclas para esto.** El hub es "una sola tecla, el resto es
  click"; F7/F8 sueltas ya se retiraron a favor de acciones del hub, y
  reintroducir F9 desharía esa decisión.
- `BOF_CASCADES=N` y `BOF_PROFILE=mobile` se fijan al lanzar
  (ej. `BOF_CASCADES=2 cargo run --release`).

---

## Fuera de alcance

Volumetric fog y god rays (diferidos hasta tener presupuesto GPU holgado en
móvil: son fill puro, que es justo lo que el target no tiene), SSAO no medido, y
cubemaps de cielo estáticos — descartados con motivo en `TEXTURES.md` Paso 9, no
por costo de implementación.

## Cómo se mide

Hub **F1**. Las palancas de este documento son `sun-shadow`, `moon-shadow`,
`shadow-dist`, `shadow-map`, `shadow-range` y `leaf-shadows`, todas en
`PerfKnob`, así que entran en la matriz A/B de `perf/sequence.rs` con warmup y
chequeo de deriva. El dial de overdraw es el instrumento para las luces
puntuales. Se reportan mediana y p95; un delta que no supera la deriva observada
entre baselines es ruido.
