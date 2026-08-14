# Datos de rendimiento — pradera

Números crudos, repetidos, con su contexto exacto — para no remedir lo que ya
se midió. Narrativa y decisiones en `AHORA.md`/`BOTWGrass.md`; esto es sólo la
tabla. Cada tanda lleva fecha, máquina y comando exactos; nada entra sin ellos.

## 2026-08-13 — `BOF_BENCH=grass`, solo púa (1 triángulo), tres corridas

**Comando:** `BOF_SCENE=Pasto BOF_BENCH=grass BOF_KNOBS=grass-shape=2 cargo run`,
tres veces seguidas, sin cerrar ni reabrir nada entre medio.

**Contexto** (idéntico en las tres corridas, confirmado línea por línea):
- Escena **Pasto**, perfil **desktop**, mirador `pos=(4.5,4.3,-36.2)
  facing=(-0.30,-0.28,-0.91)` (la F7 real de la pradera).
- **Ventana 2560×1440**, `render=100%` (baseline de la suite — ver nota en
  `AHORA.md` sobre qué declara este campo), **MSAA 2×** de baseline.
- Pasto: densidad base 40/m², alcance 100%. Sombras: mapa 1024, rango 60 m,
  hoja=off.
- En cuadro: pradera 96% de mallas / 96% de triángulos (3 draws~), terreno 4%/4%
  (1 draw~). Total 25 mallas, 4 draws~.
- **Máquina despejada**, confirmado con el usuario antes de la primera corrida
  (nada más usando la GPU).

### GPU, ms — las tres corridas y su rango

| paso | corrida 1 | corrida 2 | corrida 3 | media | rango |
|---|---:|---:|---:|---:|---:|
| baseline | 15.24 | 12.30 | 15.14 | 14.23 | 12.30–15.24 |
| grass off | 6.13 | 6.13 | 6.12 | **6.13** | 6.12–6.13 |
| grass 64/m² | 19.07 | 16.11 | 19.05 | 18.08 | 16.11–19.07 |
| grass 30/m² | 13.32 | 10.72 | 13.30 | 12.45 | 10.72–13.32 |
| grass 12/m² | 9.81 | 8.01 | 9.81 | 9.21 | 8.01–9.81 |
| reach 75% | 14.92 | 12.33 | 14.91 | 14.05 | 12.33–14.92 |
| reach 50% | 13.96 | 11.68 | 13.84 | 13.16 | 11.68–13.96 |
| render 50% | 7.14 | 6.40 | 7.13 | 6.89 | 6.40–7.14 |
| msaa 4x | 16.09 | 16.10 | 16.09 | **16.09** | 16.09–16.10 |
| msaa off | 9.06 | 9.06 | 9.05 | **9.06** | 9.05–9.06 |
| baseline repeat | 15.28 | 12.32 | 15.17 | 14.26 | 12.32–15.28 |

**Las tres corridas fueron válidas (11/11 pasos) y cada una determinista contra
sí misma** — la deriva entre sus dos baselines internos fue +0.04, +0.37 y
+0.04 ms respectivamente, muy por debajo de cualquier delta que importe.

**Pero la corrida 2 corrió sistemáticamente ~2–3 ms más rápida en los pasos con
densidad de pasto — y no es ruido de instrumento, es real y no explicado.**
`grass off`, `msaa 4x` y `msaa off` dan el mismo número en las tres corridas
(diferencias de 0.01 ms), pero `baseline`, `grass 64/64/m²`, `grass 30/m²`,
`reach 75%` y `reach 50%` — todos con pasto denso en cuadro — corren ~2–3 ms
más rápido sólo en la corrida 2, y para el paso `render 50%` (más adelante en
la misma corrida) ya volvió a estar en línea. **No se investigó la causa**
—candidatos razonables son boost de reloj de la GPU o temperatura, no algo que
el proyecto controle—, así que el rango de la tabla es el dato honesto, no la
media sola. Antes de tomar una decisión que dependa de un número exacto de
esta fila, conviene una cuarta corrida.

**Lo que sostiene, con las tres corridas:**
- **El pasto sigue costando ~60% del frame GPU incluso con la forma más
  barata que existe** (1 triángulo, sin la mitad muerta): `grass off` ahorra
  6,13 ms fijo contra un baseline de 12,3–15,2 ms.
- **`render 50%` sigue siendo la palanca más grande**: −6,4 a −8,1 ms según la
  corrida, comparable a sacar la pradera entera.
- **MSAA apagado ahorra 6,05–6,19 ms**, el número más estable de toda la
  tabla — casi tanto como toda la pradera puesta a 12/m². Subir a 4x sólo
  cuesta 0,84–3,80 ms extra sobre el 2x de baseline (este último rango sí es
  ancho, ligado a la misma anomalía de la corrida 2).
- **El alcance rinde poco** comparado con densidad o resolución: `reach 50%`
  ahorra 0,62–1,31 ms contra los 4,29–5,43 ms de `grass 12/m²`.

## 2026-08-13 — `BOF_SHOT=grass`, vitalidad de la pradera (dos corridas)

**Comando:** `BOF_SCENE=Pasto BOF_SHOT=grass BOF_KNOBS=grass-shape=2 cargo run`,
dos veces. Mismo mirador y ventana que arriba.

**Resultado, idéntico byte a byte en las dos corridas** — el censo es puro
CPU/determinista, sin el piso de ruido del viento que tiene el conteo de
píxeles:

**Corrección de unidad (2026-08-13, tarde):** las columnas son **briznas**
(instancias del registro de 16 B), no triángulos — `resident_blades`/
`alive_blades` en `GrassVitality`. Bajo "solo púa" coinciden con triángulos
porque ahí `submitted_triangles_per_blade` vale 1; en `auto` (2 triángulos por
brizna) no coinciden, y el número de abajo lo confirma.

