# MVPs incrementales — en qué orden se construye lo que los planes proponen

Los documentos de dominio dicen **qué** hay que construir y por qué. Este dice
**en qué orden**, y agrupa el trabajo en incrementos que se pueden jugar y
medir de a uno.

> **Qué es este documento.** Un plan de ingeniería transversal, como fue
> `docs/CRATES.md`. **Temporal por definición**: cada MVP se borra de acá cuando
> cierra, y cuando no quede ninguno el archivo se borra entero. No fija leyes —
> las leyes viven en `ARCHITECTURE.md` y en los documentos dueños de cada tema.
> Si hay contradicción entre esto y el documento dueño, manda el dueño.
>
> **Cómo se usa.** Uno por vez, de arriba hacia abajo. Un MVP no está cerrado
> hasta que se **jugó** y su criterio de aceptación se cumplió; si al jugarlo
> aparece algo mejor que hacer, se reordena la lista y se escribe por qué.
>
> **Qué es un MVP acá.** El incremento más chico que deja el juego
> **medible o mejor**, no una tarea. Varias tareas de documentos distintos
> pueden caer en el mismo MVP si comparten criterio de aceptación.

---

## MVP 0 — Poder medir

**Sin esto, todos los MVP que siguen se justifican con estimaciones.** Es el
único bloque cuyo entregable no cambia nada de lo que se ve, y es el que hace
que los demás sean decidibles en vez de opinables.

**Qué se construye**

1. **Los diales sueltos entran al hub.** `GrassDensity` pasa a `PerfKnob`, se
   borra el `KeyCode::F8` de `visuals/grass.rs`, y el dial queda dentro de la
   matriz de `perf/sequence.rs`. Hoy es el único ajuste visual fuera del
   registro, así que lo que produce no tiene warmup, ni asentamiento, ni
   chequeo de deriva: no es atribuible.
   *(`BOTWGrass.md` Paso 0 · `GraphicalTechniques.md` ley 6.)*
2. **El overdraw publica un número.** Hoy es un mapa de calor aditivo que
   satura alrededor de las 17 capas: responde "¿dónde?" pero no "¿cuánto?" ni
   "¿mejoró?". Es el instrumento principal para un GPU tile-based y es el único
   que no se puede poner en una tabla A/B.
   *(`GraphicalTechniques.md` Fase 1.)*
3. **El juego arranca en un teléfono.** No existe build de Android en el repo:
   ni script, ni receta, ni una sola corrida. Todo lo que los documentos llaman
   "el target" es hoy un razonamiento sobre una arquitectura. No hace falta que
   corra bien — hace falta que **arranque y se pueda medir**.
   *(`GraphicalTechniques.md` Fase 5.)*

**Criterio de aceptación**

Una corrida de la secuencia A/B con densidad de pasto como dial, con su paso de
baseline repetido mostrando la deriva; una lectura de overdraw con cifra
antes/después de un cambio conocido; y una captura del juego corriendo en el
teléfono, con su número de frame, aunque sea malo.

**Qué desbloquea:** todo. En particular, el resultado puede reordenar los MVP
3 y 4: si el pasto resulta fill-bound, la palanca es la densidad del anillo
cercano y no el shader.

---

## MVP 1 — Que un golpe no cree nada

El MVP más barato de la lista y el único cuyo efecto se lee sin cronómetro.

**Qué se construye**

- Mesh y material del hit burst pasan a un recurso creado una vez, en vez de un
  `meshes.add` + `materials.add` **por impacto**.
- La chispa deja de ser una icosfera de **720 triángulos** —el default de
  `Sphere` en Bevy— y pasa a un icosaedro sin subdividir (20 tris) o un quad.
  Son bolitas de 7 cm que viven 0,22 s: hoy ocho de ellas son 5.760 triángulos
  por golpe.
- El arco de barrido usa un pool indexado por `(reach, arc_deg)` en vez de
  crear malla y material por swing.
- `VfxBudget`: el recurso que cuenta entidades transitorias vivas y descarta la
  más lejana al llenarse. Está escrito como ley desde hace tiempo y no existe.

*(`PARTICLES.md` Pasos 1 y 2, leyes 3 y 6.)*

**Criterio de aceptación**

