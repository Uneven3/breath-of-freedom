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
// El prepass sigue usando el vertex shader por defecto. Hoy no hay prepass
// activo, pero desde que este vértice mueve la posición (viento), el día que se
// active hay que declarar `prepass_vertex_shader` con el mismo desplazamiento o
// la profundidad no va a coincidir con lo que se ve.

#import bevy_pbr::{
    pbr_types,
    pbr_functions::alpha_discard,
    pbr_fragment::pbr_input_from_standard_material,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    mesh_functions,
    mesh_view_bindings::view,
    view_transformations::position_world_to_clip,
}
#endif

struct GrassUniform {
    root_color: vec4<f32>,
    tip_color: vec4<f32>,
    sun_direction: vec3<f32>,
    sss_amount: f32,
    time: f32,
    focus_xz: vec2<f32>,
    growth_ramp: f32,
    growth_spread: f32,
    growth_sink: f32,
    wind_dir: vec2<f32>,
    wind_strength: f32,
    wind_speed: f32,
    tint_variation: f32,
    gradient_bias: f32,
    growth_start: f32,
    debug_view: u32,
    blade_width: f32,
    ring_reaches_a: vec4<f32>,
    ring_reaches_b: vec4<f32>,
    ring_chunks_a: vec4<f32>,
    ring_chunks_b: vec4<f32>,
    ring_cards_a: vec4<f32>,
    ring_cards_b: vec4<f32>,
    card_half_width: f32,
    record_stride: u32,
    blade_root_sink: f32,
    ring_colors: array<vec4<f32>, 8>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> grass_data: GrassUniform;

@group(#{MATERIAL_BIND_GROUP}) @binding(101)
var interaction_texture: texture_2d<f32>;

@group(#{MATERIAL_BIND_GROUP}) @binding(102)
var interaction_sampler: sampler;

/// Altura normalizada del vértice a lo largo de la brizna: 0 en la raíz, 1 en
/// la punta. Una sola definición, porque el viento, el degradado y el aplastado
/// del Paso 9 multiplican todos por exactamente esto.
fn blade_height_factor(uv: vec2<f32>) -> f32 {
    return clamp(uv.y, 0.0, 1.0);
}

/// Cuánto de su altura conserva una brizna a esta distancia de la cámara.
///
/// Es **crecimiento, no transparencia**: la brizna se encoge hacia su propia
/// raíz y desaparece siendo geometría, sin blending, sin `discard` y sin orden
/// de dibujo — o sea sin apagar el early-Z, que en un GPU tile-based es tirar la
/// ventaja principal del chip (ley 3 de `BOTWGrass.md`).
///
/// La banda es del borde de **su** anillo, no del anillo más lejano: los
/// anillos internos también ruedan, y sin banda propia sus chunks nacían
/// enteros y de una. `ring_reach` viaja por vértice justamente para esto.
///
/// **Y son dos números, no uno, porque son dos fenómenos distintos.** Hasta el
/// 2026-08-06 una sola constante gobernaba las dos cosas y por eso no había
/// forma de arreglar el crecimiento: acortarla las acortaba a las dos.
///
/// - `growth_ramp` es lo que tarda **una** brizna en pasar de nada a entera.
///   Corta. Una brizna sola creciendo es imperceptible; lo que se percibe es
///   *que todas crezcan juntas*.
/// - `growth_spread` es en cuántos metros se reparten los **umbrales** de las
///   distintas briznas, corridos por su hash. Largo. Esto es lo que convierte
///   una ola que avanza con el jugador en un raleo gradual hacia el borde: a
///   cada distancia sobrevive una fracción distinta del anillo, y lo que el ojo
///   lee es densidad que baja con la distancia, que es lo que hace un campo de
///   verdad.
///
/// Con la rampa corta metida dentro de una dispersión larga, en ningún momento
/// hay una franja donde todo esté creciendo a la vez — que era exactamente el
/// artefacto reportado jugando.
/// El borde **interno** del anillo al que pertenece esta brizna: el alcance más
/// grande que sea menor que el suyo.
///
/// Es lo que hace que la ley 1/d sea continua entre anillos. Anclada en un punto
/// global, cada anillo entrega menos de lo que le toca en su mitad interna y
/// deja un escalón — el artefacto que sobrevivió a toda la sesión del
/// 2026-08-06. Anclada acá, la densidad superviviente es `C/d` en todas partes y
/// el anillo pasa a decidir sólo el tamaño de chunk.
fn ring_reaches() -> array<f32, 8> {
    return array<f32, 8>(
        grass_data.ring_reaches_a.x, grass_data.ring_reaches_a.y,
        grass_data.ring_reaches_a.z, grass_data.ring_reaches_a.w,
        grass_data.ring_reaches_b.x, grass_data.ring_reaches_b.y,
        grass_data.ring_reaches_b.z, grass_data.ring_reaches_b.w,
    );
}

/// Si el anillo de esta primitiva la abre mirando a la cámara.
///
/// Por anillo y no por shader def: los anillos comparten material —uno solo para
/// toda la pradera, que es lo que evita duplicar los draws— así que la forma no
/// puede ser una variante de pipeline.
fn ring_is_card(reach: f32) -> bool {
    var flags = array<f32, 8>(
        grass_data.ring_cards_a.x, grass_data.ring_cards_a.y,
        grass_data.ring_cards_a.z, grass_data.ring_cards_a.w,
        grass_data.ring_cards_b.x, grass_data.ring_cards_b.y,
        grass_data.ring_cards_b.z, grass_data.ring_cards_b.w,
    );
    var reaches = ring_reaches();
    for (var i = 0; i < 8; i = i + 1) {
        if abs(reaches[i] - reach) < 0.5 {
            return flags[i] > 0.5;
        }
    }
    return false;
}

/// Hasta qué altura la carta es opaca en toda su anchura. Por debajo es masa
/// llena; por encima se abre en puntas.
const CARD_BASE_FILL: f32 = 0.28;
/// La punta más baja que puede tener un diente, como fracción de la altura de la
/// carta. Muy bajo el borde lee como sierra; muy alto, como el bloque de antes.
///
/// **0,75 salió midiendo, no de ojo.** Con 0,55 la banda de 45-64 m caía a 95,9%
/// aunque la densidad ya compensara el área recortada, y la razón es que a esa
/// distancia el suelo se ve casi de canto: lo que lo tapa es la **altura** de la
/// masa, no su ancho, y recortar puntas la baja. Subir el piso de los dientes
/// devuelve altura sin devolver el borde plano: 97,4% *(a, 2026-08-07)*.
const CARD_TIP_MIN: f32 = 0.75;

/// La silueta de la carta: qué altura tiene la masa de pasto en esta columna.
///
/// Devuelve la altura normalizada (0 en la base, 1 en el tope de la carta) hasta
/// la que hay pasto en la coordenada horizontal `u` ∈ [-1, 1]. Fuera de eso, la
/// carta no dibuja: es lo que la convierte de un rectángulo con el borde plano
/// —que a media distancia se lee como una hilera de bloques— en un grupo de
/// puntas.
///
/// **Dos capas de dientes triangulares, no una.** Una sola deja huecos hasta la
/// base entre diente y diente, que a esta distancia lee como un peine. Dos capas
/// de períodos distintos, desfasadas, y el máximo de las dos: las bases se
/// solapan y lo que queda irregular es sólo el borde de arriba, que es donde una
/// masa de pasto de verdad es irregular.
///
/// Sin textura y sin `pow`: el frame es fill-bound *(a, 2026-08-06)* y esto se
/// paga por fragmento. Son dos `fract`, dos `abs` y un `max`.
///
/// **Si cambiás estos números, actualizá `CARD_SILHOUETTE_AREA` en `grass.rs`.**
/// Es la integral de esta función, y de ella sale cuántas cartas se plantan: con
/// la fracción vieja el campo lejano queda ralo y sólo se nota midiendo.
fn card_silhouette(u: f32, phase: f32) -> f32 {
    // **La fase es por carta, y no es un adorno.** Todas las cartas se abren
    // mirando a la cámara, así que quedan paralelas entre sí: con una silueta
    // idéntica, los huecos de una caen exactamente sobre los de la que tiene
    // detrás y el suelo se ve por el mismo lugar en todas. Medido *(a)*: sin
    // fase, la banda de 45-64 m se quedaba en 95,4% donde el área de la silueta
    // predecía 99%, y ese hueco es justamente la correlación que Poisson supone
    // que no existe.
    //
    // El período de cada capa, en briznas por carta. Una carta mide 0,5 m y una
    // brizna 5,5 cm, así que nueve entran justas; siete y cinco dejan que las dos
    // capas se crucen sin repetir el mismo diente.
    let a = card_teeth(u, 7.0, phase);
    let b = card_teeth(u, 5.0, 0.37 + phase * 1.7);
    // El piso: abajo la masa está llena. Sin esto la carta se abre hasta la
    // tierra y deja pasar el suelo entre las puntas.
    return max(max(a, b), CARD_BASE_FILL);
}

/// Una capa de dientes: `count` puntas a lo ancho, desplazadas por `offset`, cada
/// una con su altura propia.
fn card_teeth(u: f32, count: f32, offset: f32) -> f32 {
    let s = (u * 0.5 + 0.5) * count + offset;
    let column = floor(s);
    let across = fract(s);
    // La altura de esta punta. `fract(·)` de un múltiplo grande descorrelaciona
    // columnas vecinas sin una tabla ni un seno.
    let height = CARD_TIP_MIN + fract(column * 0.618034) * (1.0 - CARD_TIP_MIN);
    // Triángulo: sube hasta el centro de la columna y baja.
    return height * (1.0 - abs(across * 2.0 - 1.0));
}

fn ring_inner(reach: f32) -> f32 {
    var reaches = ring_reaches();
    var inner = 0.0;
    for (var i = 0; i < 8; i = i + 1) {
        if reaches[i] < reach && reaches[i] > inner {
            inner = reaches[i];
        }
    }
    return inner;
}

fn blade_growth(world_xz: vec2<f32>, ring_reach: f32, blade_hash: f32) -> f32 {
    let distance = length(world_xz - grass_data.focus_xz);
    // Dos umbrales, y la brizna muere en el primero que llegue.
    //
    // El de la **ley**: `start / (1 - hash)` reparte los umbrales de modo que la
    // fracción sobreviviente a distancia `d` sea exactamente `start / d` — la ley
    // 1/d que deriva `BOTWGrass.md`, continua en vez de concentrada al borde.
    //
    // El del **borde**: la ley sola deja ~25% de las briznas vivas al llegar al
    // alcance del anillo, y ahí se cortan de golpe. Medido: la escalera se movía
    // de los 10-16 m al borde exacto. Esta banda las apaga antes de llegar.
    //
    // Hashes distintos a propósito: con el mismo, las que la ley perdona son
    // justo las que el borde mata primero, y el reparto se vuelve un escalón
    // otra vez.
    let edge_hash = fract(blade_hash * 7.1234 + 0.371);
    let inner = ring_inner(ring_reach);
    let anchor = max(inner, grass_data.growth_start);
    let by_law = anchor / max(1.0 - blade_hash, 1e-4);
    let by_edge = ring_reach - grass_data.growth_spread * edge_hash;
    let ends = min(by_law, by_edge);
    let starts = ends - grass_data.growth_ramp;
    // **Y acá NO va un borde interno**, aunque los anillos se pisen. Probado y
    // medido el 2026-08-07: recortar cada anillo por adentro deja un pozo en
    // cada frontera, porque el anillo de afuera nace donde el de adentro muere y
    // son **dos juegos de briznas distintos** — no hay forma de que una releve a
    // la otra. La banda de solapamiento es lo que hoy tapa esa costura. Lo que
    // la reemplaza es la reescritura de praderas anidadas de `BOTWGrass.md`, no
    // un recorte. El `inner` se sigue usando arriba, para anclar la ley.
    return 1.0 - smoothstep(starts, ends, distance);
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

/// El anillo de esta brizna, como índice de la paleta.
///
/// Sale del alcance que ya viaja empaquetado en `uv1.y`, así que **ninguna de
/// estas vistas cuesta un byte por vértice ni obliga a rehornear la pradera**.
/// Ése es el motivo por el que se pueden encender jugando: lo que cambia es lo
/// que el shader pinta, no lo que se dibuja.
fn ring_slot(reach: f32) -> u32 {
    var reaches = ring_reaches();
    for (var i = 0u; i < 8u; i = i + 1u) {
        if abs(reaches[i] - reach) < 0.5 {
            return i;
        }
    }
    return 7u; // El casillero de "ninguno", que en la paleta es el gris.
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
    ring_reach: f32,
    blade_hash: f32,
    blade_height: f32,
    metres_per_pixel: f32,
) -> vec3<f32> {
    let view = grass_data.debug_view;
    if view == DEBUG_OFF || view == DEBUG_MEASURE {
        return base;
    }
    let slot = ring_slot(ring_reach);
    var tint = vec3<f32>(1.0);
    if view == DEBUG_RING {
        tint = grass_data.ring_colors[slot].rgb;
    } else if view == DEBUG_CHUNK {
        // Una celda es una malla y un draw call: esta vista es también el mapa
        // de draws de la pradera.
        let cell = floor(world_xz / ring_chunk_m(slot));
        tint = pastel(cell * 13.0 + f32(slot) * 101.0, 0.45);
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
        // inventa colores, los recibe.
        let grown = blade_growth(world_xz, ring_reach, blade_hash);
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
/// Es el `Vertex` de `bevy_pbr::forward_io` **más `vertex_index`**, que allá sólo
/// existe bajo `#ifdef MORPH_TARGETS` (`forward_io.wgsl:26-28`) y que acá hace
/// falta siempre: es lo que le dice a una brizna cuál de los registros del
/// buffer le toca. Es un builtin, así que no consume un `location`.
///
/// **Y no lleva `normal`, aunque el de Bevy la declare en el location 1.** Ésa es
/// la trampa de escribir el struct a mano: allá cada campo vive dentro de su
/// `#ifdef`, así que el struct se encoge solo hasta calzar con los atributos que
/// la malla realmente tiene. Acá no hay nada que lo encoja, y la pradera **no
/// hornea normales** —la suya es +Y, calculada— así que declararla pide a la
/// etapa anterior un `Float32x3` que nadie provee y el pipeline no compila. El
/// error de validación de wgpu lo dice con el número de location, que es lo
/// único que hizo esto barato de encontrar.
struct GrassVertex {
    @builtin(instance_index) instance_index: u32,
    @builtin(vertex_index) vertex_index: u32,
    @location(0) position: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) uv_b: vec2<f32>,
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

/// Cuántos vértices gasta una carta. Cuatro, como la hoja horneada.
const VERTICES_PER_CARD: u32 = 4u;

/// El hash de una brizna a partir de dónde está parada.
///
/// **De la posición y no del índice del registro**, y no es un detalle: los
/// casilleros se reasignan cuando la grilla rueda, así que un hash atado al
/// casillero cambiaría el umbral de crecimiento de una brizna mientras el
/// jugador camina — que es el artefacto que todo este sistema evita.
fn hash_position(world_xz: vec2<f32>) -> f32 {
    return fract(sin(dot(world_xz, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

#ifndef PREPASS_PIPELINE
@vertex
fn vertex(vertex: GrassVertex) -> VertexOutput {
    var out: VertexOutput;

    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    var world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(vertex.position, 1.0),
    );

    var height_factor = blade_height_factor(vertex.uv);
    var blade_hash = abs(vertex.uv_b.x);
    var side = sign(vertex.uv_b.x);
    // `uv1.y` lleva dos números en uno: el alcance del anillo en metros enteros
    // y la altura de la brizna en la fracción.
    var ring_reach = floor(vertex.uv_b.y);
    var blade_height = fract(vertex.uv_b.y);

    // **El nivel que se lee del buffer.** Su malla es un índice compartido: sus
    // vértices no llevan posición útil, sólo su lugar dentro de la primitiva. El
    // registro se localiza con el `MeshTag` del chunk —su casillero dentro del
    // nivel, con stride fijo— más cuál de las primitivas de ese chunk es ésta.
    if grass_data.record_stride > 0u {
        let slot = mesh_functions::get_tag(vertex.instance_index);
        let record = blade_records[
            slot * grass_data.record_stride + vertex.vertex_index / VERTICES_PER_CARD
        ].base_and_shape;
        let corner = vertex.vertex_index % VERTICES_PER_CARD;

        ring_reach = floor(record.w);
        blade_height = fract(record.w);
        blade_hash = hash_position(record.xy);
        // Los cuatro vértices nacen en el centro de la base y el shader los abre;
        // sólo hace falta saber de qué esquina es éste. El orden es el mismo que
        // hornea `BladeShape::Card`: abajo-izq, abajo-der, arriba-der, arriba-izq.
        side = select(-1.0, 1.0, corner == 1u || corner == 2u);
        height_factor = select(0.0, 1.0, corner >= 2u);
        // La base, hundida como la hornea el otro camino: en el suelo mismo la
        // primitiva deja ver tierra donde nace.
        world_position = vec4<f32>(
            record.x,
            record.z - grass_data.blade_root_sink,
            record.y,
            1.0,
        );
        out.uv = vec2<f32>(record.z, height_factor);
        out.uv_b = vec2<f32>(side * blade_hash, record.w);
    } else {
        out.uv = vertex.uv;
        out.uv_b = vertex.uv_b;
    }

    // **La carta se abre acá.** Sus cuatro vértices vienen horneados en el mismo
    // punto —el centro de la base— y se separan contra el eje derecho de la
    // cámara, así que siempre da la cara. Una carta representa la masa de
    // decenas de briznas: si quedara de canto dejaría un hueco de ese tamaño, y
    // por eso ésta sí gira mientras la brizna cercana no.
    //
    // El eje derecho sale de la fila 0 de la matriz de vista, que es la que
    // lleva el `right` de la cámara en espacio de mundo, aplanado a horizontal
    // para que la carta se quede parada en vez de inclinarse con el cabeceo.
    if ring_is_card(ring_reach) {
        let camera_right = normalize(vec3<f32>(view.view_from_world[0].x, 0.0, view.view_from_world[2].x));
        world_position = vec4<f32>(
            world_position.xyz
                + camera_right * (side * grass_data.card_half_width)
                + vec3<f32>(0.0, height_factor * blade_height, 0.0),
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
    let ground_y = vertex.uv.x;
    world_position.y = mix(
        ground_y - grass_data.growth_sink,
        world_position.y,
        blade_growth(world_position.xz, ring_reach, blade_hash),
    );

    out.world_position = world_position;
    // Clip space, no world space: la posición que sale del vertex shader es la
    // que el rasterizador proyecta.
    out.position = position_world_to_clip(world_position.xyz);
    out.world_normal = blade_normal(side, world_position.xz);
    // **La carta cambia lo que lleva en `uv_b.x`**: en vez del hash con el lado
    // en el signo, el lado a secas. Interpolado a lo ancho da −1 en un borde y
    // +1 en el otro, que es la coordenada que la silueta necesita y que de otro
    // modo no existiría — el hash es un número distinto por primitiva, así que
    // no se puede normalizar en el fragment sin mandarlo aparte.
    //
    // El hash ya hizo su trabajo acá arriba, así que no se pierde nada más que la
    // vista `brizna` sobre las cartas, que pasa a ser un degradado a lo ancho.
    if ring_is_card(ring_reach) {
        out.uv_b = vec2<f32>(side, out.uv_b.y);
    }
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif

    return out;
}
#endif

@fragment
fn fragment(
    vertex_output: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var in = vertex_output;
    // Cuánto mundo cubre un píxel acá. Fuera de todo branch, porque una derivada
    // en control de flujo no uniforme no está definida.
    let metres_per_pixel = length(fwidth(in.world_position.xz));

    // **El recorte de la carta, antes que nada.** Va arriba de todo y no junto al
    // `alpha_discard` del final por una razón concreta: las vistas de diagnóstico
    // salen antes del PBR, así que un descarte puesto abajo dejaría a `medir`
    // contando la carta entera —el rectángulo que ya no se dibuja— y el medidor
    // informaría una cobertura que la imagen no tiene. El instrumento tiene que
    // ver lo mismo que la pantalla.
    // La fase sale de `fract(uv1.y)` —la altura de la carta, idéntica en sus
    // cuatro vértices— por el mismo camino que el tinte por brizna: es el único
    // identificador que el vértice ya carga.
    if ring_is_card(floor(in.uv_b.y))
        && blade_height_factor(in.uv)
            > card_silhouette(in.uv_b.x, fract(fract(in.uv_b.y) * 91.0)) {
        discard;
    }

#ifndef PREPASS_PIPELINE
    // **Sub-píxel**: en bandas planas y exactas, no en una rampa.
    //
    // Una rampa continua se mira; una banda se **cuenta**. La pregunta que esta
    // vista existe para contestar —a qué distancia una brizna deja de resolverse—
    // se contestaba comparando colores a ojo, que es justo lo que esta sesión
    // vino a sacar del medio. Con tres bandas exactas, `shot_stats.py` dice qué
    // fracción del pasto está por debajo de un píxel.
    if grass_data.debug_view == DEBUG_SUBPIXEL {
        // El ancho es el de **esta** primitiva, no el de una brizna. Una carta
        // mide 0,5 m contra los 5,5 cm de una brizna: medirlas con la misma vara
        // las reportaba nueve veces más finas de lo que son, que es exactamente
        // la clase de error que esta vista existe para cazar.
        var width = grass_data.blade_width;
        if ring_is_card(floor(in.uv_b.y)) {
            width = grass_data.card_half_width * 2.0;
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
            grass_data.ring_colors[ring_slot(floor(in.uv_b.y))].rgb,
            1.0,
        );
        return measured;
    }
#endif

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
        ),
        colour.a,
    );

    pbr_input.material.base_color = colour;
    pbr_input.material.base_color =
        alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef PREPASS_PIPELINE
    return deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);

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
#endif
}