| anillo | briznas residentes | briznas "vivas" (no degeneradas) | % vivo |
|---|---:|---:|---:|
| 0 (cerca) | 1.107.648 | 91.422 | **8,3%** |
| 1 (medio) | 159.744 | 77.954 | 48,8% |
| 2 (lejos) | 1.083.392 | 247.260 | 22,8% |

**El anillo 0 desperdicia 91,7% de lo que envía.** De 1,1 millones de briznas
residentes, sólo 91 mil producen algo visible — el resto son briznas fuera de
su alcance individual (colapsadas a altura cero por `blade_growth`) que igual
pagaron una invocación completa del vertex shader, exactamente la hipótesis de
"cull antes del shader" que motivó construir este instrumento.

### Repetido en `auto` (formas mezcladas, 2026-08-13, `BOF_SHOT=grass` sin banco)

Mismo mirador y ventana. Comparación directa:

| anillo | briznas residentes | vivas, solo púa | vivas, auto | % vivo, auto |
|---|---:|---:|---:|---:|
| 0 (cerca) | 1.107.648 | 91.422 (8,3%) | **91.422** | **8,3%** |
| 1 (medio) | 159.744 | 77.954 (48,8%) | 69.263 | 43,4% |
| 2 (lejos) | 1.083.392 | 247.260 (22,8%) | 65.765 | **6,1%** |

**El anillo 0 —el que motiva esta investigación— da exactamente el mismo
número en juego normal: 8,3%, briznas idénticas hasta la última cifra.** No es
casualidad: dentro de los 24 m del anillo 0 la forma nunca llega a ser carta
(el umbral está más allá de los 24 m), y hoja/púa comparten la misma fórmula
de huella (`BladeShape::footprint_m`), así que la ley de densidad da la misma
escalera de alcances con cualquiera de las dos. **La conclusión ya no está
pendiente: 91,7% de desperdicio en el anillo 0 es el número real, medido en
juego normal, no un artefacto del banco.**

El anillo 2 empeora bastante en `auto` (22,8%→6,1%) — ahí sí entra la carta,
con una huella distinta, y ese anillo no fue el que motivó la investigación
del anillo 0. Queda anotado, no analizado todavía.

## Escalonado del anillo 0 en tiers de buffer (2026-08-13, implementado)

Con el desperdicio confirmado real (arriba), se implementó el escalonado:
`chunk_m=12` **sin cambios** (evita el riesgo de presupuesto de draws que
encontró la crítica con `chunk_m=8`), **3 tiers internos** por punto más
cercano del chunk al foco, con histéresis (`KEEP_SLACK_M`) para no rehornear
en cada cuadro un chunk en el borde de un tier. Detalle de diseño y los dos
bugs que encontraron dos rondas de crítica antes de escribir código
(materiales no compartibles entre tiers; límites de tier como fracción del
alcance vigente, no metros fijos, por `AdjustFrontier`) en el historial de
`iterate-safely` de la sesión.

**Medido en vivo, `BOF_SHOT=grass` con solo púa, mismo mirador que arriba:**

| | antes (1 tier) | después (3 tiers) |
|---|---:|---:|
| triángulos residentes, anillo 0 | 1.107.648 | **357.552** |
| vivos | 91.422 | 85.253 |
| % vivo | 8,3% | **23,8%** |
| desperdicio | 91,7% | **76,2%** |
| chunks | 24 | 21 |

**3,1× menos triángulos residentes en el anillo 0**, cerca de lo que predijo
la corrida exploratoria (2,7×, con un solo alineamiento de grilla y sin el
margen de histéresis real). El desperdicio no se fue a cero —era esperado,
la crítica ya había mostrado que un chunk sólo puede ser tan chico como su
punto más cercano exige— pero se cortó a un tercio.

Reparto por tier de esa corrida: tier 0 (más cerca) 6 chunks/276.912 tris,
tier 1: 7 chunks/48.384 tris, tier 2 (más lejos): 8 chunks/32.256 tris — el
tier caro sigue siendo caro porque ahí es donde el jugador puede estar parado
encima de cualquier brizna del anillo; los dos tiers lejanos, que son la
mayoría de los chunks por área, pagan una fracción.

Validado: `cargo fmt`/`clippy -D warnings`/los tres suites en verde
(202+15+265+53, cinco tests nuevos sobre el invariante de cobertura, la
histéresis y el seguimiento del alcance vigente). F9 y `BOF_SHOT` muestran el
desglose por tier del anillo 0 además del total por anillo de siempre.

### Primer bug jugado: cuadrados enteros desaparecían al cruzar un tier

El usuario jugó apenas se cerró esto y encontró un bug real: caminando,
cuadrados enteros de pasto desaparecían un momento antes de volver. Causa,
confirmada leyendo el código: un chunk que cambia de tier se **suelta el
mismo cuadro** (ya no coincide con `keep_set`) pero sólo se **rehornea
cuando le toca el turno**, `CHUNKS_BAKED_PER_FRAME = 1` — el mismo límite que
alcanzaba para las fronteras de anillo (raras, anchas) pero no para las de
tier (angostas, tres veces más frecuentes en el mismo territorio). Varios
chunks cruzando de tier juntos —típico caminando en línea recta, porque la
frontera de un tier es un círculo que se mueve entero con la cámara—
desaparecían todos ese cuadro y volvían de a uno.

**Arreglado**: una celda que se suelta porque cambió de tier (no porque salió
de la grilla) entra a hornear **sin esperar el presupuesto**, ese mismo
cuadro — hornear es "casi nulo" de costo, así que no hay razón para
racionarlo ahí. Sólo los chunks genuinamente nuevos siguen esperando su
turno. Validado en frío (fmt/clippy/tests); falta la confirmación jugada de
que el síntoma se fue.

