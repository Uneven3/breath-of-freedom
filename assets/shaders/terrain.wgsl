#import bevy_pbr::{
    pbr_types,
    pbr_functions::alpha_discard,
    pbr_fragment::pbr_input_from_standard_material,
    decal::clustered::apply_decals,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::{
        STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT,
        STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
    },
}
#endif

struct TerrainExtension {
    debug: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var terrain_textures: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101)
var terrain_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(102)
var<uniform> terrain: TerrainExtension;

// Un binding por `#[uniform(n)]` del lado Rust, y no un struct que los agrupe:
// `AsBindGroup` le da a cada atributo su propio buffer, así que declararlos
// juntos acá le promete al pipeline un buffer de 48 bytes donde hay uno de 16 y
// la validación del device rechaza la pipeline entera al crearla.

// rgb = color de la raíz de una brizna, a = cuánto puede teñirse el suelo.
@group(#{MATERIAL_BIND_GROUP}) @binding(103)
var<uniform> grass_tint: vec4<f32>;

// x = cos(pendiente máxima), y = cos(pendiente empinada),
// z = máscara de capas con pasto, w = libre. Ver `visuals/grass_cover.rs`.
@group(#{MATERIAL_BIND_GROUP}) @binding(104)
var<uniform> grass_rules: vec4<f32>;

/// Cuánto pasto le toca a este punto del suelo — la misma regla que usa el
/// estampador, evaluada acá con lo que el fragment tiene a mano: la capa que
/// dice el color de vértice y la inclinación que dice la normal.
///
/// Tienen que coincidir: el sentido del tinte es que el campo no tenga borde, y
/// un borde es exactamente lo que aparece si las dos mitades no se ponen de
/// acuerdo sobre dónde termina el pasto.
fn grass_coverage(layer: i32, world_normal: vec3<f32>) -> f32 {
    let mask = u32(round(grass_rules.z));
    if (mask & (1u << u32(max(layer, 0)))) == 0u {
        return 0.0;
    }
    // El coseno con +Y *baja* cuando la pendiente sube, así que el máximo de
    // pendiente es el mínimo de coseno.
    let upness = normalize(world_normal).y;
    return smoothstep(grass_rules.x, grass_rules.y, upness);
}

fn kind_debug_color(layer: i32) -> vec3<f32> {
    switch layer {
        case 0: { return vec3<f32>(121.0, 81.0, 58.0) / 255.0; }
        case 1: { return vec3<f32>(125.0, 130.0, 140.0) / 255.0; }
        case 2: { return vec3<f32>(79.0, 155.0, 69.0) / 255.0; }
        default: { return vec3<f32>(216.0, 194.0, 116.0) / 255.0; }
    }
}

fn property_debug_color(enabled: bool, on_color: vec3<f32>) -> vec3<f32> {
    if enabled {
        return on_color;
    }
    return vec3<f32>(0.025, 0.025, 0.03);
}

@fragment
fn fragment(
    vertex_output: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var in = vertex_output;
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    let semantics = in.color;
    let layer = i32(round(semantics.r * 255.0));
    let debug_mode = u32(round(terrain.debug.x));
    var color = textureSample(terrain_textures, terrain_sampler, in.uv, layer).rgb;

    // El terreno es el LOD más lejano del pasto: donde crece pasto, el suelo ya
    // lleva su color, así que el campo no termina en una línea — termina en un
    // piso que ya combina.
    let coverage = grass_coverage(layer, in.world_normal);
    color = mix(color, grass_tint.rgb, coverage * grass_tint.a);

    if debug_mode == 1u {
        color = kind_debug_color(layer);
    } else if debug_mode == 2u {
        color = property_debug_color(semantics.g > 0.5, vec3<f32>(1.0, 0.05, 0.03));
    } else if debug_mode == 3u {
        color = property_debug_color(semantics.b > 0.5, vec3<f32>(1.0, 0.3, 0.02));
    } else if debug_mode == 4u {
        color = property_debug_color(semantics.a > 0.5, vec3<f32>(0.55, 1.0, 0.03));
    }

    pbr_input.material.base_color = vec4<f32>(color, 1.0);
    pbr_input.material.base_color =
        alpha_discard(pbr_input.material, pbr_input.material.base_color);
    apply_decals(&pbr_input);

    if debug_mode != 0u {
        pbr_input.material.flags |= pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT;
        pbr_input.material.flags &= ~pbr_types::STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT;
    }

#ifdef PREPASS_PIPELINE
    return deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    if (pbr_input.material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
#endif
}
