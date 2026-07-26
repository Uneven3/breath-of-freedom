#import bevy_pbr::forward_io::{Vertex, VertexOutput, FragmentOutput}
#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_position_local_to_world, mesh_normal_local_to_world}

struct GrassUniform {
    root_color: vec4<f32>,
    tip_color: vec4<f32>,
    sun_direction: vec3<f32>,
    sss_amount: f32,
    time: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> grass_data: GrassUniform;

@group(#{MATERIAL_BIND_GROUP}) @binding(101)
var interaction_texture: texture_2d<f32>;

@group(#{MATERIAL_BIND_GROUP}) @binding(102)
var interaction_sampler: sampler;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    var world_from_local = get_world_from_local(vertex.instance_index);
    var world_position = mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    
    out.position = world_position;
    out.world_position = world_position;
    out.world_normal = mesh_normal_local_to_world(vertex.normal, vertex.instance_index);
    out.uv = vertex.uv;
    out.color = vertex.color;
    
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;
    let factor = clamp(in.uv.y, 0.0, 1.0);
    let color = mix(grass_data.root_color, grass_data.tip_color, factor);
    
    out.color = color;
    return out;
}