### Corrección: 3 tiers no era el techo — es 6

El usuario dudó de la conclusión de 3 tiers, y tenía razón en dudar: la
cuenta exploratoria que decidió ese número usaba un solo alineamiento de
grilla, sin la histéresis real. Medido con el sistema real (`BOF_SHOT=grass`,
mismo mirador, `RING0_TIERS` cambiado y remedido cada vez):

| tiers | residentes anillo 0 | vivos | % vivo |
|---:|---:|---:|---:|
| 1 (antes de todo esto) | 1.107.648 | 91.422 | 8,3% |
| 3 | 357.552 | 85.253 | 23,8% |
| 4 | 352.476 | 85.258 | 24,2% |
| **6** | **284.616** | 85.258 | **30,0%** |
| 8 | 285.264 | 85.246 | 29,9% (y un tier vacío: 0 chunks) |

**El techo real está en 6, no en 3** — 4 casi no mejora sobre 3, pero 6 sí
mejora bastante sobre 4, y 8 no mejora nada sobre 6 (y encima desperdicia un
tier entero sin rango, confirmado con el propio guardrail de test). El
sistema quedó en **`RING0_TIERS = 6`**. Lección: la cuenta exploratoria sirve
para decidir la dirección antes de escribir código, no para fijar el número
final — una vez que el sistema real existe, remedir con él es más barato que
seguir confiando en la aproximación.

## Ancho global doblado — experimento de validación, revertido (2026-08-13, noche)

**Medido, no shippeado.** Se dobló `blade_width_m` (0,057 → 0,114 m) como
único cambio, sin tocar densidad a mano — la ley de cobertura ya está escrita
"por metro de ancho" (`hidden_per_width_per_metre`), así que reacciona sola.
`BOF_SHOT=grass`, vista `medir`, mismo mirador:

| | ancho normal (0,057 m) | ancho doble (0,114 m) |
|---|---:|---:|
| triángulos anillo 0 | 284.616 | 142.272 |
| triángulos anillo 1 | 159.744 | 79.872 |
| triángulos anillo 2 | 1.083.392 | 541.696 |
| **total pradera** | **1.527.752** | **763.840** |
| cobertura 4-6 m | 77,7% | 80,6% |
| cobertura 11-16 m | 86,8% | 87,8% |

**Exactamente la mitad de los triángulos, cobertura igual o mejor.** Confirma
que la ley de densidad responde correctamente al ancho — el mecanismo
funciona. El usuario lo jugó (ancho global, sin gradiente) sin objetar el
aspecto, pero tampoco lo aprobó explícitamente antes de pasar a otros temas.

