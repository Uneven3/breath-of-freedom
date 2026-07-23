#import bevy_pbr::forward_io::{VertexOutput, FragmentOutput}

struct OverdrawExtension {
    color: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> overdraw: OverdrawExtension;

@fragment
fn fragment(_in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;
    // AlphaMode::Add selects premultiplied-alpha blending. Alpha zero keeps
    // the destination intact; premultiplying RGB makes every fragment add one
    // fixed dose, so overlapping geometry becomes progressively brighter.
    out.color = vec4<f32>(overdraw.color.rgb * overdraw.color.a, 0.0);
    return out;
}
