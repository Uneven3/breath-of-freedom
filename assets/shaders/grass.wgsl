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
    wind_dir: vec2<f32>,
    wind_strength: f32,
    wind_speed: f32,
    tint_variation: f32,
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
fn blade_growth(world_xz: vec2<f32>, ring_reach: f32, blade_hash: f32) -> f32 {
    let distance = length(world_xz - grass_data.focus_xz);
    // El umbral propio de esta brizna, en algún punto de la dispersión. Ninguna
    // sobrevive más allá del alcance de su anillo: con hash 0 el umbral es el
    // borde exacto.
    let ends = ring_reach - grass_data.growth_spread * blade_hash;
    let starts = ends - grass_data.growth_ramp;
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

#ifndef PREPASS_PIPELINE
@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    var world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(vertex.position, 1.0),
    );

    let height_factor = blade_height_factor(vertex.uv);
    let blade_hash = abs(vertex.uv_b.x);
    let side = sign(vertex.uv_b.x);
    // `uv1.y` lleva dos números en uno: el alcance del anillo en metros enteros
    // y la altura de la brizna en la fracción.
    let ring_reach = floor(vertex.uv_b.y);
    let blade_height = fract(vertex.uv_b.y);

    // Viento primero, sobre la posición horneada.
    let sway = wind_offset(world_position.xz, height_factor, blade_height, blade_hash);
    world_position = vec4<f32>(
        world_position.x + sway.x,
        world_position.y,
        world_position.z + sway.y,
        world_position.w,
    );

    // Y después el crecimiento por distancia, que colapsa hacia la raíz. `uv.x`
    // lleva la altura del suelo justamente para esto: sin ella el shader no sabe
    // dónde está la base y no puede encogerla hacia ella.
    let ground_y = vertex.uv.x;
    world_position.y = mix(
        ground_y,
        world_position.y,
        blade_growth(world_position.xz, ring_reach, blade_hash),
    );

    out.world_position = world_position;
    // Clip space, no world space: la posición que sale del vertex shader es la
    // que el rasterizador proyecta.
    out.position = position_world_to_clip(world_position.xyz);
    out.world_normal = blade_normal(side, world_position.xz);
    out.uv = vertex.uv;
    out.uv_b = vertex.uv_b;
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
    var colour = mix(grass_data.root_color, grass_data.tip_color, factor);

    // Variación por brizna: un corrimiento de tono y valor que rompe la lectura
    // de superficie única. Es de las cosas más baratas del sistema y de las que
    // más se notan — un campo de un solo verde se lee como alfombra por densa
    // que sea.
    let drift = (blade_hash - 0.5) * 2.0 * grass_data.tint_variation;
    colour = vec4<f32>(
        colour.r * (1.0 + drift * 0.6),
        colour.g * (1.0 + drift),
        colour.b * (1.0 + drift * 0.4),
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
