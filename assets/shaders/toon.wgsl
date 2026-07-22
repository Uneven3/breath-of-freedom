// Toon/cel shading over the standard PBR pipeline: lighting (sun, ambient,
// shadow maps) is computed normally, then its luminance snaps to a small
// number of discrete bands — Zelda-style stepped light instead of continuous
// gradients. Hue is preserved, so the day/night cycle's warm/cool light
// still reads through the bands.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
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
}
#endif

struct ToonExtension {
    bands: u32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> toon: ToonExtension;

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);

    // Quantize illumination *relative to the albedo*. Quantizing absolute lit
    // luminance made different materials collapse into the same global band,
    // visually merging adjacent surfaces. The ratio keeps their base-color
    // separation while still turning continuous light into hard steps.
    let lit = out.color.rgb;
    let luma_weights = vec3<f32>(0.2126, 0.7152, 0.0722);
    let albedo_luma = dot(pbr_input.material.base_color.rgb, luma_weights);
    let lit_luma = dot(lit, luma_weights);
    let illumination = lit_luma / max(albedo_luma, 1e-5);
    let bands = max(f32(toon.bands), 1.0);
    let stepped = (floor(max(illumination, 0.0) * bands) + 0.5) / bands;
    let factor = stepped / max(illumination, 1e-5);
    var color = lit * factor;

    // Ink edge: darken grazing view angles so silhouettes separate from
    // their surroundings (reads as a soft outline on curved surfaces).
    let ndotv = saturate(dot(pbr_input.N, pbr_input.V));
    let edge = 1.0 - smoothstep(0.12, 0.32, ndotv);
    color *= 1.0 - edge * 0.55;

    out.color = vec4<f32>(color, out.color.a);

    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif

    return out;
}