Golpear veinte veces seguidas con el hub F1 abierto: `mats` y `tris` quedan
planos, cuando hoy suben con cada impacto. Y una pelea con seis enemigos sin
que el frame se despegue.

---

## MVP 2 — El suelo se paga como corresponde

El terreno es la superficie de mayor cobertura de pantalla del juego, y hoy se
muestrea de la peor forma posible.

**Qué se construye**

1. **`basis-universal` habilitado** en el `Cargo.toml` de este juego
   (`bevy = { workspace = true, features = ["basis-universal"] }`). Sin esa
   feature, Bevy **no transcodifica** KTX2 universal, que es exactamente lo que
   el plan de texturas da por hecho. Verificado con `cargo tree`: hoy están
   `ktx2` y `zstd_rust`, no `basis-universal`.
2. **Mips en el array del terreno.** `array_image` construye la imagen con un
   solo nivel aunque el sampler pida filtrado de mip y anisotropía 16×. Sin
   mips, la minificación lee memoria al azar: en un tiler eso es bandwidth puro
   y es invisible en escritorio.
3. **Anisotropía a lo que se note.** 16× es el valor más caro posible y se
   eligió sin medir. A/B entre 16×, 4× y 1× **después** de que existan mips.
4. **`assets/textures/SOURCES.ron`** con una fila por archivo (autor, origen,
   licencia SPDX) y un test que falle si hay un PNG sin fila. Hay diez PNG en el
   repo sin procedencia declarada, en un proyecto GPL.

*(`TEXTURES.md` Pasos 0, 4 y 5.)*

**Criterio de aceptación**

El array entero bajo el tope de 2 MB, con mips, indistinguible del PNG a
distancia de juego; una lectura de overdraw antes/después (si los mips valen
algo, se ve en el suelo); y el test de procedencia en verde.

---

## MVP 3 — Las sombras dejan de tener un escalón

Las sombras son la palanca más cara ya medida del proyecto: llevar las hojas a
no castear y el mapa a 1024 px las bajó de ~70% del frame a 2,74 ms.

**Qué se construye**

- **Cerrar la ventana crepuscular.** Sol y luna deciden por separado si
  castean, y en el cruce los dos superan el umbral: ~1,3 minutos reales de
  **cascadas dobles** en cada crepúsculo, dos veces por día de juego, justo
  cuando el sol rasante produce los volúmenes más grandes. El corte pasa a ser
  comparativo: sólo el astro dominante castea.
- **El caso del relámpago, de una vez.** Un destello de 50.000 lux cruza el
  umbral y enciende las cascadas por un frame — el frame que tiene que ser
  instantáneo. El destello va en una luz que no castea.
- **La matriz de sombras, corrida en el teléfono** (necesita MVP 0): mapa,
  distancia, alcance de casters y cascadas. Los defaults actuales se eligieron
  midiendo en una GPU de escritorio.

*(`LIGHTING.md` ley 2, Fases 1 y 4.)*

**Criterio de aceptación**

Un test que barra la hora del cruce y afirme que nunca hay dos direccionales
casteando; el frame del crepúsculo sin escalón; y una tabla de sombras con
números del target.

---

## MVP 4 — El pasto se paga solo

El bloque más grande, y el que más cambia lo que se ve. Va después del MVP 0
porque su primer paso es una medición que puede reordenar el resto.

**Qué se construye, en este orden**

1. **`ExtendedMaterial` enchufado y el shader arreglado.** `grass_material.rs` y
   `grass.wgsl` existen, están registrados y **no se usan**; el shader tiene
   tres bugs reales (posición sin transformar a clip space, fragment que nunca
   llama al PBR, `vertex.color` ignorado). El entregable es que el campo se vea
   **idéntico** con el material nuevo puesto.
2. **El vértice adelgazado**: de 48 B a 12 B, quitando normal, color y uv, que
   son derivables. No cambia un píxel y es prerrequisito de todo lo demás.
3. **El terreno teñido** del mismo verde que la raíz de la brizna: es lo que
   hace pagables las densidades de los anillos, porque las briznas dejan de ser
   responsables de tapar el suelo.
4. **Los anillos de densidad**: la densidad necesaria cae como 1/d, así que a
   20 m las 45 briznas/m² actuales son treinta veces la cobertura necesaria.
   6,2× el área por el 78% de los triángulos.

