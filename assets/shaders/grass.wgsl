// El material de la pradera: PBR estándar más lo que hace que un campo se lea
// como un campo y no como una alfombra.
//
// Casi todo lo que este archivo hace es **gratis en geometría**: viento,
// variación por brizna, normal abombada y transmisión a contraluz no agregan un
// solo triángulo. Esa es la razón por la que la Fase 2 de `BOTWGrass.md` va
// después de la 1 y no antes — "denso no es frondoso", y subir densidad es la
// única de todas estas palancas que se paga en vértices.
//
// Es un `ExtendedMaterial`, así que conserva luz, sombras, niebla y decals; lo
// propio es de dónde salen el color base, la normal y la posición del vértice.
//
// **Qué lleva cada canal del vértice**, porque nada de esto es una uv de textura:
//   uv0.x  altura del suelo bajo la brizna (para colapsar hacia la raíz)
//   uv0.y  altura normalizada del vértice: 0 raíz, 1 punta
//   uv1.x  hash de la brizna, con el *lado* del quad en el signo
//   uv1.y  altura de la brizna en metros
//
// **El prepass tiene su propio vertex y fragment, no el default de Bevy**
// (2026-08-09): éste vértice reconstruye la brizna desde `blade_records`, así
// que el vertex de prepass default —que sólo transforma un atributo de
// posición horneado— no puede reproducir la misma geometría, y sin eso la
// profundidad no coincidiría con lo que el pase principal dibuja. Ambos
// caminos llaman a `build_blade`, para que no puedan divergir con el tiempo.
// Y el fragment de prepass default tampoco alcanza: no conoce la máscara alpha
// de la carta — sin recortarlo ahí, el prepass escribiría profundidad
// para el cuadrado entero de cada carta en vez de su silueta, y algo detrás
// del hueco recortado se perdería por early-Z contra una silueta que el pase
// principal nunca dibuja.
//
// `NormalPrepass` va siempre junto a `DepthPrepass` (`camera/mod.rs`): con
// sólo depth, Bevy bindea un material bind group **vacío** para materiales
// opacos (`is_depth_only_opaque_prepass`, bevy_pbr 0.19
// `prepass/mod.rs:381`) — y el vertex necesita `blade_records`, que vive en
// ese bind group, para reconstruir la brizna. Sin `NormalPrepass` el prepass
// no puede funcionar, no es una opción de calidad.

// `mesh_functions`, `view` y `position_world_to_clip` afuera del `#ifdef`:
// tanto el vertex principal como el de prepass reconstruyen la brizna
// (`build_blade`, más abajo) y los necesitan los dos. Antes vivían sólo en la
// rama `#else` porque el prepass no tenía vertex propio — el día que lo tuvo,
// habría faltado el import.
#import bevy_pbr::{
    pbr_types,
    pbr_functions::alpha_discard,
    pbr_fragment::pbr_input_from_standard_material,
    mesh_functions,
    mesh_view_bindings::view,
    view_transformations::position_world_to_clip,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::prepass_io::{VertexOutput, FragmentOutput}
#else
#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

