// Silhouette outlines: Roberts-cross edge detection over the depth and
// normal prepasses, inked on top of the rendered frame. Depth edges are
// relative to view distance (no false lines on receding floors); normal
// edges catch creases between faces the depth test misses.

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
    let px = clamp(
        vec2<i32>(in.uv * vec2<f32>(dims)),
        vec2<i32>(1, 1),
        dims - vec2<i32>(2, 2),
    );
    let t = max(i32(settings.thickness), 1);

    // Roberts cross on linearized depth, relative to the center distance.
    let center = depth_at(px);
    let d_a = abs(depth_at(px + vec2(-t, -t)) - depth_at(px + vec2(t, t)));
    let d_b = abs(depth_at(px + vec2(t, -t)) - depth_at(px + vec2(-t, t)));
    let depth_edge = (d_a + d_b) / max(center, 1e-3);

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