**Revertido a 0,057 m** — quedó sin commitear ni documentado un rato, lo cual
violaba la propia regla del proyecto ("ningún número entra sin salir del
medidor" incluye no dejarlo suelto sin anotar). El plan real es **gradual por
distancia** (delgada cerca, ancha lejos), no un doblado plano — ver abajo.

## Ancho gradual por distancia — planeado, NO ejecutado (2026-08-13, noche)

Pedido explícito del usuario, con `/iterate-safely`. El plan (multiplicar un
nuevo factor `width_scale_at(distancia)` en la base de hoja/púa, y el mismo
factor en `minimum_density`) pasó por una crítica que encontró **dos bugs de
corrección reales antes de escribir código**:

1. **Regresión de cobertura silenciosa en la carta.** `minimum_density` se
   evalúa para cualquier forma, incluida Carta — sin gatear el factor nuevo
   por forma, la densidad de la banda de carta bajaría hasta 2,5× mientras el
   plan decía explícitamente "no tocar la carta" en el shader. Cobertura real
   perdida, sin que nada lo avisara.
2. **Escalar `waist` corrompe la altura, no el ancho.** `waist` es una
   posición a lo largo del **alto** de la hoja (con inclinación), no un
   vector de ancho — multiplicarla por el factor nuevo movería la cintura de
   altura sólo por estar lejos. El ancho real sale únicamente de `across`
   (`grass.wgsl:586`), que ya es la base compartida de hoja y púa — escalar
   sólo ahí alcanza y no toca `waist`.

Más lo que encontró la crítica sin ser bugs de corrección pero sí riesgo real:
agregar campos a `GrassUniform` (bindeado, sin ningún test que verifique que
Rust y WGSL declaran los campos en el mismo orden — el mismo tipo de bug
silencioso que ya costó una sesión completa con `sun_direction`/`record.w`
antes) es más riesgoso que reusar los `.w` de `sun_color`/`ambient_color`, ya
documentados como libres en los dos lados. Y `grass-view=subpixel` (que ya
compensa `spike_growth_scale` para no subestimar píxeles) necesitaría
enterarse del factor nuevo o empezaría a mentir en la dirección de "parece
que sobra más desperdicio del real".

**No se ejecutó esta noche a propósito** — con dos bugs de corrección
encontrados en la sola etapa de plan, y encima montado sobre el escalonado de
tiers de hoy (todavía sin confirmar jugado), el riesgo de que algo se vea
raro mañana sin poder saber si la causa es esto o lo de hoy era demasiado
alto para tocarlo sin que el usuario pueda mirar la pantalla. Queda el plan
corregido, listo para retomar: gatear el factor a `shape != Card`, escalar
sólo `across`, reusar `sun_color.w`/`ambient_color.w` en vez de campos
nuevos, y avisar en `grass-view=subpixel`.

## Chunks apareciendo de golpe en el anillo lejano — diagnosticado y medido (2026-08-13, noche)

Jugando, el usuario reportó un cuadrado de pasto apareciendo de la nada con
un movimiento mínimo de cámara, cerca del horizonte. Confirmado con capturas
F7 reales comparadas con `compare -fuzz 25%` de ImageMagick (filtra el ruido
de paralaje y sólo deja los cambios grandes): puntas de pasto aparecen en el
mismo punto del horizonte entre tres fotos casi idénticas.

**Causa, confirmada con números reales, no supuesta**: `growth_ramp`
(perilla `grass-growth`, default 6 m) suaviza la muerte de **una brizna
individual** cerca de **su propio** alcance — no tiene ninguna noción de "este
chunk se acaba de hornear". El anillo 2 tiene `chunk_m = 64` m: cuando un
chunk de esa escala aparece (presupuesto de horneado, 1 por cuadro), la
mayoría de sus briznas no están cerca de su propio límite individual, así que
aparecen a altura completa de una — el crecimiento nunca las toca.

### Control de F9 (implementado)

Se agregó un control en F9 ("Rampa de crecimiento", ±2 m por click) para
mover `growth_ramp` en vivo. **Deliberadamente no vive en
`GrassRendererSettings`** — ese struct se compara por valor en
`MeadowRebuildDials`, así que cualquier campo ahí dispara un rehorneado
completo de la grilla; `GrowthRampOverride` es un recurso aparte que
`growth_band` consulta, y no toca `roll_meadow_grid` para nada — el mismo
número que ya viajaba gratis por uniform cada cuadro sigue viajando gratis.
Gana sobre la perilla F1 cuando está puesto, con el mismo aviso `⚠` que ya
usa el conflicto `grass-shape`/`grass-card`; "Restaurar baseline" lo apaga
(vuelve a `None`, no a un número fijo) y el mando vuelve a F1.

### Medido: agrandar la rampa NO es el arreglo — es un trade-off parejo, no localizado

Barrido con `BOF_SHOT=grass`, vista `medir`, mismo mirador, sólo cambiando
`growth_ramp`:

| rampa | cobertura total | 4-6 m (cerca) | 45-64 m (lejos, anillo 2) |
|---:|---:|---:|---:|
| 6 m (hoy) | 56,06% | 63,0% | 49,4% |
| 10 m | 51,54% | 54,1% | 54,7% |
| 20 m | 42,49% | 42,1% | 50,0% |

**La cobertura baja en todas las bandas al agrandar la rampa, no sólo lejos
— incluida la banda más cercana a la cámara, donde el problema original ni
siquiera existe.** La razón, entendida después de medir: la mayoría de los
índices de brizna tienen un alcance individual corto (son la "gente" que
llena la densidad, no los pocos de índice bajo que llegan lejos) — con una
rampa de 20 m, esas briznas pasan **la mayor parte de su vida** dentro de su
propia zona de desvanecimiento, nunca llegan a altura completa. No es un
arreglo localizado al borde: es un impuesto parejo sobre toda la pradera.
**No se encontró un valor que mejore el borde sin empeorar el resto** en este
barrido de tres puntos.

**Conclusión, honesta:** el control de F9 queda — sirve para explorar esto en
vivo — pero no hay una recomendación de valor nuevo. El arreglo real
probablemente necesita algo más quirúrgico (achicar `chunk_m` del anillo 2
específicamente, para que menos briznas aparezcan de golpe por horneado —
**no probado esta noche**, es el próximo experimento) o el mecanismo de
"tiempo desde que este chunk nació" que se descartó por complejo al empezar
esta sesión de trabajo. El default queda en 6 m, sin tocar.

## Borde de tier con ruido — Técnica 2, implementada y apagada por default (2026-08-13, noche)

Aclaración del usuario: la "ameba" de antes no era una forma orgánica
literal — el concepto es **ruido en el límite**, no una figura nueva. Es
exactamente la Técnica 2 del plan de anillos, anotada desde 2026-08-09 y
nunca hecha: romper el círculo perfecto de una frontera de LOD con ruido
determinista, no una forma distinta.

**Implementado con la misma disciplina de seguridad que ya costó un bug
real esta sesión**: el ruido (`tier_boundary_jitter_m`, hash determinista por
celda) **sólo puede ser `<= 0`** — empuja a un chunk a verse *más cerca* de
lo real, nunca más lejos. Así, como mucho pide un tier con más rango del
necesario (desperdicio), nunca uno con menos (agujero de pasto). Un test
(`ragged_boundary_never_assigns_a_smaller_tier_than_the_clean_one`) barre
900 celdas y confirma que el tier con ruido nunca es mayor que el tier sin
ruido, para cualquier celda.

Control en F9 ("Borde de tier: ruido on/off"), **apagado por default** —
a diferencia de la rampa de crecimiento, esto sí vive en
`GrassRendererSettings` porque cambia a qué tier pertenece cada chunk, así
que el rehorneado completo que dispara es necesario, no evitable.

**Validado en frío**: con el toggle apagado, `BOF_SHOT=grass` da
exactamente los mismos números que antes de este cambio (284.616 tris,
30,0% vivo, byte a byte) — cero regresión. `cargo fmt`/`clippy -D
warnings`/los tres suites en verde (206+15+265+53, dos tests nuevos).

**No verificado jugando ni con captura**: si el ruido efectivamente rompe la
lectura circular de una frontera es una pregunta visual, no numérica — la
misma razón por la que esto no se apagó/prendió y comparó con capturas
esta noche. `ragged_tier_boundary_max_m` (default 4,0 m) es un valor a ojo,
sin medir contra nada.

## Palancas ya medidas, aparcadas (2026-08-13)

No se abandonan — se anotan para volver si el ataque al desperdicio de
triángulos no alcanza solo. Todas actúan sobre el frame entero, no sobre la
causa (triángulos que no debían enviarse):

- **`render 50%`**: −6,4 a −8,1 ms, la palanca más grande medida hasta hoy.
- **MSAA off**: −6,05 a −6,19 ms, muy estable entre corridas.
- **Reach 50%/75%**: rinde poco (−0,6 a −1,3 ms) comparado con densidad.

## Próximo dato que falta

- **Achicar `chunk_m` del anillo 2** (hoy 64 m) como ataque más quirúrgico al
  pop de chunk — no probado esta noche, es lo que sigue.

- **Jugar la caja Pasto con el escalonado nuevo** — confirmar que ningún borde
  de tier dejó un parche de pasto faltante, y que cruzar un tier caminando no
  se nota. Nada de lo de arriba lo reemplaza.
- Un `BOF_BENCH=grass` fresco (post-escalonado) para saber cuánto de esos
  3,1× menos triángulos se traduce en ms — no medido todavía, sólo el conteo.
- Una cuarta corrida de `BOF_BENCH=grass` (del baseline sin tiers) para
  decidir si la anomalía de la corrida 2 era reloj de la GPU o algo real —
  quedó sin cerrar.
- `BOF_BENCH=general` repetido igual de 3 veces — hoy sólo hay una corrida
  suelta, sin rango.

## El ruido de borde estaba en el límite equivocado — y auditar encontró un bug real (2026-08-13, madrugada)

**El ruido de tier no podía verse, por diseño.** Jugando, el usuario reportó
que el botón "Borde de tier: ruido on/off" de F9 no cambiaba nada, y que
seguían viéndose "cuadrados" aislando un anillo en modo normal. Investigando:
`tier_boundary_jitter_m` perturba a qué **tier interno de ring0** pertenece
un chunk — una partición de buffer diseñada a propósito para ser invisible
(mismo resultado visual, sólo cambia cuántos índices se someten al vertex
shader). El límite que sí se ve como una línea/círculo al jugar es la
**transición entre anillos** (0→1→2, donde cambian densidad y forma de
golpe), que el ruido nunca tocó. Confirmado con `BOF_SHOT`, cámara fija,
3 capturas (`ring_1.png`, `ring_2.png`, `ring_3.png`, cada anillo aislado en
modo normal, sin ningún tinte de diagnóstico): las tres muestran un campo
continuo, sin costuras — ningún cuadrado persistente en estado asentado.
Conclusión: la Técnica 2 quedó aplicada al lugar equivocado; el trabajo de
moverla a la frontera real de anillos queda pendiente.

**El audit que se pidió en su lugar encontró un bug real, distinto.** Antes
de tocar la frontera de anillos, se auditó todo el sistema (`grass.rs`,
`grass_tiles.rs`, `grass.wgsl`) con un subagente sin contexto previo. Halló
que `ring0_tier_with_hysteresis` (grass.rs) retiene un chunk ya horneado en
su tier hasta que su punto más cercano baja a `bounds[tier] - KEEP_SLACK_M`
(3 m) — pero `tile_ranges` presupuestaba el stride de ese tier para
`bounds[tier]` a secas, el caso de la asignación **limpia**, no el de la
retenida. Como la escalera de alcance es no-creciente en la distancia, un
chunk retenido hasta 3 m más cerca de lo que su presupuesto contempla se
queda sin las briznas de su borde real — un agujero de densidad real,
reproducible en juego normal (sin necesitar el ruido) cada vez que la
cámara se acerca a una frontera de tier. Es probablemente lo que se veía
como "cuadrados" jugando, más que el bug de anillo lejano ya documentado
arriba.

Ningún test existente lo cubría: `no_tier_boundary_strands_a_blade_...`
probaba la asignación limpia (`bounds[tier]`), nunca el peor caso que la
histéresis realmente permite.

**Arreglo:** `tile_ranges` ahora presupuesta cada tier (salvo el tier 0, que
no tiene lado retenido) para `bounds[tier] - KEEP_SLACK_M`, el peor caso
real que la histéresis deja vivo, en vez de `bounds[tier]`. No se tocó
`ring0_tier_with_hysteresis` — la histéresis en sí sigue igual, sólo el
presupuesto que la contempla. Costo: un poco más de triángulos residentes
por tier (sobre-provisión del ancho de `KEEP_SLACK_M`), aceptable frente a
dejar briznas afuera.

Test nuevo: `no_tier_boundary_strands_a_blade_retained_by_hysteresis` —
mismo invariante que el test viejo, pero contra el peor caso retenido, no
el limpio. `cargo fmt`/`clippy -D warnings`/los cuatro suites en verde
(207+15+265+53).

**No verificado jugando todavía** — el arreglo es sobre un caso que sólo se
manifiesta en movimiento (acercándose a una frontera de tier con un chunk ya
horneado), así que una captura estática no lo prueba ni lo desprueba. Falta
que el usuario camine cerca de una frontera de tier y confirme que el parche
ralo desapareció.

**Pendiente, sin cambiar:** mover el ruido de borde a la frontera real de
anillos sigue abierto — es un cambio más grande (cambia densidad y forma al
cruzar, no sólo triángulos desperdiciados) y se dejó para después del audit.

## El ruido se movió a la frontera real de anillos (2026-08-13, madrugada, segunda vuelta)

Con el bug de histéresis arreglado, quedaba mover Técnica 2 al lugar
correcto (pedido explícito del usuario: "arregla todo lo que encontraste en
el audit, más los errores que conocías, como el de la frontera de ruido").
`/iterate-safely` con crítica de un subagente sin contexto previo antes de
tocar código — encontró tres problemas reales en el plan original:

1. **El anillo 2 no puede recibir el mismo ruido.** Más allá de
   `farthest_reach()` (128 m) no vive ninguna brizna de toda la pradera —
   `grass_tiles::reach_ladder` corta ahí, un tope duro. Un chunk seleccionado
   sólo por el ruido en el borde exterior del anillo 2 saldría con cero
   briznas vivas: puro gasto de draw call y memoria contra un presupuesto
   móvil ya ajustado, sin romper nada visualmente. Arreglo: el ruido sólo se
   aplica a los anillos `0..GRASS_RING_COUNT-1` — el último nunca recibe
   ruido, sin importar la configuración (`ring_boundary_jitter_cap_m`
   devuelve 0 ahí siempre).
2. **Un string de la UI habría quedado mintiendo.** El readout de F9 tenía
   una segunda mención de "borde de tier" (aparte del texto del botón) que
   el plan original no iba a tocar — exactamente la clase de confusión que
   costó la sesión de esta madrugada. Corregido.
3. **`span` no debía ensancharse sin gatear.** Sumar el máximo del ruido al
   radio de búsqueda de celdas sin chequear si el ruido está prendido habría
   sido una regresión de CPU chica pero real y silenciosa en el estado por
   default (apagado). Arreglo: `span` y el jitter comparten la misma función
   de tope (`ring_boundary_jitter_cap_m`), que da 0 cuando está apagado.

**Diseño final:** `ring_boundary_jitter_m(cell, ring, settings)` — signo
opuesto al de los tiers (siempre `>= 0`, nunca `<= 0`), porque perturba
`reach_m` (el radio real que decide si un chunk existe), no un límite
interno de buffer. Sólo toca el corte **exterior** de `ring_cells_with_slack`
— el `handover` (corte interior de la corona siguiente) sigue anclado al
alcance limpio del anillo de adentro, que nunca cubre menos que ese valor
con o sin ruido, así que excluir hasta ahí sigue siendo seguro (en el peor
caso hay superposición extra, nunca un agujero — verificado por la crítica
contra el código real). Reutiliza y renombra los campos existentes
(`ragged_tier_boundary_*` → `ragged_ring_boundary_*`) y el mismo botón de
F9 en vez de agregar uno nuevo al lado — el mecanismo viejo (ruido en el
límite interno de tiers, invisible por diseño) se eliminó por completo, no
quedó código muerto.

Tests nuevos: `ring_boundary_jitter_is_zero_when_disabled_and_never_negative_when_on`,
`ring_boundary_jitter_never_touches_the_outermost_ring`,
`ring_boundary_jitter_never_shrinks_the_selected_cells` (compara
`ring_cells_with_slack` con y sin ruido, confirma superconjunto),
`ring0_tier_saturates_safely_past_its_own_clean_edge_when_the_ring_boundary_is_jittered`.
`cargo fmt`/`clippy -D warnings`/los cuatro suites en verde (209+15+265+53).

**Apagado por default es un no-op algebraico, no sólo "probablemente
igual"**: con `ragged_ring_boundary_enabled=false`,
`ring_boundary_jitter_cap_m` devuelve 0 antes de calcular nada, así que
`span` y `reach_m` se reducen a la fórmula exacta de antes del cambio — no
hizo falta remedir con `BOF_SHOT` para confirmarlo, es una garantía de
código, cubierta además por el primer test de la lista de arriba.

**No verificado jugando ni con captura**: si el ruido activado realmente
rompe la lectura circular de la frontera 0→1/1→2 es una pregunta visual —
la misma razón por la que esto no se comparó con capturas antes/después
esta madrugada. `ragged_ring_boundary_max_m` (default 4,0 m, heredado del
mecanismo viejo) sigue siendo un valor a ojo, sin medir contra nada.

## Tercera auditoría, sin hallazgos nuevos (2026-08-13, tercera vuelta)

Tras cerrar los dos fixes de arriba (histéresis + reubicación del ruido),
se corrió un tercer ciclo de `/iterate-safely`: un subagente sin contexto
previo de la sesión releyó `grass.rs` completo (3341 líneas, no sólo el
diff), `grass_tiles.rs`, `grass_records.rs`, las partes relevantes del
shader, y los diffs de `grass_debug.rs`/`mod.rs`/`grass_lab.rs`/`shot.rs`.
Buscando específicamente: fallas lógicas, duplicación, referencias
huérfanas al rename "tier boundary" → "ring boundary", y bugs que un
refactor de esta rama haya introducido sobre otro.

**Resultado**: rederivó desde cero (sin apoyarse en este documento) el
invariante que arregló la primera vuelta —el presupuesto de `tile_ranges`
bajo histéresis— y lo confirmó sólido. No encontró ninguna falla lógica
nueva, ninguna referencia huérfana al nombre viejo, ni duplicación. Todo
lo cableado en `mod.rs`/`grass_lab.rs`/`shot.rs` para el ciclo anterior
(`GrowthRampOverride`, `RaggedRingBoundary`, el orden de argumentos a
`grass_vitality`) resultó correcto.

**Un solo hallazgo, de cobertura, no de lógica**: el test
`no_patch_of_ground_is_planted_by_more_than_two_rings` (que garantiza que
ningún punto de suelo lo planten más de `RINGS_OVER_THE_SAME_GROUND` = 3
anillos) sólo corría con `GrassRendererSettings::default()`, es decir con
el ruido de frontera **apagado**. Nunca se había verificado ese invariante
con el toggle de F9 encendido — la configuración donde el ruido extiende
el borde exterior de un anillo hasta `ragged_ring_boundary_max_m` (4,0 m
default), un valor que coincide con el gap mínimo que `AdjustFrontier`
fuerza entre anillos vecinos.

Se extrajo la lógica común a `worst_rings_over_ground(&settings)` y se
agregó `no_patch_of_ground_is_planted_by_more_than_two_rings_with_ragged_boundary_on`,
idéntica salvo `ragged_ring_boundary_enabled = true`. **Pasa**: confirma
empíricamente (no sólo por el razonamiento a mano de que `handover` sigue
anclado al alcance limpio del anillo de adentro) que el ruido no abre un
cuarto anillo sobre el mismo suelo.

Validado: `cargo fmt -- --check` limpio, `cargo clippy --all-targets -- -D
warnings` limpio, los cuatro suites en verde: 210 (+1 sobre la vuelta
anterior) + 15 + 265 + 53.

**Veredicto de la auditoría**: sólido, sin nada estructural pendiente.
Sigue sin verificarse jugando (ver arriba).

## `chunk_m` del anillo 2, 64 → 48 — mitiga el pop, no lo elimina (2026-08-13, madrugada)

Jugando con un anillo aislado, el usuario reportó el pop de chunk cuadrado
ya diagnosticado ("Chunks apareciendo de golpe en el anillo lejano", más
arriba) y señaló que la rampa de crecimiento no lo tapaba — exactamente lo
que esa sección ya había medido: `growth_ramp` no tiene noción de "este
chunk se acaba de hornear", así que no ayuda acá.

**El experimento pendiente que esa sección dejaba anotado** — achicar
`chunk_m` del anillo 2 — se probó por fin. Antes de tocar el default se
sondearon candidatos con el mismo método que usa
`the_neighbourhood_is_bounded` (peor alineación posible, barrido 8×8):

| `chunk_m` | peor caso (todos los anillos) | presupuesto |
|---:|---:|---:|
| 64 (hoy) | 78 | 100 |
| 56 | 84 | 100 |
| 48 | 90 | 100 |
| 44 | 92 | 100 |
| 42 | 96 | 100 |
| 40 | **104 — rompe** | 100 |
| 32 (4× menos briznas por pop) | **120 — rompe** | 100 |

**32, el valor que hubiera dado la reducción "4× menos briznas por pop"
buscada, rompe el presupuesto de draws del peor caso.** No es un riesgo
teórico — es el mismo tipo de límite que ya frenó `chunk_m=8` en el anillo
0 (`AHORA.md`, "casi agotaba `MOBILE_DRAWS`, 99/100"). El margen real
empieza a apretarse en 44 y se pierde en 40.

**Elegido: 48m** — deja margen comparable al de hoy (90/100, contra 78/100
actual) y reduce el pop por chunk nuevo en `(48/64)² ≈ 44%` menos briznas
de golpe, no el 75% que hubiera dado 32.

**Medido antes/después** (`BOF_SHOT=grass`, `grass-view=medir`,
`grass-rings=solo 2`, mismo mirador):

| | chunk_m=64 | chunk_m=48 |
|---|---:|---:|
| chunks residentes | 23 | 33 |
| triángulos residentes | 2.166.784 | 1.748.736 |
| triángulos en frustum | 753.664 | 688.896 |
| vitalidad (vivas/residentes) | 6,1% | 7,5% |

Efecto colateral no buscado pero medido: bajaron **también** los
triángulos totales y subió la vitalidad — un chunk más chico calza mejor
contra el corte circular real (`reach_m`), así que se desperdicia menos
esquina de chunk fuera del círculo. Comparando las dos capturas
(`ring2_before.png`/`ring2_after.png`) lado a lado, la composición final se
ve idéntica — esperable: el pop es un efecto de **movimiento**, una toma
estática después de asentar no lo muestra ni lo desmiente.

Validado: `cargo fmt -- --check` limpio, `cargo clippy --all-targets -- -D
warnings` limpio, los cuatro suites en verde (210+15+265+53, sin tests
nuevos — `the_neighbourhood_is_bounded` ya cubre dinámicamente el
presupuesto contra el nuevo default).

**Honesto sobre el alcance**: esto mitiga, no elimina. El mecanismo de
fondo (una brizna recién horneada que no está cerca de su propio límite de
alcance no tiene por qué desvanecerse) sigue intacto — sólo hace el pop
más chico, no lo apaga. Si jugando sigue notándose, el próximo paso es el
mecanismo de "tiempo desde que este chunk nació" que la sesión anterior
descartó por complejo — ese sí ataca la causa en vez de acotar el síntoma.
**No verificado jugando todavía.**

### Incidente de esta madrugada: un subagente de crítica borró trabajo sin commitear

Durante el ciclo `/iterate-safely` de este experimento, el subagente que
revisó el plan agregó sus propios tests de sondeo a `grass.rs` y los
revirtió con `git checkout -- src/visuals/grass.rs` para "no dejar nada".
Ese comando no revierte sólo los cambios propios: resetea el archivo
entero al último commit — que en este caso predataba toda esta sesión y la
anterior (el sistema de anillos, histéresis, ruido de frontera). El
diagnóstico llegó al notar que `cargo test` fallaba con símbolos
inexistentes (`GrowthRampOverride`, `grass_vitality`, `BladeShape`
público) que otros archivos sí seguían importando — es decir, `grass.rs`
había vuelto a un estado más viejo que sus propios vecinos.

**Recuperado sin pérdida** desde `~/.claude/file-history/<sessionId>/`, el
historial de versiones por archivo que Claude Code guarda en cada edición
—independiente de git—, restaurando la última versión (`v10`) tomada
segundos antes del daño. Confirmado byte a byte por comportamiento: los
cuatro suites volvieron a dar exactamente 210+15+265+53, igual que antes
del incidente.

**Lección para instrucciones futuras a subagentes de crítica**: si van a
tocar el árbol de trabajo para probar algo, pedirles explícitamente que
reviertan con el editor (deshacer su propio bloque insertado) y no con
`git checkout --`/`git reset` sobre un archivo con cambios sin commitear
de otra sesión — ese comando no sabe distinguir "mis cambios de recién" de
"todo lo que no está en HEAD".

## Fundido por tiempo de chunk — el pop de anillo 0/1 no era el de anillo 2 (2026-08-14, madrugada)

El arreglo de `chunk_m` del anillo 2 (arriba) atacaba el pop equivocado.
Jugando, el usuario corrigió: el pasto aparece en cuadrados en el **anillo
0 (cercano) y el anillo 1 (mediano)**, no en el lejano — confirmado **sólo
caminando** (nunca parado) y con `growth_ramp` en su default (6 m, no un
efecto residual de haberla bajado a mano en una sesión anterior).

**Por qué ahí y no en el anillo 2**: mismo mecanismo de fondo
(`blade_growth` desvanece una brizna cerca de **su propio** alcance, sin
noción de cuándo nació su chunk), pero el anillo 0 tiene chunks chicos
(`chunk_m=12`) con alcance corto (`reach_m=24`) — se cruza su frontera
todo el tiempo al caminar, y por estar cerca cada chunk nuevo ocupa gran
parte de la pantalla. El anillo lejano tiene el mismo defecto pero se
cruza rara vez (hacen falta decenas de metros de caminata), por eso costó
tanto detectarlo la sesión anterior (comparación de capturas F7 con
`compare -fuzz`) y por eso ahora, jugando de verdad, lo que más se nota es
el cercano.

**Ya descartadas dos mitigaciones** antes de llegar acá: subir
`growth_ramp` empeora la cobertura pareja de toda la pradera (medido,
sección de arriba); achicar `chunk_m` del anillo 0 no tiene margen —
`chunk_m=8` casi agotó el presupuesto de draws del peor caso (99/100,
`docs/AHORA.md`), por eso quedó en 12.

### El arreglo real: una segunda dimensión de desvanecimiento, por tiempo

Se agregó `total_growth = min(blade_growth, chunk_time_fade)` — un chunk
recién horneado tarda `CHUNK_FADE_IN_S` (0,35 s) en llegar al
desvanecimiento por distancia que ya tenía prometido, en vez de aparecer a
altura completa de una.

**Mecanismo**: cada casillero de `RingRecords` gana un segundo buffer
(`born_buffer`), **un `f32` por casillero, no por brizna** — mucho más
chico que el buffer de registros. `roll_meadow_grid` lo estampa con
`Time::elapsed_secs()` al hornear un chunk genuinamente nuevo. El shader
(`chunk_time_fade`, `grass.wgsl`) lo resta contra un reloj gemelo
(`grass_data.chunk_clock`) y satura a `[0,1]` sobre la ventana de
`chunk_fade_in_s`.

**Dos riesgos reales que la crítica encontró antes de escribir código, no
después**:

1. **Reasignación de tier ≠ brizna nueva.** El anillo 0 reasigna de tier
   constantemente al caminar cerca de una frontera (histéresis). Si eso
   reseteara el reloj de nacimiento, reabriría exactamente el pop que la
   histéresis ya tapa. `ALREADY_GROWN_BORN_AT` (`-1.0e9`) es el centinela
   que usa `is_reassignment(key)` para decir "esta celda no es nueva, no
   arranques el fundido" — nunca `now`.
2. **El reloj con wrap no sirve para esto.** `grass_data.time` usa
   `Time::elapsed_secs_wrapped()` (wrappea cada hora, `Time::
   DEFAULT_WRAP_PERIOD`) porque la fase del viento sólo necesita precisión
   relativa dentro de una vuelta. Restar contra ese reloj daría una edad
   **negativa permanente** para cualquier chunk horneado justo antes de
   una vuelta — un bug rarísimo de reproducir, sólo visible cerca de la
   hora de juego continuo. `chunk_clock` es un campo de uniform aparte,
   con `elapsed_secs()` sin wrap.

Un tercer hallazgo de la crítica, mecánico: `debug_colour` ya tenía una
variable local `slot` que es el **índice de anillo**, no el casillero de
chunk — el parámetro nuevo se llama `chunk_slot` a propósito, y se calcula
en el sitio de la llamada (`mesh_functions::get_tag(in.instance_index)`
en el fragment), no "threadeado" desde el vertex shader como decía el
plan original (vertex y fragment no comparten variables locales).

**Decisiones de alcance, deliberadas**:
- `CHUNK_FADE_IN_S` es una constante (0,35 s), no un campo de
  `GrassRendererSettings` ni un control de F9 — mismo motivo que
  `GrowthRampOverride`: sólo escala un uniform, no cambia qué chunks
  existen, y meterlo en el struct que compara `MeadowRebuildDials`
  costaría un rehorneado completo por nada.
- Un rehorneado completo (perilla de densidad/alcance/anillos, pintar
  terreno) hace que **todos** los chunks entren por la rama "genuinamente
  nuevo" (`reassigned_cells` queda vacío ese frame) — decisión consciente,
  no un caso sin cubrir: total, la pradera entera ya aparece de golpe en
  ese evento raro, así que sincronizar un fundido de 0,35 s la suaviza en
  vez de empeorarla, y no hay fronteras de chunk visibles cuando todo
  aparece junto.
- `chunk_vitality`/`grass_vitality` (réplica en CPU, estadísticas de F9)
  se quedan sólo con el criterio de distancia — la vitalidad es una
  propiedad de estado estable, el fundido por tiempo es transitorio y
  siempre converge a 1.

**Validado**: `cargo fmt -- --check`/`clippy --all-targets -- -D
warnings` limpios, los cuatro suites en verde (213+15+265+53, tres tests
nuevos sobre la matemática del fundido en CPU). **Ningún test compila el
WGSL** (los existentes sólo hacen `contains()` de substrings) — la única
verificación real de que el shader compila es una corrida real
(`BOF_SHOT=grass`), hecha dos veces durante la implementación: renderiza
igual que antes (mismos triángulos/chunks/vitalidad por anillo), sin
errores nuevos en el log.

**No verificado jugando todavía** — una captura estática no puede mostrar
un fundido transitorio de 0,35 s; hace falta caminar y mirar.