struct GrassUniform {
    root_color: vec4<f32>,
    tip_color: vec4<f32>,
    sun_direction: vec3<f32>,
    sss_amount: f32,
    time: f32,
    /// Sin wrap, a diferencia de `time` — ver `chunk_born_at` y
    /// `total_growth`. Mismo orden que `GrassUniform::chunk_clock` en
    /// `grass_material.rs`.
    chunk_clock: f32,
    chunk_fade_in_s: f32,
    focus_xz: vec2<f32>,
    growth_ramp: f32,
    spike_from_m: f32,
    growth_sink: f32,
    wind_dir: vec2<f32>,
    wind_strength: f32,
    wind_speed: f32,
    tint_variation: f32,
    gradient_bias: f32,
    card_from_m: f32,
    debug_view: u32,
    blade_width: f32,
    ring_reaches_a: vec4<f32>,
    ring_reaches_b: vec4<f32>,
    ring_chunks_a: vec4<f32>,
    ring_chunks_b: vec4<f32>,
    ring_cards_a: vec4<f32>,
    ring_cards_b: vec4<f32>,
    card_half_width: f32,
    blade_root_sink: f32,
    blade_lean: f32,
    blade_waist: f32,
    /// `x` = registros por chunk, `y` = forma. **En un `vec4` y no como dos
    /// escalares sueltos**: así su alineación es la del bloque de 16 bytes y no
    /// depende de cuántos `f32` los precedan, que es donde un campo agregado al
    /// final corre en silencio todo lo que viene después.
    record_layout: vec4<u32>,
    /// Radiancia del sol, ya escalada por iluminancia. `w` sin usar. Mirror de
    /// `GrassUniform::sun_color` en `grass_material.rs` — mismo orden, mismo tipo.
    sun_color: vec4<f32>,
    /// Lo mismo para la ambiente. `w` sin usar.
    ambient_color: vec4<f32>,
    ring_colors: array<vec4<f32>, 8>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> grass_data: GrassUniform;

@group(#{MATERIAL_BIND_GROUP}) @binding(101)
var interaction_texture: texture_2d<f32>;

@group(#{MATERIAL_BIND_GROUP}) @binding(102)
var interaction_sampler: sampler;

@group(#{MATERIAL_BIND_GROUP}) @binding(104)
var card_albedo: texture_2d<f32>;

@group(#{MATERIAL_BIND_GROUP}) @binding(105)
var card_albedo_sampler: sampler;

/// Altura normalizada del vértice a lo largo de la brizna: 0 en la raíz, 1 en
/// la punta. Una sola definición, porque el viento, el degradado y el aplastado
/// del Paso 9 multiplican todos por exactamente esto.
fn blade_height_factor(uv: vec2<f32>) -> f32 {
    return clamp(uv.y, 0.0, 1.0);
}

/// UV de la carta de mata. La malla índice sólo transporta lado y altura; no
/// puede usar UV0 porque allí viaja la dirección `(brizna, esquina)` que ubica
/// el registro en el `ShaderBuffer`.
fn card_uv(in: VertexOutput) -> vec2<f32> {
    return vec2<f32>(in.uv_b.x * 0.5 + 0.5, 1.0 - blade_height_factor(in.uv));
}

/// La misma muestra resuelve detalle, alpha y prepass. Las derivadas se calculan
/// antes de cualquier branch no uniforme en el caller; así el pase de color y
/// profundidad no pueden divergir en el borde de la textura.
fn sample_card_albedo(uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>) -> vec4<f32> {
    return textureSampleGrad(card_albedo, card_albedo_sampler, uv, dx, dy);
}

/// Si **esta brizna**, a esta distancia, se dibuja como carta.
///
/// La forma la decide la distancia y no el nivel: desde que la brizna pertenece
/// al mundo, las de índice bajo —las del nivel de cartas— están vivas también a
/// tres metros, y ahí un billboard de medio metro gira con la cámara y se ve
/// (reportado jugando el 2026-08-08). Más cerca del umbral, la misma brizna se
/// construye como hoja: no desaparece, cambia de forma.
///
/// **El salto es duro a propósito.** El umbral es donde la brizna mide un píxel
/// y medio (`card_from_m`), así que ahí el cambio no se ve; interpolar el ancho
/// en cambio sí se veía — la carta angosta queda rectangular y con el tope
/// plano, que es una tira, no una brizna.
///
/// El vertex mide la distancia a la base y el fragment a su propio punto, así
/// que una brizna justo en el umbral puede construirse de una forma y recortarse
/// con la otra. Son 1,5 píxeles a esa distancia: menos que el error de cualquier
/// varying que se agregara para evitarlo.
///
/// **Y el umbral es de cada brizna, no del campo.** Con uno solo, todas las
/// cartas se abren sobre el mismo círculo y lo que se ve es un **anillo de
/// matojos que sigue al jugador** — encontrado mirando una captura el
/// 2026-08-08. Repartido, el cambio de forma ocurre en una banda de treinta
/// metros y deja de haber una línea.
///
/// La semilla es la **altura de la brizna**, que es idéntica en sus cuatro
/// vértices y llega igual al fragment (`fract(uv1.y)`): sin ella los dos lados
/// elegirían umbrales distintos para la misma brizna. Es el mismo identificador
/// que usa la vista `brizna`, por la misma razón.
fn card_distance_for(blade_seed: f32) -> f32 {
    return grass_data.card_from_m * (0.7 + 0.6 * fract(blade_seed * 91.0));
}

fn blade_is_card(distance: f32, blade_seed: f32) -> bool {
    return ring_is_card() && distance >= card_distance_for(blade_seed);
}

/// Cuánto crece una primitiva **dentro de su propia banda**, del umbral en el
/// que nace hasta su propio alcance. Compartida por la carta y la púa — ver
/// `CARD_GROWTH_MIN`/`MAX` y `SPIKE_GROWTH_MIN`/`MAX`.
///
/// **No toca el salto que ya existe.** En el umbral vale el mínimo, así que el
/// cambio de forma sigue siendo del mismo tamaño que hoy — interpolar *eso* ya
/// se probó una vez para la carta y se veía como una tira, no una brizna
/// (`BOTWGrass.md`). Lo que crece es sólo lo que pasa *después*: a igual
/// densidad, una primitiva más lejana representa más masa de pasto que una
/// recién nacida, y hoy las dos miden lo mismo sin que la perspectiva ni el
/// raleo de densidad lo compensen.
fn growth_scale(distance: f32, threshold: f32, blade_reach: f32, min_scale: f32, max_scale: f32) -> f32 {
    let span = max(blade_reach - threshold, 1e-3);
    let t = clamp((distance - threshold) / span, 0.0, 1.0);
    return mix(min_scale, max_scale, t);
}

/// Candidato a explicar por qué se sigue notando la banda de cartas
/// (`BOTWGrass.md` → *Por dónde retomar*). **Sin medir**, valor a ojo para
/// jugarlo; si hace falta, se sweepea como perilla igual que `grass-growth`.
const CARD_GROWTH_MIN: f32 = 1.0;
const CARD_GROWTH_MAX: f32 = 1.6;

fn card_growth_scale(distance: f32, threshold: f32, blade_reach: f32) -> f32 {
    return growth_scale(distance, threshold, blade_reach, CARD_GROWTH_MIN, CARD_GROWTH_MAX);
}

/// La otra mitad del experimento: engordar la púa en vez de reemplazarla por
/// una carta, para comparar las dos sin sacar ninguna (jugando, 2026-08-09).
/// No toca la carta ni la hoja — sólo el ancho de la púa, y sólo lejos de su
/// propio nacimiento. **Sin medir**, valor a ojo.
const SPIKE_GROWTH_MIN: f32 = 1.0;
const SPIKE_GROWTH_MAX: f32 = 3.0;

fn spike_growth_scale(distance: f32, threshold: f32, blade_reach: f32) -> f32 {
    return growth_scale(distance, threshold, blade_reach, SPIKE_GROWTH_MIN, SPIKE_GROWTH_MAX);
}

/// Desde dónde una brizna pierde la cintura, repartido por brizna igual que
/// `card_distance_for` — con otra semilla, para que el cambio de forma no
/// vuelva a ser un círculo.
fn spike_distance_for(blade_seed: f32) -> f32 {
    return grass_data.spike_from_m * (0.75 + 0.5 * fract(blade_seed * 57.0));
}

/// La forma con la que se construye **esta** brizna, a esta distancia.
///
/// Las tres salen del mismo criterio que la escalera de LOD ya usaba —cuántos
/// píxeles mide de ancho— pero aplicado por brizna. El nivel sólo aporta cuántos
/// triángulos tiene reservados en su malla, y todos reservan dos: la púa deja el
/// segundo degenerado (sus esquinas 2 y 3 caen en la punta) y no paga fragmentos.
///
/// Sin esto, una brizna de un nivel de púas que hoy está cerca se construía como
/// hoja pero sólo tenía un triángulo indexado: **media hoja**, que es *"pastos
/// chicos que se renderizan incluso cerca"* (jugando, 2026-08-08).
///
/// El umbral de la púa también va repartido por brizna, con otra semilla que el
/// de la carta: con uno solo, el cambio de forma vuelve a ser un círculo.
fn blade_shape_at(distance: f32, blade_seed: f32) -> u32 {
    if blade_is_card(distance, blade_seed) {
        return SHAPE_CARD;
    }
    let spike_at = spike_distance_for(blade_seed);
    return select(SHAPE_LEAF, SHAPE_SPIKE, distance >= spike_at);
}

/// Si esta primitiva se abre mirando a la cámara.
///
/// **Se lo pregunta al material, no a una tabla.** Cada nivel tiene el suyo, y
/// `record_layout.y` ya lleva su forma: buscarla otra vez recorriendo los
/// alcances era reconstruir un dato que el draw ya tenía, y bastaba con que esa
/// tabla llegara distinta al GPU para que la carta **no se abriera** — sus cuatro
/// vértices se hornean en el mismo punto y es este `if` el que los separa. Lo que
/// se veía entonces era el nivel lejano entero ausente, sin un solo error y con
/// resultado distinto en cada corrida (2026-08-07).
fn ring_is_card() -> bool {
    return grass_data.record_layout.y == SHAPE_CARD;
}

/// Cuánto de su altura conserva una brizna a esta distancia de la cámara.
///
/// Es **crecimiento, no transparencia**: la brizna se encoge hacia su propia
/// raíz y desaparece siendo geometría, sin blending, sin `discard` y sin orden
/// de dibujo — o sea sin apagar el early-Z, que en un GPU tile-based es tirar la
/// ventaja principal del chip (ley 3 de `BOTWGrass.md`).
///
/// **El umbral es de la brizna y viene en su registro.** Desde que la brizna
/// pertenece al mundo, su alcance sale del índice que ocupa en su baldosa, así
/// que acá no queda ley que aplicar ni hash que consultar: sólo una rampa de
/// `growth_ramp` metros antes de su propio final. La ley `1/d` se sigue
/// cumpliendo —de ahí salió la escalera de alcances— pero por construcción.
fn blade_growth(world_xz: vec2<f32>, blade_reach: f32) -> f32 {
    let distance = length(world_xz - grass_data.focus_xz);
    // **La corona de este nivel empieza acá.** Más cerca, la misma brizna la
    // dibuja el nivel de adentro: no desaparece, la dibuja otro. Por eso el corte
    // puede ser duro sin que se vea nada — es un relevo, no una frontera. Un
    // nivel que dibujara desde los pies del jugador tendría, aislado, briznas por
    // toda la pantalla, que es como se leyó jugando el 2026-08-08.
    if distance < f32(grass_data.record_layout.w) {
        return 0.0;
    }
    // Un solo umbral, y es de esta brizna: el índice que ocupa en su baldosa ya
    // decidió hasta dónde llega, así que acá no hay ley que repartir ni hash que
    // consultar. Nada nace del lado de adentro tampoco — una brizna existe desde
    // los pies del jugador hasta su propio final, y por eso no hay frontera que
    // cruzar.
    return 1.0 - smoothstep(blade_reach - grass_data.growth_ramp, blade_reach, distance);
}

/// Cuánto de su fundido por tiempo lleva el chunk de este casillero. `1.0` en
/// cuanto pasaron `chunk_fade_in_s` segundos desde que se horneó —
/// `chunk_born_at` ya viene con el centinela "ya creció" para reasignaciones
/// de tier, así que acá no hay caso especial que distinguir.
fn chunk_time_fade(chunk_slot: u32) -> f32 {
    let age = grass_data.chunk_clock - chunk_born_at[chunk_slot];
    return saturate(age / max(grass_data.chunk_fade_in_s, 0.001));
}

/// El desvanecimiento real de una brizna: el mínimo entre "qué tan cerca está
/// de su propio alcance" (`blade_growth`, por distancia) y "cuánto hace que su
/// chunk nació" (`chunk_time_fade`, por tiempo). Ninguno de los dos puede
/// hacer que una brizna se vea más crecida de lo que el otro permite — el
/// primero evita que un chunk recién horneado plante brizna a pleno donde ya
/// debería estar muerta; el segundo evita que aparezca a pleno de golpe donde
/// sí le toca vivir.
fn total_growth(world_xz: vec2<f32>, blade_reach: f32, chunk_slot: u32) -> f32 {
    return min(blade_growth(world_xz, blade_reach), chunk_time_fade(chunk_slot));
}

/// Ruido barato y determinista, para la ráfaga.
fn value_noise(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

/// Ruido suave de baja frecuencia: la interpolación entre cuatro esquinas de una
/// celda. Es lo que hace que la ráfaga tenga frente en vez de ser un temblor
/// parejo.
fn smooth_noise(p: vec2<f32>) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let blend = f * f * (3.0 - 2.0 * f);
    let a = value_noise(cell);
    let b = value_noise(cell + vec2<f32>(1.0, 0.0));
    let c = value_noise(cell + vec2<f32>(0.0, 1.0));
    let d = value_noise(cell + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, blend.x), mix(c, d, blend.x), blend.y);
}

/// Desplazamiento del viento para un vértice, en metros de mundo.
///
/// Tres capas, y la tercera es la que separa "hay viento" de "hay ráfagas":
///  1. una onda que viaja en la dirección del viento,
///  2. un segundo armónico más corto y más chico, para que no lea como coseno,
///  3. un ruido de escala grande que **modula la amplitud** de las dos — sin él
///     el campo entero ondea parejo y se lee como tela, con él hay zonas quietas
///     y zonas agitadas y una frontera que se desplaza.
///
/// Todo multiplicado por la altura normalizada y por la altura real de la
/// brizna: la raíz no se mueve y una brizna alta viaja más que una corta.
fn wind_offset(world_xz: vec2<f32>, height_factor: f32, blade_height: f32, phase: f32) -> vec2<f32> {
    let dir = normalize(grass_data.wind_dir);
    let travel = dot(world_xz, dir) - grass_data.time * grass_data.wind_speed;

    let primary = sin(travel * 0.55 + phase * 6.2831);
    let harmonic = 0.35 * sin(travel * 1.7 + phase * 3.1);

    // La ráfaga viaja más lento que la onda, o el frente se lee como parpadeo.
    let gust_field = world_xz * 0.045 - dir * (grass_data.time * 0.25);
    let gust = 0.35 + 0.65 * smooth_noise(gust_field);

    let sway = (primary + harmonic) * gust * grass_data.wind_strength;
    // Cuadrático en la altura: la brizna se arquea en vez de inclinarse rígida
    // desde la base, que es la diferencia entre una hoja y un palo.
    return dir * sway * height_factor * height_factor * blade_height;
}

// Las vistas de diagnóstico. El orden es el de `GRASS_DEBUG_STEPS` en
// `bof_domain::perf`, y hay un test en `visuals::grass_debug` que lo cobra.
const DEBUG_OFF: u32 = 0u;
const DEBUG_RING: u32 = 1u;
const DEBUG_CHUNK: u32 = 2u;
const DEBUG_BLADE: u32 = 3u;
const DEBUG_GROWTH: u32 = 4u;
const DEBUG_SUBPIXEL: u32 = 5u;
const DEBUG_MEASURE: u32 = 6u;
const DEBUG_SHAPE_MEASURE: u32 = 7u;

/// Cuántos píxeles de ancho tiene que medir una brizna para que valga lo que
/// cuesta.
///
/// El rasterizador trabaja en cuartetos de 2×2: un triángulo que no llena un
/// píxel dispara los cuatro fragmentos igual. Por debajo de un píxel de ancho la
/// brizna paga cuatro y aporta uno — la ley 2 de `BOTWGrass.md`, que hasta ahora
/// se podía citar pero no mirar. Dos píxeles es donde deja de haber desperdicio
/// de cuarteto en el eje angosto.
const SUBPIXEL_RED: f32 = 1.0;
const SUBPIXEL_GREEN: f32 = 2.0;

/// Cuánto tapa el pastel al color real en las vistas de *ver*.
///
/// No 1,0 a propósito: con el color entero reemplazado se pierde el degradado y
/// la luz, y una vista que destruye la imagen contesta preguntas sobre otra
/// imagen. Acá la categoría se lee de un vistazo y el campo sigue siendo un
/// campo.
const DEBUG_TINT: f32 = 0.72;

/// El nivel de este draw, como índice de la paleta.
///
/// **Lo dice el material, no una tabla.** Salía de buscar el alcance del vértice
/// entre los de los anillos, y desde que cada brizna lleva **su propio** alcance
/// esa búsqueda no encuentra nada: las vistas pintarían todo del gris de
/// "ninguno" y el medidor contaría cero por nivel. El draw ya sabe cuál es.
fn ring_slot() -> u32 {
    return grass_data.record_layout.z;
}

fn ring_chunk_m(slot: u32) -> f32 {
    var chunks = array<f32, 8>(
        grass_data.ring_chunks_a.x, grass_data.ring_chunks_a.y,
        grass_data.ring_chunks_a.z, grass_data.ring_chunks_a.w,
        grass_data.ring_chunks_b.x, grass_data.ring_chunks_b.y,
        grass_data.ring_chunks_b.z, grass_data.ring_chunks_b.w,
    );
    return max(chunks[slot], 0.001);
}

/// Un pastel determinista a partir de una semilla: tres canales de ruido
/// arrastrados hacia el blanco.
///
/// `whiten` no es una preferencia. Con categorías grandes —un chunk ocupa media
/// pantalla— conviene desaturar, porque el campo tiene que seguir viéndose
/// mientras se lo diagnostica. Con categorías **más chicas que un píxel**, que
/// es lo que es una brizna a diez metros, cada píxel promedia decenas de
/// colores y el promedio de colores claros al azar es un beige uniforme:
/// medido, la vista de brizna salía lisa. Ahí hay que saturar, porque lo que
/// distingue no es el color de una categoría sino el **contraste** entre
/// vecinas.
fn pastel(seed: vec2<f32>, whiten: f32) -> vec3<f32> {
    let raw = vec3<f32>(
        value_noise(seed),
        value_noise(seed + vec2<f32>(37.0, 17.0)),
        value_noise(seed + vec2<f32>(11.0, 91.0)),
    );
    return mix(raw, vec3<f32>(1.0), whiten);
}

/// El color de la vista puesta, ya mezclado con el color real.
///
/// `world_xz` es el del **vértice**, no el de la raíz, así que una brizna
/// inclinada a menos de 27 cm del borde de su chunk puede tener la punta del
/// color del chunk vecino. Es la inclinación horneada haciéndose visible; en
/// chunks de 8 a 32 m afecta un fleco y ninguna vista de medición lo usa.
fn debug_colour(
    base: vec3<f32>,
    world_xz: vec2<f32>,
    blade_reach: f32,
    blade_hash: f32,
    blade_height: f32,
    metres_per_pixel: f32,
    chunk_slot: u32,
) -> vec3<f32> {
    let view = grass_data.debug_view;
    if view == DEBUG_OFF || view == DEBUG_MEASURE || view == DEBUG_SHAPE_MEASURE {
        return base;
    }
    // Ojo: esto es el índice del **anillo** (`record_layout.z`), no el
    // casillero del chunk — `chunk_slot`, el parámetro nuevo de arriba, es
    // el otro "slot" que hace falta para `total_growth`. Mismo nombre,
    // preguntas distintas; no confundirlos indexa el buffer equivocado.
    let ring = ring_slot();
    var tint = vec3<f32>(1.0);
    if view == DEBUG_RING {
        tint = grass_data.ring_colors[ring].rgb;
    } else if view == DEBUG_CHUNK {
        // Una celda es una malla y un draw call: esta vista es también el mapa
        // de draws de la pradera.
        let cell = floor(world_xz / ring_chunk_m(ring));
        tint = pastel(cell * 13.0 + f32(ring) * 101.0, 0.45);
    } else if view == DEBUG_BLADE {
        // **La semilla es la altura, no el hash.** `blade_hash` sale de
        // `abs(uv1.x)`, que lleva el lado del quad en el signo: en el *vértice*
        // vale exactamente el hash, pero interpolado a lo ancho de la brizna
        // barre de +h a −h pasando por cero, así que en el fragment **no es
        // constante por brizna**. Con él, esta vista salía como ruido RGB por
        // píxel en vez de un color por brizna — así se encontró.
        //
        // `fract(uv1.y)` es la altura en metros y viaja igual en los cinco
        // vértices, así que sí identifica la brizna. Rinde [0,45; 0,90], que es
        // un aleatorio determinista por brizna y alcanza de sobra como semilla.
        //
        // Sin desaturar: una brizna es más chica que un píxel en casi todo el
        // cuadro, y el promedio de pasteles claros es un beige liso.
        let per_blade = fract(blade_height);
        tint = pastel(vec2<f32>(per_blade * 813.0, per_blade * 271.0), 0.0);
    } else if view == DEBUG_GROWTH {
        // Dos entradas de la paleta y no dos colores nuevos: el shader no
        // inventa colores, los recibe. `total_growth` y no `blade_growth` a
        // secas: esta vista tiene que mostrar lo que el jugador realmente ve,
        // que desde el fundido por tiempo ya no es sólo el criterio de
        // distancia.
        let grown = total_growth(world_xz, blade_reach, chunk_slot);
        tint = mix(
            grass_data.ring_colors[0].rgb,
            grass_data.ring_colors[3].rgb,
            grown,
        );
    }
    return mix(base, tint, DEBUG_TINT);
}

/// La normal de la brizna: **+Y**, abierta un poco hacia afuera a lo ancho.
///
/// El +Y es la ley del sistema: una cara plana iluminada por su propia normal se
/// apaga en cuanto el sol pega de lado, y un campo entero apagándose a la vez es
/// lo que más delata al pasto generado. El abombado lateral hace que sombree
/// como un cilindro suave en vez de como un papel, y sale de un dato que ya
/// viaja: el signo del hash dice de qué borde del quad es este vértice.
fn blade_normal(side: f32, world_xz: vec2<f32>) -> vec3<f32> {
    let across = normalize(vec3<f32>(-grass_data.wind_dir.y, 0.0, grass_data.wind_dir.x));
    // 0,35 abría tanto la normal que un borde entero de la brizna quedaba
    // notablemente más oscuro que el otro y el campo se veía moteado. 0,18
    // insinúa el cilindro sin llegar a leerse como dos caras distintas.
    return normalize(vec3<f32>(0.0, 1.0, 0.0) + across * side * 0.18);
}

/// El vértice que este material lee, declarado acá y no importado.
///
/// **Una dirección y una posición que nadie lee.** Lo único que este shader
/// necesita de la malla es *cuál brizna del chunk* y *cuál esquina de ella* es
/// este vértice, y eso viaja en `UV_0` — ver `ring_index_mesh` en
/// `grass_records.rs`, que explica por qué no puede salir de `vertex_index`.
///
/// **Y lleva sólo los atributos que la malla índice tiene.** Ésa es la trampa de
/// escribirlo a mano: en el de Bevy cada campo vive dentro de su `#ifdef` y el
/// struct se encoge solo hasta calzar con la malla. Acá no hay nada que lo
/// encoja, así que declarar un atributo de más —`normal`, en el location 1— pide
/// a la etapa anterior un `Float32x3` que nadie provee, y el pipeline no
/// compila. wgpu lo dice con el número de location, que es lo único que hizo
/// esto barato de encontrar.
struct GrassVertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    /// `x` = qué brizna del chunk, `y` = qué esquina de la brizna.
    @location(2) address: vec2<f32>,
}

/// Un registro por primitiva: dónde nace y qué forma tiene.
///
/// **16 bytes contra los 136 que pesa una hoja horneada** (cuatro vértices de 28
/// B más seis índices de 4). Es el canje del Paso 2 de `BOTWGrass.md`: la brizna
/// deja de ser geometría y pasa a ser un dato, y la malla se convierte en un
/// índice que todos los chunks del nivel comparten.
/// `xy` es la base en XZ de mundo, `z` la altura del terreno debajo, y `w` el
/// alcance del anillo en la parte entera con la altura de la brizna en la
/// fracción — el mismo empaquetado que viajaba en `uv1.y`. **El orden vive acá y
/// en `blade_record` de `grass.rs`, y en ningún otro lado.**
struct BladeRecord {
    base_and_shape: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(103)
var<storage, read> blade_records: array<BladeRecord>;

/// Un `f32` por **casillero**, no por brizna: cuándo nació (`chunk_clock` de
/// ese momento) el chunk que ocupa ese casillero. Ver `total_growth`.
@group(#{MATERIAL_BIND_GROUP}) @binding(106)
var<storage, read> chunk_born_at: array<f32>;

/// Las tres formas, en el orden de `BladeShape` de `grass.rs`.
const SHAPE_LEAF: u32 = 0u;
const SHAPE_SPIKE: u32 = 1u;
const SHAPE_CARD: u32 = 2u;

/// El hash de una brizna a partir de dónde está parada.
///
/// **De la posición y no del índice del registro**, y no es un detalle: los
/// casilleros se reasignan cuando la grilla rueda, así que un hash atado al
/// casillero cambiaría el umbral de crecimiento de una brizna mientras el
/// jugador camina — que es el artefacto que todo este sistema evita.
///
/// `salt` da varios números independientes de la misma posición: la orientación
/// y la inclinación salen de acá en vez de viajar en el registro, que es lo que
/// lo mantiene en 16 bytes.
///
/// **Mezcla de enteros, no `fract(sin(...))`.** El truco del seno se degrada
/// justamente donde este campo vive: con coordenadas de mundo de hasta ±160 m el
/// argumento llega a los diez mil, y en `f32` el seno de eso pierde tantos bits
/// que el resultado se agrupa en unos pocos valores. Medido *(a, 2026-08-07)*: el
/// umbral de crecimiento sale de este hash, y con el seno la banda de 45-64 m
/// caía al 44,8% donde debía dar 96%.
fn hash_position(world_xz: vec2<f32>, salt: u32) -> f32 {
    // A milímetros, que es más fino que cualquier separación entre briznas y
    // entra de sobra en un i32 para un mundo de 320 m.
    let q = vec2<i32>(floor(world_xz * 1000.0));
    var h = bitcast<u32>(q.x) * 0x9e3779b9u
        ^ bitcast<u32>(q.y) * 0x85ebca6bu
        ^ (salt + 1u) * 0xc2b2ae35u;
    // Wang hash: cinco operaciones enteras y una avalancha decente.
    h = (h ^ 61u) ^ (h >> 16u);
    h = h + (h << 3u);
    h = h ^ (h >> 4u);
    h = h * 0x27d4eb2du;
    h = h ^ (h >> 15u);
    return f32(h & 0x00ffffffu) / f32(0x01000000u);
}

/// La geometría de una brizna, construida en el vertex shader.
///
/// **Esto es lo que antes se horneaba.** Los cuatro (o tres) vértices salen de un
/// registro de 16 bytes más dos hashes de su posición; lo que la malla aporta es
/// sólo cuál de ellos es éste. La forma es la misma que tenía
/// `build_chunk_mesh`: la hoja termina en punta por los dos lados y lleva una
/// fila de vértices en la cintura, que es lo que le permite arquearse.
struct BladeVertex {
    offset: vec3<f32>,
    along: f32,
    side: f32,
}

fn blade_vertex(shape: u32, corner: u32, height: f32, world_xz: vec2<f32>, spike_width_scale: f32) -> BladeVertex {
    // **Inicializados, no dados por cero.** WGSL no promete que un `var` sin
    // inicializar valga cero, y las esquinas de la base no escriben `along`: una
    // altura basura ahí manda el vértice a cualquier parte, que es un triángulo
    // gigante cruzando el campo.
    var out: BladeVertex;
    out.offset = vec3<f32>(0.0, 0.0, 0.0);
    out.along = 0.0;
    out.side = -1.0;
    if shape == SHAPE_CARD {
        // La carta nace entera en el centro de la base; el shader la abre más
        // abajo, contra el eje derecho de la cámara.
        //
        // **Las esquinas van en filas, no en ciclo**, y eso lo manda la malla
        // índice: sus dos triángulos son `(0,1,2)` y `(1,3,2)`, que es la
        // triangulación de una tira —abajo-izq, abajo-der, arriba-izq,
        // arriba-der—. Numeradas en ciclo, el segundo triángulo no contiene al
        // vértice 0 y **media carta no se dibuja**: medido *(a)*, la banda de
        // 45-64 m se cayó de 97,4% a 45,0% sin que la forma se viera rota.
        out.offset = vec3<f32>(0.0, 0.0, 0.0);
        out.side = select(-1.0, 1.0, (corner & 1u) == 1u);
        out.along = select(0.0, 1.0, corner >= 2u);
        return out;
    }

    let yaw = hash_position(world_xz, 1u) * TAU;
    let across = vec2<f32>(cos(yaw), sin(yaw)) * (grass_data.blade_width * 0.5);
    // La inclinación va contra la perpendicular del ancho, para que las briznas
    // no se caigan todas para el mismo lado.
    let lean_amount = (hash_position(world_xz, 2u) - 0.5) * 2.0 * grass_data.blade_lean;
    let lean = vec2<f32>(-sin(yaw), cos(yaw)) * lean_amount;
    let tip = vec3<f32>(lean.x, height, lean.y);

    if shape == SHAPE_SPIKE {
        // Un triángulo: dos esquinas en la base y una punta. **El ancho crece
        // con la distancia** (`spike_growth_scale`), para comparar contra la
        // carta sin sacarla — la punta no escala, sólo la base, así que la
        // silueta sigue terminando afilada y no se engorda como un poste.
        let spike_across = across * spike_width_scale;
        if corner == 0u {
            out.offset = vec3<f32>(-spike_across.x, 0.0, -spike_across.y);
            out.side = -1.0;
        } else if corner == 1u {
            out.offset = vec3<f32>(spike_across.x, 0.0, spike_across.y);
            out.side = 1.0;
        } else {
            out.offset = tip;
            out.along = 1.0;
            out.side = 1.0;
        }
        return out;
    }

    // La hoja: raíz hundida, dos vértices de cintura y punta.
    let waist = tip * grass_data.blade_waist;
    if corner == 0u {
        out.offset = vec3<f32>(0.0, -grass_data.blade_root_sink, 0.0);
        out.side = -1.0;
    } else if corner == 1u {
        out.offset = vec3<f32>(waist.x - across.x, waist.y, waist.z - across.y);
        out.along = grass_data.blade_waist;
        out.side = -1.0;
    } else if corner == 2u {
        out.offset = vec3<f32>(waist.x + across.x, waist.y, waist.z + across.y);
        out.along = grass_data.blade_waist;
        out.side = 1.0;
    } else {
        out.offset = tip;
        out.along = 1.0;
        out.side = 1.0;
    }
    return out;
}

const TAU: f32 = 6.2831853;

/// Lo que la brizna necesita para ir a cualquiera de los dos `VertexOutput`
/// posibles (principal o prepass) — ver la nota de arriba del porqué hay dos
/// caminos.
struct BuiltBlade {
    world_position: vec4<f32>,
    world_normal: vec3<f32>,
    uv: vec2<f32>,
    uv_b: vec2<f32>,
}

/// La reconstrucción entera de una brizna: qué registro le toca, qué forma
/// tiene a esta distancia, viento y colapso por crecimiento. Un solo lugar
/// para las dos etapas que la necesitan (principal y prepass), para que no
/// puedan divergir — sería exactamente la clase de bug que un desfasaje de
/// profundidad no avisa solo.
fn build_blade(instance_index: u32, address: vec2<f32>) -> BuiltBlade {
    // **Nada de esto viene de la malla salvo una dirección**: qué brizna del
    // chunk y qué esquina de ella. Todo lo demás sale de un registro de 16 bytes
    // que el chunk tiene en el buffer, ubicado por el `MeshTag` —su casillero
    // dentro del nivel, con stride fijo—.
    let slot = mesh_functions::get_tag(instance_index);
    let blade = u32(address.x);
    let corner = u32(address.y);
    let record = blade_records[slot * grass_data.record_layout.x + blade].base_and_shape;

    // `uv1.y` llevaba dos números en uno y el registro los sigue llevando: el
    // alcance del anillo en metros enteros y la altura de la brizna en la
    // fracción.
    let blade_reach = floor(record.w);
    let blade_height = fract(record.w);
    let blade_hash = hash_position(record.xy, 0u);

    // **La forma sale de la distancia, no del nivel.** Una brizna del nivel de
    // cartas que hoy está cerca se construye como hoja: con la del nivel
    // quedaría un billboard de medio metro girando a tres metros de la cámara.
    let distance_to_focus = length(record.xy - grass_data.focus_xz);
    let shape = blade_shape_at(distance_to_focus, blade_height);
    let as_card = shape == SHAPE_CARD;
    // Sólo importa cuando `shape == SHAPE_SPIKE`; para hoja y carta el valor
    // no se usa, así que calcularlo siempre es más barato que un branch más.
    let spike_width_scale = spike_growth_scale(distance_to_focus, spike_distance_for(blade_height), blade_reach);
    let built = blade_vertex(shape, corner, blade_height, record.xy, spike_width_scale);
    let side = built.side;
    let height_factor = built.along;
    // `record.z` es la altura del terreno: lo que `uv0.x` llevaba por vértice.
    var world_position = vec4<f32>(
        record.x + built.offset.x,
        record.z + built.offset.y,
        record.y + built.offset.z,
        1.0,
    );
    var uv = vec2<f32>(record.z, height_factor);
    var uv_b = vec2<f32>(side * blade_hash, record.w);

    // **La carta se abre acá, y sólo tan lejos como corresponda.** Sus cuatro
    // vértices vienen horneados en el mismo punto —el centro de la base— y se
    // separan contra el eje derecho de la cámara, así que siempre da la cara.
    // Una carta representa la masa de decenas de briznas: si quedara de canto
    // dejaría un hueco de ese tamaño.
    //
    // **Lo que se interpola es el ancho, no si gira.** Desde que la brizna
    // pertenece al mundo, las de índice bajo —las del nivel de cartas— están
    // vivas también a tres metros, y ahí una carta de medio metro girando con la
    // cámara se ve (reportado jugando el 2026-08-08). Angosta no: una brizna de
    // 5,5 cm que gira es indistinguible de una que no, así que no hace falta un
    // segundo camino de construcción ni un triángulo más.
    //
    // El eje derecho sale de la fila 0 de la matriz de vista, que es la que
    // lleva el `right` de la cámara en espacio de mundo, aplanado a horizontal
    // para que la carta se quede parada en vez de inclinarse con el cabeceo.
    if as_card {
        let camera_right = normalize(vec3<f32>(view.view_from_world[0].x, 0.0, view.view_from_world[2].x));
        // Crece dentro de su propia banda, del umbral en el que nace hasta su
        // propio alcance — ver `card_growth_scale`. En el umbral vale 1.0, así
        // que el salto de hoja a carta sigue midiendo lo mismo que antes.
        let growth = card_growth_scale(distance_to_focus, card_distance_for(blade_height), blade_reach);
        // **Varianza de ancho por carta (técnica 1 de 3, `NORTE.md`/discusión
        // 2026-08-09).** El diagnóstico de siempre seguía sin resolverse: la
        // carta mira siempre a cámara y por eso muestra **siempre** su ancho
        // completo, mientras que una púa real tiene orientación fija en el mundo
        // y la mayoría de las veces se ve de perfil. No se toca el eje (`side *
        // camera_right`): girar la carta reintroduce el hueco de cobertura que el
        // billboard existe para evitar (comentario de arriba). En cambio se
        // reparte el **ancho** por brizna — mismo billboard, distinta huella —
        // que aproxima la variedad que una población de púas mostraría sin
        // arriesgar una carta de canto. Rango ±30% **centrado en 1.0**: no
        // desplaza la media, así que `footprint_m()` y la ley de densidad de
        // `minimum_density` siguen valiendo sin tocarlas.
        let width_seed = hash_position(record.xy, 3u);
        let width_scale = mix(0.7, 1.3, width_seed);
        world_position = vec4<f32>(
            world_position.xyz
                + camera_right * (side * grass_data.card_half_width * growth * width_scale)
                + vec3<f32>(0.0, height_factor * blade_height * growth, 0.0),
            world_position.w,
        );
    }

    // Viento primero, sobre la posición horneada.
    let sway = wind_offset(world_position.xz, height_factor, blade_height, blade_hash);
    world_position = vec4<f32>(
        world_position.x + sway.x,
        world_position.y,
        world_position.z + sway.y,
        world_position.w,
    );

    // Y después el crecimiento por distancia, que colapsa la brizna hacia un
    // punto **bajo** el suelo. `uv.x` lleva la altura del terreno justamente
    // para esto: sin ella el shader no sabe dónde está la base.
    //
    // **Lo de "bajo" es el arreglo del parpadeo** (reportado jugando tres veces;
    // la tercera con la descripción que lo resolvió: "unos pastos que parecen
    // pegados en el piso que parpadean"). Colapsando hacia `ground_y` a secas,
    // una brizna encogida no desaparece: sus cuatro vértices llegan a la altura
    // del suelo, pero la punta conserva su desplazamiento horizontal —el lean
    // horneado más el viento— así que queda un cuadrilátero **horizontal,
    // coplanar con el terreno**. Eso es z-fighting, el viento lo agita, y
    // parpadea. No era aliasing: MSAA no lo habría tocado.
    //
    // Hundiendo el punto de colapso, la brizna se mete en la tierra antes de
    // degenerar y el terreno la tapa por profundidad. Y de paso hace lo que el
    // sistema quería desde el principio: la brizna **brota del suelo** en vez de
    // aparecer aplastada sobre él. Emerge cuando el crecimiento pasa de
    // `sink / (altura + sink)`, o sea alrededor de un quinto de la rampa.
    // La altura del terreno sale del registro, que es donde vive desde que el
    // vértice dejó de llevarla.
    let ground_y = record.z;
    world_position.y = mix(
        ground_y - grass_data.growth_sink,
        world_position.y,
        total_growth(world_position.xz, blade_reach, slot),
    );

    // **La carta cambia lo que lleva en `uv_b.x`**: en vez del hash con el lado
    // en el signo, el lado a secas. Interpolado a lo ancho da −1 en un borde y
    // +1 en el otro, que es la coordenada que la silueta necesita y que de otro
    // modo no existiría — el hash es un número distinto por primitiva, así que
    // no se puede normalizar en el fragment sin mandarlo aparte.
    //
    // El hash ya hizo su trabajo acá arriba, así que no se pierde nada más que la
    // vista `brizna` sobre las cartas, que pasa a ser un degradado a lo ancho.
    if as_card {
        uv_b = vec2<f32>(side, uv_b.y);
    }

    var out: BuiltBlade;
    out.world_position = world_position;
    out.world_normal = blade_normal(side, world_position.xz);
    out.uv = uv;
    out.uv_b = uv_b;
    return out;
}

#ifndef PREPASS_PIPELINE
@vertex
fn vertex(vertex: GrassVertex) -> VertexOutput {
    var out: VertexOutput;
    let built = build_blade(vertex.instance_index, vertex.address);
    out.world_position = built.world_position;
    // Clip space, no world space: la posición que sale del vertex shader es la
    // que el rasterizador proyecta.
    out.position = position_world_to_clip(built.world_position.xyz);
    out.world_normal = built.world_normal;
    out.uv = built.uv;
    out.uv_b = built.uv_b;
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
    return out;
}
#endif

/// El vértice que el prepass lee. **`address` en `location(1)`, no `(2)`**: en
/// `prepass_io::Vertex` (bevy_pbr 0.19) el hueco que en el principal reserva
/// `normal`@1 no existe hasta location(3) y detrás de
/// `NORMAL_PREPASS_OR_DEFERRED_PREPASS`, así que UV0 —donde viaja
/// `address`— cae un lugar antes. Verificado contra
/// `prepass/mod.rs::PrepassPipeline::specialize`, que arma el layout real:
/// `ATTRIBUTE_UV_0.at_shader_location(1)`. Copiar el struct principal tal
/// cual no compila.
#ifdef PREPASS_PIPELINE
struct PrepassGrassVertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) address: vec2<f32>,
}

@vertex
fn vertex(vertex: PrepassGrassVertex) -> VertexOutput {
    var out: VertexOutput;
    let built = build_blade(vertex.instance_index, vertex.address);
    out.position = position_world_to_clip(built.world_position.xyz);
    out.world_position = built.world_position;
    out.uv = built.uv;
    out.uv_b = built.uv_b;
#ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
    out.world_normal = built.world_normal;
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
    return out;
}
#endif

#ifndef PREPASS_PIPELINE
@fragment
fn fragment(
    vertex_output: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var in = vertex_output;
    // Cuánto mundo cubre un píxel acá. Fuera de todo branch, porque una derivada
    // en control de flujo no uniforme no está definida.
    let metres_per_pixel = length(fwidth(in.world_position.xz));

    // **El recorte de la carta, antes que nada — mismo motivo que arriba.**
    // Las derivadas van afuera del branch de la carta, para que `textureSampleGrad`
    // siga siendo válida y color/profundidad evalúen el mismo texel.
    let card_texture_uv = card_uv(in);
    let card_texture_dx = dpdx(card_texture_uv);
    let card_texture_dy = dpdy(card_texture_uv);

    var card_alpha = 1.0;
    let is_card_blade = blade_is_card(length(in.world_position.xz - grass_data.focus_xz), fract(in.uv_b.y));
    if is_card_blade {
        card_alpha = sample_card_albedo(card_texture_uv, card_texture_dx, card_texture_dy).a;
    }

    // **Sub-píxel**: en bandas planas y exactas, no en una rampa.
    //
    // Una rampa continua se mira; una banda se **cuenta**. La pregunta que esta
    // vista existe para contestar —a qué distancia una brizna deja de resolverse—
    // se contestaba comparando colores a ojo, que es justo lo que esta sesión
    // vino a sacar del medio. Con tres bandas exactas, `perf::shot_stats` dice qué
    // fracción del pasto está por debajo de un píxel.
    if grass_data.debug_view == DEBUG_SUBPIXEL {
        // El ancho es el de **esta** primitiva, no el de una brizna. Una carta
        // mide 0,5 m contra los 5,5 cm de una brizna: medirlas con la misma vara
        // las reportaba nueve veces más finas de lo que son, que es exactamente
        // la clase de error que esta vista existe para cazar.
        //
        // **Y una púa lejos no mide lo mismo que una cerca**, desde que
        // `spike_growth_scale` la engorda con la distancia (experimento
        // 2026-08-09): sin esto, la vista subestimaría cuántos píxeles mide una
        // púa crecida y reportaría desperdicio de cuarteto que ya no existe.
        let subpixel_distance = length(in.world_position.xz - grass_data.focus_xz);
        let subpixel_seed = fract(in.uv_b.y);
        var width = grass_data.blade_width;
        if blade_is_card(subpixel_distance, subpixel_seed) {
            width = grass_data.card_half_width * 2.0;
        } else if subpixel_distance >= spike_distance_for(subpixel_seed) {
            width *= spike_growth_scale(
                subpixel_distance,
                spike_distance_for(subpixel_seed),
                floor(in.uv_b.y),
            );
        }
        let pixels_wide = width / max(metres_per_pixel, 1e-6);
        var band = 0u;
        if pixels_wide >= SUBPIXEL_GREEN {
            band = 3u; // se resuelve entera: no hay desperdicio de cuarteto
        } else if pixels_wide >= SUBPIXEL_RED {
            band = 5u; // entre uno y dos píxeles: el cuarteto ya desperdicia
        }
        var banded: FragmentOutput;
        banded.color = vec4<f32>(grass_data.ring_colors[band].rgb, 1.0);
        return banded;
    }

    // **Medir**: el color exacto del anillo, y se termina acá.
    //
    // Sin luz, sin niebla y sin el post-procesado: lo que llega al PNG es el
    // valor que está en la paleta, y contar píxeles de un color conocido es
    // aritmética en vez de detección de bordes. Sale antes del PBR a propósito
    // — cualquier cosa que multiplique este color lo vuelve inservible para lo
    // único que esta vista hace.
    if grass_data.debug_view == DEBUG_MEASURE {
        var measured: FragmentOutput;
        measured.color = vec4<f32>(
            grass_data.ring_colors[ring_slot()].rgb,
            1.0,
        );
        return measured;
    }

    // Mismo camino plano que `medir`, pero la categoría es la forma que esta
    // brizna ya construyó. La alpha de una carta se conserva para que
    // AlphaToCoverage cuente su huella real, no el rectángulo de su textura.
    if grass_data.debug_view == DEBUG_SHAPE_MEASURE {
        let shape = blade_shape_at(
            length(in.world_position.xz - grass_data.focus_xz),
            fract(in.uv_b.y),
        );
        var measured: FragmentOutput;
        measured.color = vec4<f32>(
            grass_data.ring_colors[shape].rgb,
            select(1.0, card_alpha, shape == SHAPE_CARD),
        );
        return measured;
    }

    // El PBR entero primero: sin esto el pasto sale plano, sin luz, sin sombras
    // y sin niebla, que es exactamente lo que `ExtendedMaterial` existe para
    // conservar.
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    // **El arreglo del pasto negro.** El material es `double_sided`, y para una
    // cara trasera Bevy invierte la normal (`prepare_world_normal`: multiplica
    // por -1 cuando `!is_front`). Nuestra normal apunta a +Y, así que invertida
    // apunta al suelo: cero luz del sol, brizna negra. Con yaw al azar, la mitad
    // del campo le da la espalda a la cámara, y de ahí el moteado oscuro.
    //
    // La brizna no tiene reverso: es una hoja, y las dos caras miran al cielo
    // por la misma razón por la que la normal es +Y y no la de la cara. Así que
    // se reponen las dos normales que el PBR usa — `world_normal` para el
    // sombreado de sombras y `N` para la iluminación.
    let lit_normal = normalize(in.world_normal);
    pbr_input.world_normal = lit_normal;
    pbr_input.N = lit_normal;

    let factor = blade_height_factor(in.uv);
    let blade_hash = abs(in.uv_b.x);
    // El degradado va sesgado hacia la raíz, no lineal: el campo se ve desde
    // arriba, así que lo que llena la pantalla son puntas, y un degradado
    // lineal deja al campo entero en el promedio de sus dos colores. Sesgado, la
    // brizna se queda oscura casi hasta el final y sólo el último tramo se
    // ilumina — que es lo que hace un dosel de verdad. Ver `gradient_bias` en
    // `grass_material.rs`, con la medición que lo eligió.
    //
    // Una interpolación entre la rampa lineal y su cuadrado, y **no `pow`**.
    // `pow` con exponente variable son dos transcendentales por fragmento, y
    // este frame es fill-bound (eso sí está medido, 2026-08-06): lo que se paga
    // por píxel se paga muchas veces por píxel. Un multiply y un mix dan la
    // misma curva a ojo.
    //
    // **Cuánto ahorra no se sabe.** Se intentó medirlo el mismo día y las tres
    // corridas dieron 10,89, 11,88 y 3,83 ms para el mismo pasto, con Blender,
    // Firefox y Discord abiertos. La forma barata se elige por principio, no por
    // una medición — y esta nota está acá para que nadie la cite como una.
    //
    // Sólo el color: el viento y el crecimiento siguen leyendo `factor` crudo,
    // porque sesgarles la altura les cambia la física, no el look.
    let shade = mix(factor, factor * factor, grass_data.gradient_bias);
    var colour = mix(grass_data.root_color, grass_data.tip_color, shade);

    // Variación por brizna: un corrimiento de tono y valor que rompe la lectura
    // de superficie única. Es de las cosas más baratas del sistema y de las que
    // más se notan — un campo de un solo verde se lee como alfombra por densa
    // que sea.
    //
    // **Y hasta el 2026-08-07 no era por brizna.** Se sembraba de `blade_hash`,
    // que sale de `abs(uv1.x)`: ese canal lleva el hash con el *lado* del quad
    // en el signo, así que interpolado a lo ancho de la brizna barre de +h a −h
    // pasando por cero. Lo que producía no era un corrimiento por brizna sino un
    // degradado simétrico dentro de cada una, **de media cero** — o sea el
    // efecto que este comentario dice que se nota, sin notarse. Lo encontró la
    // vista `grass-view=brizna`, que por lo mismo salía como ruido por píxel.
    //
    // `fract(uv1.y)` es la altura en metros y viaja idéntica en los cinco
    // vértices, así que interpolarla la deja igual: es el único identificador de
    // brizna que el vértice ya carga. El `fract(·137)` la descorrelaciona de la
    // altura —si no, las briznas altas tendrían todas el mismo tono— con un
    // multiply y un fract, sin transcendentales, que es lo que un frame
    // fill-bound tolera.
    let blade_tint = fract(fract(in.uv_b.y) * 137.0);
    let drift = (blade_tint - 0.5) * 2.0 * grass_data.tint_variation;
    colour = vec4<f32>(
        colour.r * (1.0 + drift * 0.6),
        colour.g * (1.0 + drift),
        colour.b * (1.0 + drift * 0.4),
        colour.a,
    );

    // Y recién acá la vista de diagnóstico, sobre el color ya armado: tiñe el
    // albedo y deja que la luz, la sombra y la niebla sigan actuando. Con la
    // vista apagada esto devuelve `colour` sin tocar — un branch por uniforme,
    // igual para todo el draw, no por fragmento divergente.
    colour = vec4<f32>(
        debug_colour(
            colour.rgb,
            in.world_position.xz,
            floor(in.uv_b.y),
            blade_hash,
            in.uv_b.y,
            metres_per_pixel,
            mesh_functions::get_tag(in.instance_index),
        ),
        colour.a,
    );

    // La ilustración aporta la lectura de hojas superpuestas, no una segunda
    // iluminación. Conservamos el degradado, tinte por brizna y transmisión
    // de la pradera, y aplicamos sólo su variación de luminosidad normalizada
    // contra el promedio visible del asset (#529338, luma lineal 0,231).
    if is_card_blade {
        let card_rgb = sample_card_albedo(card_texture_uv, card_texture_dx, card_texture_dy).rgb;
        let tone = clamp(dot(card_rgb, vec3<f32>(0.2126, 0.7152, 0.0722)) / 0.231, 0.72, 1.28);
        colour = vec4<f32>(colour.rgb * tone, colour.a);
    }

    pbr_input.material.base_color = vec4<f32>(colour.rgb, colour.a * card_alpha);
    pbr_input.material.base_color =
        alpha_discard(pbr_input.material, pbr_input.material.base_color);

    var out: FragmentOutput;
    // **Experimento 2026-08-09**: no `apply_pbr_lighting`. Ese pipeline evalúa
    // luces clusterizadas (point/spot) y sombra por fragmento — carísimo en un
    // frame fill-bound donde el pasto es el 98% de los píxeles, y ninguna luz
    // puntual dinámica ilumina la pradera hoy (no hay ninguna en el juego). Una
    // sola difusa direccional + ambiente plano, con los mismos `sun_color` /
    // `ambient_color` que ya sigue el ciclo día/noche. Si se mide que no ahorra
    // lo suficiente, esto vuelve a `apply_pbr_lighting(pbr_input)`.
    let n_dot_l = max(dot(pbr_input.N, normalize(grass_data.sun_direction)), 0.0);
    let lit = grass_data.ambient_color.rgb + n_dot_l * grass_data.sun_color.rgb;
    out.color = vec4<f32>(pbr_input.material.base_color.rgb * lit, pbr_input.material.base_color.a);

    // Transmisión a contraluz: la luz atraviesa la hoja. Es lo que separa "hay
    // pasto" de "hay un campo vivo", y sólo aparece cuando el sol está detrás
    // del campo — de frente no suma nada y no se paga.
    let to_sun = normalize(grass_data.sun_direction);
    let view_dir = normalize(in.world_position.xyz - view.world_position.xyz);
    let through = max(dot(view_dir, to_sun), 0.0);
    // Al cuadrado para que sea un halo a contraluz y no un lavado general.
    let transmission = through * through * grass_data.sss_amount * factor;
    out.color = vec4<f32>(
        out.color.rgb + grass_data.tip_color.rgb * transmission,
        out.color.a,
    );

    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
#endif

/// El fragment del prepass: sólo el recorte de silueta de carta y, si hace
/// falta, la normal — nada de color, tinte ni PBR. Es la mitad barata a
/// propósito: correr el resto acá otra vez pagaría en el prepass el mismo
/// costo que el prepass existe para evitarle al pase principal.
///
/// **`AlphaToCoverage` con MSAA prendido no dispara el `MAY_DISCARD` del
/// prepass default** (`alpha_mode_pipeline_key` en `material.rs` de
/// bevy_pbr: con MSAA ≠ `Off` cae a `BLEND_ALPHA_TO_COVERAGE`, no
/// `MAY_DISCARD`) — y aunque lo disparara, ese camino lee `base_color` de
/// una textura, no la silueta procedural de acá. Sin este fragment propio,
/// el prepass escribiría profundidad para el cuadrado entero de cada carta
/// en vez de su silueta recortada.
#ifdef PREPASS_PIPELINE
@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
    let is_card_blade = blade_is_card(length(in.world_position.xz - grass_data.focus_xz), fract(in.uv_b.y));
    if is_card_blade {
        let card_texture_uv = card_uv(in);
        let card_texture_dx = dpdx(card_texture_uv);
        let card_texture_dy = dpdy(card_texture_uv);
        if sample_card_albedo(card_texture_uv, card_texture_dx, card_texture_dy).a < 0.5 {
            discard;
        }
    }

    var out: FragmentOutput;
#ifdef NORMAL_PREPASS
    out.normal = vec4<f32>(normalize(in.world_normal) * 0.5 + vec3<f32>(0.5), 1.0);
#endif
    return out;
}
#endif
