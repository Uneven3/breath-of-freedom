// Silhouette outlines: edge detection over the depth and normal prepasses,
// inked on top of the rendered frame.
//
// Depth edges use the *second* derivative, not the first. A Roberts cross
// measures how fast depth changes between neighbours, which on a receding
// floor is large and constant — dividing by view distance does not cancel it,
// because grazing angle makes it grow faster than distance does. Past some
// range every ground pixel crossed the threshold and the terrain inked solid,
// leaving a dark disc that followed the camera and read as a domed ceiling.
//
// A Laplacian is zero on any plane regardless of how steeply it recedes, and
// only spikes where depth actually breaks. That is what a silhouette is.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var texture_sampler: sampler;
@group(0) @binding(2) var depth_texture: texture_depth_2d;
@group(0) @binding(3) var normal_texture: texture_2d<f32>;

struct OutlineSettings {
    color: vec3<f32>,
    depth_threshold: f32,
    normal_threshold: f32,
    thickness: f32,
}
@group(0) @binding(4) var<uniform> settings: OutlineSettings;

// Bevy uses reverse infinite-Z with near = 0.1: view distance = near / raw.
fn linear_depth(raw: f32) -> f32 {
    return 0.1 / max(raw, 1e-6);
}

fn depth_at(px: vec2<i32>) -> f32 {
    return linear_depth(textureLoad(depth_texture, px, 0));
}

fn normal_at(px: vec2<i32>) -> vec3<f32> {
    return normalize(textureLoad(normal_texture, px, 0).xyz * 2.0 - 1.0);
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(screen_texture, texture_sampler, in.uv);

    let dims = vec2<i32>(textureDimensions(depth_texture));
    if dims.x < 3 || dims.y < 3 {
        return color;
    }
    let max_t = max((min(dims.x, dims.y) - 1) / 2, 1);
    let t = i32(clamp(settings.thickness, 1.0, f32(max_t)));
    let px = clamp(
        vec2<i32>(in.uv * vec2<f32>(dims)),
        vec2<i32>(t, t),
        dims - vec2<i32>(t + 1, t + 1),
    );

    // Laplacian of linearized depth, relative to the center distance so a
    // given break inks the same whether it is near or far.
    let center = depth_at(px);
    let dx = depth_at(px - vec2(t, 0)) + depth_at(px + vec2(t, 0)) - 2.0 * center;
    let dy = depth_at(px - vec2(0, t)) + depth_at(px + vec2(0, t)) - 2.0 * center;
    let depth_edge = (abs(dx) + abs(dy)) / max(center, 1e-3);

    // Roberts cross on world normals.
    let n_a = 1.0 - dot(normal_at(px + vec2(-t, -t)), normal_at(px + vec2(t, t)));
    let n_b = 1.0 - dot(normal_at(px + vec2(t, -t)), normal_at(px + vec2(-t, t)));
    let normal_edge = max(n_a, n_b);

    let edge = max(
        step(settings.depth_threshold, depth_edge),
        step(settings.normal_threshold, normal_edge),
    );

    return vec4<f32>(mix(color.rgb, settings.color, edge), color.a);
}
