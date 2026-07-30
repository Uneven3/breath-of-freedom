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
