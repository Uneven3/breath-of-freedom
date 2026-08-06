// El material de la pradera: PBR estándar con el degradado raíz→punta movido
// del vértice al shader.
//
// Es un `ExtendedMaterial`, así que conserva luz, sombras, niebla y decals; lo
// único propio es de dónde sale el color base y de dónde sale la normal. Las
// tres cosas que este archivo hace, y por qué:
//
// 1. **La normal es una constante del sistema, no un atributo.** Toda brizna
//    apunta a +Y en sus cuatro vértices — una cara plana iluminada por su propia
//    normal se apaga en cuanto el sol pega de lado, y un campo entero apagándose
//    a la vez es el artefacto que más delata al pasto generado. Reconstruirla
//    acá borra 12 bytes por vértice sin cambiar un píxel.
// 2. **El color es una función pura de `uv.y`.** Los dos extremos ya viajan como
//    uniforms, así que hornear un `vec4` por vértice era pagar 16 bytes por un
//    `mix` de una línea.
// 3. **El vértice no toca la posición.** Cuando el viento entre (Paso 5), esto
//    deja de ser cierto y el prepass —que usa el vertex shader por defecto— se
//    desincronizaría; hoy no hay prepass activo, y ese día habrá que declarar
//    `prepass_vertex_shader` con el mismo desplazamiento.

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
    fade_start: f32,
    fade_end: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> grass_data: GrassUniform;

@group(#{MATERIAL_BIND_GROUP}) @binding(101)
var interaction_texture: texture_2d<f32>;

@group(#{MATERIAL_BIND_GROUP}) @binding(102)
var interaction_sampler: sampler;

/// Altura normalizada del vértice a lo largo de la brizna: 0 en la raíz, 1 en
/// la punta. Una sola definición, porque el viento y el aplastado del Paso 9
/// van a multiplicar por exactamente esto.
fn blade_height_factor(uv: vec2<f32>) -> f32 {
    return clamp(uv.y, 0.0, 1.0);
}

/// Cuánto de su altura conserva una brizna a esta distancia de la cámara.
///
/// Es **crecimiento, no transparencia**: la brizna se encoge hacia su propia
/// raíz y desaparece siendo geometría, sin blending, sin `discard` y sin orden
/// de dibujo — o sea sin apagar el early-Z, que en un GPU tile-based es tirar la
/// ventaja principal del chip (ley 3 de `BOTWGrass.md`). El chunk se descarta
/// recién después, con sus briznas ya en cero, así que nada aparece de golpe al
/// caminar.
fn blade_growth(world_xz: vec2<f32>) -> f32 {
    let distance = length(world_xz - grass_data.focus_xz);
    return 1.0 - smoothstep(grass_data.fade_start, grass_data.fade_end, distance);
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

    // La brizna se encoge hacia su raíz con la distancia. `uv.x` lleva la altura
    // del suelo bajo la brizna justamente para esto: sin ella el shader no sabe
    // dónde está la base y no puede colapsar hacia ella.
    let ground_y = vertex.uv.x;
    world_position.y = mix(ground_y, world_position.y, blade_growth(world_position.xz));

    out.world_position = world_position;
    // Clip space, no world space: la posición que sale del vertex shader es la
    // que el rasterizador proyecta.
    out.position = position_world_to_clip(world_position.xyz);
    // +Y constante (ver 1 arriba), transformada por si el chunk rota.
    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        vec3<f32>(0.0, 1.0, 0.0),
        vertex.instance_index,
    );
    out.uv = vertex.uv;
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

    let factor = blade_height_factor(in.uv);
    pbr_input.material.base_color = mix(grass_data.root_color, grass_data.tip_color, factor);
    pbr_input.material.base_color =
        alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef PREPASS_PIPELINE
    return deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
#endif
}