*(`BOTWGrass.md`, Fases 0 a 2. Ahí están las cuentas.)*

**Criterio de aceptación**

Cada paso se cierra jugando la caja `Pasto` y midiendo con la secuencia A/B. Los
dos primeros deben ser **neutrales a la vista** — cambiar el motor sin cambiar
la imagen es la única forma de saber que el cambio fue limpio. Los dos últimos
se juzgan mirando: no debe verse el anillo donde baja la densidad, ni el borde
del campo.

---

## MVP 5 — El personaje se mueve como un personaje

Hoy el juego no reproduce ninguna animación, y la cápsula del jugador es un
poste de un metro de diámetro.

**Qué se construye**

1. **La cápsula correcta, como paso propio.** `BodyDimensions::PLAYER` pasa de
   radio 0.5 a 0.35. No es cosmético: el radio decide por dónde cabe, cómo se
   pega a la pared al escalar y cuánto sobresale al hacer mantle, y hay umbrales
   de `AutoVault`, `EdgeLeap` y `Climb` afinados contra el valor viejo sin
   saberlo. Va **antes** de afinar animaciones contra la escala, o se afina dos
   veces.
2. **Un clip se reproduce.** `src/visuals/animation.rs` con `AnimationRole`,
   `ROLE_TABLE` (cadenas de fallback), `CharacterAnimations` y el resolutor que
   aplica el rol al `AnimationPlayer` con crossfade. El contrato del que se
   cuelga —`PLAYER_CLIP_CONTRACT` y el guardrail de `build.rs`— ya existe y no
   tiene consumidor.
3. **Sin foot-sliding:** `k_speed_node = V_real / V_autorada`, protegido bajo
   0,05 m/s.

*(`BOTWMovements.md` secciones 3 y 5 · `CHARACTER_ANIMATION_IK.md` Pasos 0 y 2.)*

**Criterio de aceptación**

Rejugar la caja `Traversal` completa después del cambio de cápsula —escalar,
mantle, vault, salto de borde— y que ninguno se sienta distinto de como estaba
afinado. Después: el personaje camina y su animación camina con él.

---

## MVP 6 — El mundo suena y termina en algún lado

**Qué se construye**

1. **El primer sonido de verdad.** Un `.ogg` propio para `Step` sobre `Grass`,
   reproducido con `AudioPlayer`. Hoy `play_audio_cues` imprime por log y no hay
   un solo archivo de audio en el repo. Este paso descubre lo que no se sabe:
   latencia, volumen relativo, si el ritmo de 2 m se siente bien al correr.
   Después, la tabla `SurfaceKind → Handle<AudioSource>`, la cota espacial de
   25 m y el presupuesto de voces.
2. **Decidir qué es la niebla.** `FOG_MAX_ALPHA = 0.3` es un techo duro sobre la
   mezcla: a cualquier distancia el terreno lejano sigue siendo 70% él mismo, o
   sea que la niebla **no puede** cerrar el horizonte, aunque dos documentos lo
   prometían. Se elige: o el alfa llega a 1.0 y `start`/`end` se ajustan para
   que el juego cercano siga limpio, o se acepta que es atmósfera y no LOD y se
   borra la promesa.

*(`AUDIO.md` Fase 2 · `LIGHTING.md` Fase 3 · `TEXTURES.md` Paso 11.)*

**Criterio de aceptación**

Caminar por terrenos distintos y escuchar el cambio, con una lectura de CPU con
y sin audio. Y, desde el punto más alto que se pueda esculpir, o el terreno se
funde con el cielo o está escrito que no es el objetivo.

---

## Lo que deliberadamente no está en esta lista

- **Billboards, cartas de grupo y shell texturing** para el pasto: descartados
  con cuentas en `BOTWGrass.md`, no por costo de implementación.
- **Vertex pulling**: descartado por aritmética, con un disparador falsable
  escrito para reabrirlo.
- **Volumetric fog, SSAO, toon shading, cubemap de cielo**: cada uno tiene su
  motivo escrito en el documento dueño.
- **Todo lo que dependa de arte final.** Estos MVP están ordenados para que
  ninguno quede bloqueado esperando un asset: los que tocan arte usan lo que ya
  hay, o placeholders propios.
