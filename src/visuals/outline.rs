//! Post-process silhouette outlines (BotW-style): a fullscreen pass after
//! the main render detects edges in the depth and normal prepasses and inks
//! them, so shapes separate even when their fill colors match. Presentation
//! only — the camera opts in with [`OutlineSettings`] plus depth/normal
//! prepasses (and `Msaa::Off`, since the pass reads single-sample textures).

use bevy::core_pipeline::prepass::ViewPrepassTextures;
use bevy::core_pipeline::{Core3dSystems, FullscreenShader, schedule::Core3d};
use bevy::prelude::*;
use bevy::render::{
    RenderApp, RenderStartup,
    extract_component::{
        ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin,
        UniformComponentPlugin,
    },
    render_resource::{
        binding_types::{sampler, texture_2d, texture_depth_2d, uniform_buffer},
        *,
    },
    renderer::{RenderContext, RenderDevice, ViewQuery},
    view::ViewTarget,
};

const SHADER_ASSET_PATH: &str = "shaders/outline.wgsl";

/// Per-camera outline tuning; its presence turns the effect on.
#[derive(Component, Clone, Copy, ExtractComponent, ShaderType)]
pub struct OutlineSettings {
    /// Ink color.
    pub color: Vec3,
    /// Relative depth discontinuity (fraction of view distance) that counts
    /// as an edge.
    pub depth_threshold: f32,
    /// Normal disagreement (1 - dot) that counts as an edge.
    pub normal_threshold: f32,
    /// Sample offset in pixels — line weight.
    pub thickness: f32,
}

impl Default for OutlineSettings {
    fn default() -> Self {
        Self {
            color: Vec3::new(0.05, 0.05, 0.08),
            depth_threshold: 0.12,
            normal_threshold: 0.45,
            thickness: 1.0,
        }
    }
}

/// Benchmark knob: strips the ink pass *and* the two prepasses that only
/// exist to feed it, so an A/B measures the whole outline feature rather than
/// just its fullscreen pass. Restoring re-inserts all three.
pub(super) fn apply_outline_perf(
    mut commands: Commands,
    perf: Res<crate::perf::PerfToggles>,
    camera: Single<(Entity, Option<&OutlineSettings>), With<Camera3d>>,
) {
    if !perf.is_changed() {
        return;
    }
    let (camera, settings) = *camera;
    match (perf.outline, settings.is_some()) {
        (true, false) => {
            commands.entity(camera).try_insert((
                OutlineSettings::default(),
                bevy::core_pipeline::prepass::DepthPrepass,
                bevy::core_pipeline::prepass::NormalPrepass,
            ));
        }
        (false, true) => {
            commands
                .entity(camera)
                .remove::<OutlineSettings>()
                .remove::<bevy::core_pipeline::prepass::DepthPrepass>()
                .remove::<bevy::core_pipeline::prepass::NormalPrepass>();
        }
        _ => {}
    }
}

pub struct OutlinePlugin;

impl Plugin for OutlinePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractComponentPlugin::<OutlineSettings>::default(),
            UniformComponentPlugin::<OutlineSettings>::default(),
        ));
        app.add_systems(Update, apply_outline_perf);

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app.add_systems(RenderStartup, init_outline_pipeline);
        render_app.add_systems(Core3d, outline_system.in_set(Core3dSystems::PostProcess));
    }
}

#[derive(Resource)]
struct OutlinePipeline {
    layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    pipeline_id: CachedRenderPipelineId,
}

fn init_outline_pipeline(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    asset_server: Res<AssetServer>,
    fullscreen_shader: Res<FullscreenShader>,
    pipeline_cache: Res<PipelineCache>,
) {
    let layout = BindGroupLayoutDescriptor::new(
        "outline_bind_group_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                texture_depth_2d(),
                texture_2d(TextureSampleType::Float { filterable: true }),
                uniform_buffer::<OutlineSettings>(true),
            ),
        ),
    );
    let sampler = render_device.create_sampler(&SamplerDescriptor::default());
    let shader = asset_server.load(SHADER_ASSET_PATH);
    let vertex_state = fullscreen_shader.to_vertex_state();
    let pipeline_id = pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
        label: Some("outline_pipeline".into()),
        layout: vec![layout.clone()],
        vertex: vertex_state,
        fragment: Some(FragmentState {
            shader,
            targets: vec![Some(ColorTargetState {
                format: TextureFormat::Rgba8UnormSrgb,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
            ..default()
        }),
        ..default()
    });
    commands.insert_resource(OutlinePipeline {
        layout,
        sampler,
        pipeline_id,
    });
}

#[derive(Default)]
struct OutlineBindGroupCache {
    cached: Option<(TextureViewId, BindGroup)>,
}

fn outline_system(
    view: ViewQuery<(
        &ViewTarget,
        &ViewPrepassTextures,
        &DynamicUniformIndex<OutlineSettings>,
    )>,
    outline_pipeline: Option<Res<OutlinePipeline>>,
    pipeline_cache: Res<PipelineCache>,
    settings_uniforms: Res<ComponentUniforms<OutlineSettings>>,
    mut cache: Local<OutlineBindGroupCache>,
    mut ctx: RenderContext,
) {
    let Some(outline_pipeline) = outline_pipeline else {
        return;
    };
    let (view_target, prepass, settings_index) = view.into_inner();
    let Some(pipeline) = pipeline_cache.get_render_pipeline(outline_pipeline.pipeline_id) else {
        return;
    };
    let Some(settings_binding) = settings_uniforms.uniforms().binding() else {
        return;
    };
    let (Some(depth_view), Some(normal_view)) = (prepass.depth_view(), prepass.normal_view())
    else {
        return;
    };

    let post_process = view_target.post_process_write();

    // The bind group is rebuilt whenever the source view flips (see the
    // upstream custom_post_processing example for why this happens here).
    let bind_group = match &mut cache.cached {
        Some((texture_id, bind_group)) if post_process.source.id() == *texture_id => bind_group,
        cached => {
            let bind_group = ctx.render_device().create_bind_group(
                "outline_bind_group",
                &pipeline_cache.get_bind_group_layout(&outline_pipeline.layout),
                &BindGroupEntries::sequential((
                    post_process.source,
                    &outline_pipeline.sampler,
                    depth_view,
                    normal_view,
                    settings_binding.clone(),
                )),
            );
            let (_, bind_group) = cached.insert((post_process.source.id(), bind_group));
            bind_group
        }
    };

    let mut render_pass = ctx
        .command_encoder()
        .begin_render_pass(&RenderPassDescriptor {
            label: Some("outline_pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: post_process.destination,
                depth_slice: None,
                resolve_target: None,
                ops: Operations::default(),
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

    render_pass.set_pipeline(pipeline);
    render_pass.set_bind_group(0, bind_group, &[settings_index.index()]);
    render_pass.draw(0..3, 0..1);
}
