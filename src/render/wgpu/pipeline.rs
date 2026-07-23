//! wgpu render pipelines for burin primitives.

/// Rectangle pipeline: renders colored quads (for backgrounds, rects, etc.).
pub fn create_rect_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    sample_count: u32,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("rect shader"),
        source: wgpu::ShaderSource::Wgsl(RECT_SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("rect layout"),
        bind_group_layouts: &[],
        ..Default::default()
    });
    let vb_desc = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<RectVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 8,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 24,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 32,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 48,
                shader_location: 4,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 56,
                shader_location: 5,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 60,
                shader_location: 6,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 68,
                shader_location: 7,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 76,
                shader_location: 8,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 92,
                shader_location: 9,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 108,
                shader_location: 10,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 124,
                shader_location: 11,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 128,
                shader_location: 12,
            },
        ],
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("rect pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(vb_desc)],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}

/// Gradient pipeline: renders quads with linear gradient fills (4 stops max).
pub fn create_gradient_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    sample_count: u32,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("gradient shader"),
        source: wgpu::ShaderSource::Wgsl(GRADIENT_SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("gradient layout"),
        bind_group_layouts: &[],
        ..Default::default()
    });
    let vb_desc = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<GradientVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 8,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 16,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 32,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 40,
                shader_location: 4,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 44,
                shader_location: 5,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 52,
                shader_location: 6,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 60,
                shader_location: 7,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 76,
                shader_location: 8,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 92,
                shader_location: 9,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 108,
                shader_location: 10,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 124,
                shader_location: 11,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 140,
                shader_location: 12,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 156,
                shader_location: 13,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 172,
                shader_location: 14,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 180,
                shader_location: 15,
            },
        ],
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("gradient pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(vb_desc)],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}

pub fn create_text_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    bind_group_layout: &wgpu::BindGroupLayout,
    sample_count: u32,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("text shader"),
        source: wgpu::ShaderSource::Wgsl(TEXT_SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("text layout"),
        bind_group_layouts: &[Some(bind_group_layout)],
        ..Default::default()
    });
    let vb_desc = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<TextVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 8,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 16,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 32,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 48,
                shader_location: 4,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 56,
                shader_location: 5,
            },
        ],
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("text pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(vb_desc)],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}

pub fn create_text_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("text bind group layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

pub fn create_atlas_bind_group(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    texture_view: &wgpu::TextureView,
    sampler: Option<&wgpu::Sampler>,
) -> wgpu::BindGroup {
    let local_sampler;
    let samp = match sampler {
        Some(s) => s,
        None => {
            local_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("glyph sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            });
            &local_sampler
        }
    };
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("text bind group"),
        layout: bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(samp),
            },
        ],
    })
}

// ── Vertex formats ──

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RectVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub size: [f32; 2],
    pub radii: [f32; 4],
    pub local: [f32; 2],
    pub stroke_width: f32,
    pub world_pos: [f32; 2],
    pub shadow_offset: [f32; 2],
    pub clip_rect: [f32; 4],
    pub clip_radius: [f32; 4],
    pub shadow_color: [f32; 4],
    pub shadow_blur_radius: f32,
    pub is_shadow: f32,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GradientVertex {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub radii: [f32; 4],
    pub local: [f32; 2],
    pub stroke_width: f32,
    pub world_pos: [f32; 2],
    pub shadow_offset: [f32; 2],
    pub clip_rect: [f32; 4],
    pub clip_radius: [f32; 4],
    pub color_0: [f32; 4],
    pub color_1: [f32; 4],
    pub color_2: [f32; 4],
    pub color_3: [f32; 4],
    pub count_offsets: [f32; 4],
    pub dir_start: [f32; 2],
    pub dir_end: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TextVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub clip_rect: [f32; 4],
    pub clip_radius: [f32; 4],
    pub world_pos: [f32; 2],
    pub opacity: f32,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BlitVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PathVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub world_pos: [f32; 2],
    pub clip_rect: [f32; 4],
    pub clip_radius: [f32; 4],
}

// ── Shaders ──

const RECT_SHADER: &str = r#"
struct VertexInput {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) size: vec2<f32>,
    @location(3) radii: vec4<f32>,
    @location(4) local: vec2<f32>,
    @location(5) stroke: f32,
    @location(6) world_pos: vec2<f32>,
    @location(7) shadow_offset: vec2<f32>,
    @location(8) clip_rect: vec4<f32>,
    @location(9) clip_radius: vec4<f32>,
    @location(10) shadow_color: vec4<f32>,
    @location(11) shadow_blur_radius: f32,
    @location(12) is_shadow: f32,
}
struct VertexOutput {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) size: vec2<f32>,
    @location(2) radii: vec4<f32>,
    @location(3) local: vec2<f32>,
    @location(4) stroke: f32,
    @location(5) world_pos: vec2<f32>,
    @location(6) shadow_offset: vec2<f32>,
    @location(7) clip_rect: vec4<f32>,
    @location(8) clip_radius: vec4<f32>,
    @location(9) shadow_color: vec4<f32>,
    @location(10) shadow_blur_radius: f32,
    @location(11) is_shadow: f32,
}

@vertex fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.pos = vec4(in.pos, 0.0, 1.0);
    out.color = in.color;
    out.size = in.size;
    out.radii = in.radii;
    out.local = in.local;
    out.stroke = in.stroke;
    out.world_pos = in.world_pos;
    out.shadow_offset = in.shadow_offset;
    out.clip_rect = in.clip_rect;
    out.clip_radius = in.clip_radius;
    out.shadow_color = in.shadow_color;
    out.shadow_blur_radius = in.shadow_blur_radius;
    out.is_shadow = in.is_shadow;
    return out;
}

fn rounded_box_sdf(p: vec2<f32>, half_size: vec2<f32>, corners: vec4<f32>) -> f32 {
    let box_half = select(corners.xw, corners.yz, p.x > 0.0);
    let corner = select(box_half.x, box_half.y, p.y > 0.0);
    let q = abs(p) - half_size + corner;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2(0.0))) - corner;
}

fn element_alpha(local: vec2<f32>, size: vec2<f32>, radii: vec4<f32>, stroke: f32) -> f32 {
    let half = size * 0.5;
    let outer_d = rounded_box_sdf(local - half, half, radii);
    let aa = max(length(vec2<f32>(dpdx(outer_d), dpdy(outer_d))), 0.001);
    if stroke <= 0.0 {
        return 1.0 - smoothstep(-aa, aa, outer_d);
    }
    let half_stroke = stroke * 0.5;
    let inner_size = max(half - half_stroke, vec2(0.0));
    let inner_r = max(radii - half_stroke, vec4(0.0));
    let inner_d = rounded_box_sdf(local - half, inner_size, inner_r);
    let outer_a = 1.0 - smoothstep(half_stroke - aa * 0.5, half_stroke + aa * 0.5, outer_d);
    let inner_a = 1.0 - smoothstep(-aa * 0.5, aa * 0.5, inner_d);
    return outer_a * (1.0 - inner_a);
}

fn clip_alpha(world_pos: vec2<f32>, clip_rect: vec4<f32>, clip_radius: vec4<f32>) -> f32 {
    if clip_rect.z <= clip_rect.x || clip_rect.w <= clip_rect.y {
        return 1.0;
    }
    if world_pos.x < clip_rect.x || world_pos.y < clip_rect.y
        || world_pos.x > clip_rect.z || world_pos.y > clip_rect.w {
        return 0.0;
    }
    if all(clip_radius == vec4(0.0)) {
        return 1.0;
    }
    let clip_size = vec2(clip_rect.z - clip_rect.x, clip_rect.w - clip_rect.y);
    let half = clip_size * 0.5;
    let p = world_pos - vec2(clip_rect.x, clip_rect.y) - half;
    let clamped_radius = min(clip_radius, vec4(min(half.x, half.y)));
    let d = rounded_box_sdf(p, half, clamped_radius);
    let aa = max(length(vec2<f32>(dpdx(d), dpdy(d))), 0.001);
    return 1.0 - smoothstep(-aa, aa, d);
}

fn shadow_alpha(world_pos: vec2<f32>, center: vec2<f32>, size: vec2<f32>, radii: vec4<f32>, shadow_blur: f32) -> f32 {
    let half = size * 0.5;
    let p = world_pos - center;
    let blur = vec4(shadow_blur, shadow_blur, shadow_blur, shadow_blur);
    let sd = rounded_box_sdf(p, half, radii + blur);
    let aa = max(length(vec2<f32>(dpdx(sd), dpdy(sd))), 0.001);
    return 1.0 - smoothstep(-max(shadow_blur, aa), max(shadow_blur, aa), sd);
}

@fragment fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let clamped_radii = min(in.radii, vec4(min(in.size.x, in.size.y) * 0.5));
    let clip_a = clip_alpha(in.world_pos, in.clip_rect, in.clip_radius);

    if in.is_shadow > 0.5 {
        let s_a = shadow_alpha(in.world_pos, in.shadow_offset, in.size, clamped_radii, in.shadow_blur_radius);
        let alpha = s_a * clip_a * in.shadow_color.a;
        return vec4(in.shadow_color.rgb * alpha, alpha);
    }

    let elem_a = element_alpha(in.local, in.size, clamped_radii, in.stroke);
    let alpha = elem_a * clip_a;

    if in.shadow_color.a > 0.0 {
        let s_a = shadow_alpha(in.world_pos, in.shadow_offset, in.size, clamped_radii, in.shadow_blur_radius);
        let shadow_a = s_a * clip_a * in.shadow_color.a;
        let fill_rgb = in.color.rgb * alpha;
        let fill_a = in.color.a * alpha;
        return vec4(mix(in.shadow_color.rgb * shadow_a, fill_rgb, fill_a), shadow_a + fill_a * (1.0 - shadow_a));
    }

    return vec4(in.color.rgb * alpha, in.color.a * alpha);
}
"#;

const GRADIENT_SHADER: &str = r#"
struct VertexInput {
    @location(0) pos: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) radii: vec4<f32>,
    @location(3) local: vec2<f32>,
    @location(4) stroke: f32,
    @location(5) world_pos: vec2<f32>,
    @location(6) shadow_offset: vec2<f32>,
    @location(7) clip_rect: vec4<f32>,
    @location(8) clip_radius: vec4<f32>,
    @location(9) col0: vec4<f32>,
    @location(10) col1: vec4<f32>,
    @location(11) col2: vec4<f32>,
    @location(12) col3: vec4<f32>,
    @location(13) count_offsets: vec4<f32>,
    @location(14) dir_start: vec2<f32>,
    @location(15) dir_end: vec2<f32>,
}
struct VertexOutput {
    @builtin(position) pos: vec4<f32>,
    @location(0) size: vec2<f32>,
    @location(1) radii: vec4<f32>,
    @location(2) local: vec2<f32>,
    @location(3) stroke: f32,
    @location(4) world_pos: vec2<f32>,
    @location(5) clip_rect: vec4<f32>,
    @location(6) clip_radius: vec4<f32>,
    @location(7) col0: vec4<f32>,
    @location(8) col1: vec4<f32>,
    @location(9) col2: vec4<f32>,
    @location(10) col3: vec4<f32>,
    @location(11) count_offsets: vec4<f32>,
    @location(12) dir_start: vec2<f32>,
    @location(13) dir_end: vec2<f32>,
    @location(14) kind: f32,
}

@vertex fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.pos = vec4(in.pos, 0.0, 1.0);
    out.size = in.size;
    out.radii = in.radii;
    out.local = in.local;
    out.stroke = in.stroke;
    out.world_pos = in.world_pos;
    out.clip_rect = in.clip_rect;
    out.clip_radius = in.clip_radius;
    out.col0 = in.col0;
    out.col1 = in.col1;
    out.col2 = in.col2;
    out.col3 = in.col3;
    out.count_offsets = in.count_offsets;
    out.dir_start = in.dir_start;
    out.dir_end = in.dir_end;
    out.kind = in.shadow_offset.x;   // gradient kind baked into unused shadow_offset
    return out;
}

fn rounded_box_sdf(p: vec2<f32>, half_size: vec2<f32>, corners: vec4<f32>) -> f32 {
    let box_half = select(corners.xw, corners.yz, p.x > 0.0);
    let corner = select(box_half.x, box_half.y, p.y > 0.0);
    let q = abs(p) - half_size + corner;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2(0.0))) - corner;
}

fn element_alpha(local: vec2<f32>, size: vec2<f32>, radii: vec4<f32>, stroke: f32) -> f32 {
    let half = size * 0.5;
    let outer_d = rounded_box_sdf(local - half, half, radii);
    let aa = max(length(vec2<f32>(dpdx(outer_d), dpdy(outer_d))), 0.001);
    if stroke <= 0.0 {
        return 1.0 - smoothstep(-aa, aa, outer_d);
    }
    let half_stroke = stroke * 0.5;
    let inner_size = max(half - half_stroke, vec2(0.0));
    let inner_r = max(radii - half_stroke, vec4(0.0));
    let inner_d = rounded_box_sdf(local - half, inner_size, inner_r);
    let outer_a = 1.0 - smoothstep(half_stroke - aa * 0.5, half_stroke + aa * 0.5, outer_d);
    let inner_a = 1.0 - smoothstep(-aa * 0.5, aa * 0.5, inner_d);
    return outer_a * (1.0 - inner_a);
}

fn clip_alpha(world_pos: vec2<f32>, clip_rect: vec4<f32>, clip_radius: vec4<f32>) -> f32 {
    if clip_rect.z <= clip_rect.x || clip_rect.w <= clip_rect.y {
        return 1.0;
    }
    if world_pos.x < clip_rect.x || world_pos.y < clip_rect.y
        || world_pos.x > clip_rect.z || world_pos.y > clip_rect.w {
        return 0.0;
    }
    if all(clip_radius == vec4(0.0)) {
        return 1.0;
    }
    let clip_size = vec2(clip_rect.z - clip_rect.x, clip_rect.w - clip_rect.y);
    let half = clip_size * 0.5;
    let p = world_pos - vec2(clip_rect.x, clip_rect.y) - half;
    let clamped_radius = min(clip_radius, vec4(min(half.x, half.y)));
    let d = rounded_box_sdf(p, half, clamped_radius);
    let aa = max(length(vec2<f32>(dpdx(d), dpdy(d))), 0.001);
    return 1.0 - smoothstep(-aa, aa, d);
}

fn gradient_t(kind: f32, pos: vec2<f32>, start: vec2<f32>, end: vec2<f32>) -> f32 {
    if kind < 0.5 {
        // Linear: project onto start->end.
        let v = end - start;
        let len = length(v);
        if len < 0.001 { return 0.0; }
        return dot(v / len, pos - start) / len;
    } else if kind < 1.5 {
        // Radial: distance from center / radius (|end-start|).
        let radius = length(end - start);
        if radius < 0.001 { return 0.0; }
        return length(pos - start) / radius;
    } else {
        // Conic: angle around center, 0deg = direction of (end-start).
        let ref_ang = atan2(end.y - start.y, end.x - start.x);
        let p = pos - start;
        let two_pi = 6.2831853;
        var a = atan2(p.y, p.x) - ref_ang;
        a = a - two_pi * floor(a / two_pi);   // wrap to [0, 2pi)
        return a / two_pi;
    }
}

fn gradient_color(kind: f32, pos: vec2<f32>, start: vec2<f32>, end: vec2<f32>, c0: vec4<f32>, c1: vec4<f32>, c2: vec4<f32>, c3: vec4<f32>, count_offsets: vec4<f32>) -> vec4<f32> {
    let colors = array<vec4<f32>, 4>(c0, c1, c2, c3);
    let count = i32(count_offsets.x);
    let offsets = array<f32, 4>(0.0, count_offsets.y, count_offsets.z, count_offsets.w);
    let t = clamp(gradient_t(kind, pos, start, end), 0.0, 1.0);
    var c = colors[0];
    for (var i: i32 = 0; i < count - 1; i++) {
        if (t >= offsets[i] && t <= offsets[i + 1]) {
            let f = smoothstep(offsets[i], offsets[i + 1], t);
            c = mix(colors[i], colors[i + 1], f);
        }
    }
    if (t >= offsets[count - 1]) { c = colors[count - 1]; }
    return c;
}

@fragment fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let clamped_radii = min(in.radii, vec4(min(in.size.x, in.size.y) * 0.5));
    let grad = gradient_color(in.kind, in.world_pos, in.dir_start, in.dir_end, in.col0, in.col1, in.col2, in.col3, in.count_offsets);
    let elem_a = element_alpha(in.local, in.size, clamped_radii, in.stroke);
    let clip_a = clip_alpha(in.world_pos, in.clip_rect, in.clip_radius);
    let alpha = elem_a * clip_a;
    return vec4(grad.rgb * alpha, grad.a * alpha);
}
"#;

const TEXT_SHADER: &str = r#"
struct VertexInput {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) clip_rect: vec4<f32>,
    @location(3) clip_radius: vec4<f32>,
    @location(4) world_pos: vec2<f32>,
    @location(5) opacity: f32,
}
struct VertexOutput {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world_pos: vec2<f32>,
    @location(2) clip_rect: vec4<f32>,
    @location(3) clip_radius: vec4<f32>,
    @location(4) opacity: f32,
}

@vertex fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.pos = vec4(in.pos, 0.0, 1.0);
    out.uv = in.uv;
    out.world_pos = in.world_pos;
    out.clip_rect = in.clip_rect;
    out.clip_radius = in.clip_radius;
    out.opacity = in.opacity;
    return out;
}

fn rounded_box_sdf(p: vec2<f32>, half_size: vec2<f32>, corners: vec4<f32>) -> f32 {
    let box_half = select(corners.xw, corners.yz, p.x > 0.0);
    let corner = select(box_half.x, box_half.y, p.y > 0.0);
    let q = abs(p) - half_size + corner;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2(0.0))) - corner;
}

fn clip_alpha(world_pos: vec2<f32>, clip_rect: vec4<f32>, clip_radius: vec4<f32>) -> f32 {
    if clip_rect.z <= clip_rect.x || clip_rect.w <= clip_rect.y { return 1.0; }
    if world_pos.x < clip_rect.x || world_pos.y < clip_rect.y
        || world_pos.x > clip_rect.z || world_pos.y > clip_rect.w { return 0.0; }
    if all(clip_radius == vec4(0.0)) { return 1.0; }
    let clip_size = vec2(clip_rect.z - clip_rect.x, clip_rect.w - clip_rect.y);
    let half = clip_size * 0.5;
    let p = world_pos - vec2(clip_rect.x, clip_rect.y) - half;
    let clamped_radius = min(clip_radius, vec4(min(half.x, half.y)));
    let d = rounded_box_sdf(p, half, clamped_radius);
    let aa = max(length(vec2<f32>(dpdx(d), dpdy(d))), 0.001);
    return 1.0 - smoothstep(-aa, aa, d);
}

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

@fragment fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Negative LOD bias preserves crispness — prefers the next-higher
    // mip level rather than letting trilinear blend with an undersized one.
    let sample = textureSampleBias(tex, samp, in.uv, -0.5);
    let ca = clip_alpha(in.world_pos, in.clip_rect, in.clip_radius);
    return vec4(sample.rgb, sample.a * ca * in.opacity);
}
"#;

const BLIT_SHADER: &str = r#"
struct VertexInput { @location(0) pos: vec2<f32>, @location(1) uv: vec2<f32> }
struct VertexOutput { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> }

@vertex fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.pos = vec4(in.pos, 0.0, 1.0);
    out.uv = in.uv;
    return out;
}

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

@fragment fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, in.uv);
}
"#;

const PATH_SHADER: &str = r#"
struct VertexInput {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world_pos: vec2<f32>,
    @location(3) clip_rect: vec4<f32>,
    @location(4) clip_radius: vec4<f32>,
}
struct VertexOutput {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) world_pos: vec2<f32>,
    @location(2) clip_rect: vec4<f32>,
    @location(3) clip_radius: vec4<f32>,
}

@vertex fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.pos = vec4(in.pos, 0.0, 1.0);
    out.color = in.color;
    out.world_pos = in.world_pos;
    out.clip_rect = in.clip_rect;
    out.clip_radius = in.clip_radius;
    return out;
}

fn rounded_box_sdf(p: vec2<f32>, half: vec2<f32>, radii: vec4<f32>) -> f32 {
    let box_half = select(radii.xw, radii.yz, p.x > 0.0);
    let corner = select(box_half.x, box_half.y, p.y > 0.0);
    let q = abs(p) - half + corner;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2(0.0))) - corner;
}

fn clip_alpha(world_pos: vec2<f32>, clip_rect: vec4<f32>, clip_radius: vec4<f32>) -> f32 {
    if clip_rect.z <= clip_rect.x || clip_rect.w <= clip_rect.y {
        return 1.0;
    }
    if world_pos.x < clip_rect.x || world_pos.y < clip_rect.y
        || world_pos.x > clip_rect.z || world_pos.y > clip_rect.w {
        return 0.0;
    }
    if all(clip_radius == vec4(0.0)) {
        return 1.0;
    }
    let clip_size = vec2(clip_rect.z - clip_rect.x, clip_rect.w - clip_rect.y);
    let half = clip_size * 0.5;
    let p = world_pos - vec2(clip_rect.x, clip_rect.y) - half;
    let clamped_radius = min(clip_radius, vec4(min(half.x, half.y)));
    let d = rounded_box_sdf(p, half, clamped_radius);
    let aa = max(length(vec2<f32>(dpdx(d), dpdy(d))), 0.001);
    return 1.0 - smoothstep(-aa, aa, d);
}

@fragment fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let ca = clip_alpha(in.world_pos, in.clip_rect, in.clip_radius);
    return vec4(in.color.rgb, in.color.a * ca);
}
"#;

// ── Blend-mode effect shader ──
// Reuses the RectVertex layout. For effect rects the (otherwise unused) fields
// are repurposed: `shadow_offset` carries the snapshot UV (world_pos / screen,
// baked on the CPU so no uniform is needed) and `is_shadow` carries the blend
// mode (1=Multiply, 2=Screen, 3=Overlay). Samples the backdrop (dst) snapshot
// and applies a per-channel blend. Output is premultiplied; the rounded-rect SDF
// masks edges via mix(dst, blended, a) so there are no black corners.
const BLEND_SHADER: &str = r#"
struct VertexInput {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) size: vec2<f32>,
    @location(3) radii: vec4<f32>,
    @location(4) local: vec2<f32>,
    @location(5) stroke: f32,
    @location(6) world_pos: vec2<f32>,
    @location(7) uv: vec2<f32>,
    @location(8) clip_rect: vec4<f32>,
    @location(9) clip_radius: vec4<f32>,
    @location(10) shadow_color: vec4<f32>,
    @location(11) shadow_blur_radius: f32,
    @location(12) mode: f32,
}
struct VertexOutput {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) size: vec2<f32>,
    @location(2) radii: vec4<f32>,
    @location(3) local: vec2<f32>,
    @location(4) stroke: f32,
    @location(5) world_pos: vec2<f32>,
    @location(6) uv: vec2<f32>,
    @location(7) clip_rect: vec4<f32>,
    @location(8) clip_radius: vec4<f32>,
    @location(9) mode: f32,
}

@group(0) @binding(0) var snapshot_tex: texture_2d<f32>;
@group(0) @binding(1) var snapshot_samp: sampler;

@vertex fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.pos = vec4(in.pos, 0.0, 1.0);
    out.color = in.color;
    out.size = in.size;
    out.radii = in.radii;
    out.local = in.local;
    out.stroke = in.stroke;
    out.world_pos = in.world_pos;
    // Screen-space UV for sampling the backdrop snapshot, derived directly from
    // the NDC position (robust to scroll / logical-coordinate offsets). NDC x/y
    // in [-1,1] → UV in [0,1] with y flipped (texture origin is top-left).
    out.uv = vec2(in.pos.x * 0.5 + 0.5, 0.5 - in.pos.y * 0.5);
    out.clip_rect = in.clip_rect;
    out.clip_radius = in.clip_radius;
    out.mode = in.mode;
    return out;
}

fn rounded_box_sdf(p: vec2<f32>, half_size: vec2<f32>, corners: vec4<f32>) -> f32 {
    let box_half = select(corners.xw, corners.yz, p.x > 0.0);
    let corner = select(box_half.x, box_half.y, p.y > 0.0);
    let q = abs(p) - half_size + corner;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2(0.0))) - corner;
}

fn element_alpha(local: vec2<f32>, size: vec2<f32>, radii: vec4<f32>, stroke: f32) -> f32 {
    let half = size * 0.5;
    let outer_d = rounded_box_sdf(local - half, half, radii);
    let aa = max(length(vec2<f32>(dpdx(outer_d), dpdy(outer_d))), 0.001);
    if stroke <= 0.0 {
        return 1.0 - smoothstep(-aa, aa, outer_d);
    }
    let half_stroke = stroke * 0.5;
    let inner_size = max(half - half_stroke, vec2(0.0));
    let inner_r = max(radii - half_stroke, vec4(0.0));
    let inner_d = rounded_box_sdf(local - half, inner_size, inner_r);
    let outer_a = 1.0 - smoothstep(half_stroke - aa * 0.5, half_stroke + aa * 0.5, outer_d);
    let inner_a = 1.0 - smoothstep(-aa * 0.5, aa * 0.5, inner_d);
    return outer_a * (1.0 - inner_a);
}

fn clip_alpha(world_pos: vec2<f32>, clip_rect: vec4<f32>, clip_radius: vec4<f32>) -> f32 {
    if clip_rect.z <= clip_rect.x || clip_rect.w <= clip_rect.y {
        return 1.0;
    }
    if world_pos.x < clip_rect.x || world_pos.y < clip_rect.y
        || world_pos.x > clip_rect.z || world_pos.y > clip_rect.w {
        return 0.0;
    }
    if all(clip_radius == vec4(0.0)) {
        return 1.0;
    }
    let clip_size = vec2(clip_rect.z - clip_rect.x, clip_rect.w - clip_rect.y);
    let half = clip_size * 0.5;
    let p = world_pos - vec2(clip_rect.x, clip_rect.y) - half;
    let clamped_radius = min(clip_radius, vec4(min(half.x, half.y)));
    let d = rounded_box_sdf(p, half, clamped_radius);
    let aa = max(length(vec2<f32>(dpdx(d), dpdy(d))), 0.001);
    return 1.0 - smoothstep(-aa, aa, d);
}

fn blend_overlay(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    // Per-channel hard-light(dst, src): if b < 0.5 → 2*b*s else 1-2*(1-b)*(1-s)
    let lo = 2.0 * b * s;
    let hi = 1.0 - 2.0 * (1.0 - b) * (1.0 - s);
    return select(hi, lo, b < vec3(0.5));
}

@fragment fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let clamped_radii = min(in.radii, vec4(min(in.size.x, in.size.y) * 0.5));
    let elem_a = element_alpha(in.local, in.size, clamped_radii, in.stroke);
    let clip_a = clip_alpha(in.world_pos, in.clip_rect, in.clip_radius);
    let a = in.color.a * elem_a * clip_a;

    // Sample the backdrop (dst) at this fragment's screen position.
    let dst = textureSample(snapshot_tex, snapshot_samp, in.uv).rgb;
    let src = in.color.rgb;

    let mode = i32(in.mode + 0.5);
    var blended: vec3<f32>;
    if mode == 1 {
        blended = src * dst;                       // Multiply
    } else if mode == 2 {
        blended = src + dst - src * dst;           // Screen
    } else if mode == 3 {
        blended = blend_overlay(dst, src);         // Overlay
    } else {
        blended = src;
    }

    // Composite the blended result over dst by coverage `a`. Outside the rounded
    // rect (a=0) this returns dst unchanged → no black edges. Output premultiplied.
    let out_rgb = mix(dst, blended, a);
    return vec4(out_rgb * a, a);
}
"#;

pub fn create_blend_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    // Texture + sampler only (blend mode & UV are baked into vertex data).
    create_text_bind_group_layout(device)
}

pub fn create_blend_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    bind_group_layout: &wgpu::BindGroupLayout,
    sample_count: u32,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("blend shader"),
        source: wgpu::ShaderSource::Wgsl(BLEND_SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("blend layout"),
        bind_group_layouts: &[Some(bind_group_layout)],
        ..Default::default()
    });
    let vb_desc = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<RectVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 8,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 24,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 32,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 48,
                shader_location: 4,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 56,
                shader_location: 5,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 60,
                shader_location: 6,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 68,
                shader_location: 7,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 76,
                shader_location: 8,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 92,
                shader_location: 9,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 108,
                shader_location: 10,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 124,
                shader_location: 11,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 128,
                shader_location: 12,
            },
        ],
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("blend pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(vb_desc)],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}

pub fn create_blit_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    create_text_bind_group_layout(device)
}

// ── Backdrop blur infrastructure ──
// Separable Gaussian blur: two full-screen passes (horizontal then vertical)
// over a snapshot of the backdrop, then a composite pass masks the blurred
// result into the element's rounded rect (with optional tint).

const BLUR_SHADER: &str = r#"
struct VertexInput { @location(0) pos: vec2<f32>, @location(1) uv: vec2<f32> }
struct VertexOutput { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> }

@vertex fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.pos = vec4(in.pos, 0.0, 1.0);
    out.uv = in.uv;
    return out;
}

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> u: BlurUniforms;

struct BlurUniforms {
    direction: vec2<f32>,   // (1,0) horizontal or (0,1) vertical, in texels
    texel: vec2<f32>,       // 1/texture_size
    radius: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

@fragment fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let sigma = max(u.radius * 0.5, 0.5);
    let two_sigma2 = 2.0 * sigma * sigma;
    let half_w = i32(ceil(u.radius));
    let step = u.direction * u.texel;
    var acc = vec4(0.0);
    var wsum = 0.0;
    for (var i = -half_w; i <= half_w; i = i + 1) {
        let fi = f32(i);
        let w = exp(-(fi * fi) / two_sigma2);
        acc = acc + textureSample(tex, samp, in.uv + step * fi) * w;
        wsum = wsum + w;
    }
    return acc / wsum;
}
"#;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BlurUniforms {
    pub direction: [f32; 2],
    pub texel: [f32; 2],
    pub radius: f32,
    pub _pad: [f32; 3],
}

pub fn create_blur_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("blur bind group layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    })
}

pub fn create_blur_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("blur shader"),
        source: wgpu::ShaderSource::Wgsl(BLUR_SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("blur layout"),
        bind_group_layouts: &[Some(bind_group_layout)],
        ..Default::default()
    });
    let vb_desc = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<BlitVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 8,
                shader_location: 1,
            },
        ],
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("blur pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(vb_desc)],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}

// Composite: draw the element's rounded rect into the (MSAA) persistent target,
// sampling the blurred backdrop and applying an optional tint. Reuses RectVertex
// (blend_mode/is_shadow field carries nothing here; UV from NDC like blend).
const COMPOSITE_SHADER: &str = r#"
struct VertexInput {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,       // tint (premultiplied-ready rgba; a=tint strength)
    @location(2) size: vec2<f32>,
    @location(3) radii: vec4<f32>,
    @location(4) local: vec2<f32>,
    @location(5) stroke: f32,
    @location(6) world_pos: vec2<f32>,
    @location(7) uv_unused: vec2<f32>,
    @location(8) clip_rect: vec4<f32>,
    @location(9) clip_radius: vec4<f32>,
    @location(10) shadow_color: vec4<f32>,
    @location(11) shadow_blur_radius: f32,
    @location(12) mode: f32,
}
struct VertexOutput {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) size: vec2<f32>,
    @location(2) radii: vec4<f32>,
    @location(3) local: vec2<f32>,
    @location(4) stroke: f32,
    @location(5) world_pos: vec2<f32>,
    @location(6) uv: vec2<f32>,
    @location(7) clip_rect: vec4<f32>,
    @location(8) clip_radius: vec4<f32>,
}

@group(0) @binding(0) var blurred_tex: texture_2d<f32>;
@group(0) @binding(1) var blurred_samp: sampler;

@vertex fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.pos = vec4(in.pos, 0.0, 1.0);
    out.color = in.color;
    out.size = in.size;
    out.radii = in.radii;
    out.local = in.local;
    out.stroke = in.stroke;
    out.world_pos = in.world_pos;
    out.uv = vec2(in.pos.x * 0.5 + 0.5, 0.5 - in.pos.y * 0.5);
    out.clip_rect = in.clip_rect;
    out.clip_radius = in.clip_radius;
    return out;
}

fn rounded_box_sdf(p: vec2<f32>, half_size: vec2<f32>, corners: vec4<f32>) -> f32 {
    let box_half = select(corners.xw, corners.yz, p.x > 0.0);
    let corner = select(box_half.x, box_half.y, p.y > 0.0);
    let q = abs(p) - half_size + corner;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2(0.0))) - corner;
}

fn element_alpha(local: vec2<f32>, size: vec2<f32>, radii: vec4<f32>) -> f32 {
    let half = size * 0.5;
    let d = rounded_box_sdf(local - half, half, radii);
    let aa = max(length(vec2<f32>(dpdx(d), dpdy(d))), 0.001);
    return 1.0 - smoothstep(-aa, aa, d);
}

fn clip_alpha(world_pos: vec2<f32>, clip_rect: vec4<f32>, clip_radius: vec4<f32>) -> f32 {
    if clip_rect.z <= clip_rect.x || clip_rect.w <= clip_rect.y { return 1.0; }
    if world_pos.x < clip_rect.x || world_pos.y < clip_rect.y
        || world_pos.x > clip_rect.z || world_pos.y > clip_rect.w { return 0.0; }
    if all(clip_radius == vec4(0.0)) { return 1.0; }
    let clip_size = vec2(clip_rect.z - clip_rect.x, clip_rect.w - clip_rect.y);
    let half = clip_size * 0.5;
    let p = world_pos - vec2(clip_rect.x, clip_rect.y) - half;
    let clamped_radius = min(clip_radius, vec4(min(half.x, half.y)));
    let d = rounded_box_sdf(p, half, clamped_radius);
    let aa = max(length(vec2<f32>(dpdx(d), dpdy(d))), 0.001);
    return 1.0 - smoothstep(-aa, aa, d);
}

@fragment fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let clamped_radii = min(in.radii, vec4(min(in.size.x, in.size.y) * 0.5));
    let elem_a = element_alpha(in.local, in.size, clamped_radii);
    let clip_a = clip_alpha(in.world_pos, in.clip_rect, in.clip_radius);
    let a = elem_a * clip_a;
    var rgb = textureSample(blurred_tex, blurred_samp, in.uv).rgb;
    // Optional tint: color.a is tint strength, color.rgb the tint colour.
    rgb = mix(rgb, in.color.rgb, in.color.a);
    return vec4(rgb * a, a);
}
"#;

pub fn create_composite_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    bind_group_layout: &wgpu::BindGroupLayout,
    sample_count: u32,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("composite shader"),
        source: wgpu::ShaderSource::Wgsl(COMPOSITE_SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("composite layout"),
        bind_group_layouts: &[Some(bind_group_layout)],
        ..Default::default()
    });
    let vb_desc = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<RectVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 8,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 24,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 32,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 48,
                shader_location: 4,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 56,
                shader_location: 5,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 60,
                shader_location: 6,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 68,
                shader_location: 7,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 76,
                shader_location: 8,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 92,
                shader_location: 9,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 108,
                shader_location: 10,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 124,
                shader_location: 11,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 128,
                shader_location: 12,
            },
        ],
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("composite pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(vb_desc)],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}

pub fn create_blit_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    bind_group_layout: &wgpu::BindGroupLayout,
    _sample_count: u32,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("blit shader"),
        source: wgpu::ShaderSource::Wgsl(BLIT_SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("blit layout"),
        bind_group_layouts: &[Some(bind_group_layout)],
        ..Default::default()
    });
    let vb_desc = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<BlitVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 8,
                shader_location: 1,
            },
        ],
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("blit pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(vb_desc)],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}

pub fn create_path_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    sample_count: u32,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("path shader"),
        source: wgpu::ShaderSource::Wgsl(PATH_SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("path layout"),
        bind_group_layouts: &[],
        ..Default::default()
    });
    let vb_desc = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<PathVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 8,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 24,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 32,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 48,
                shader_location: 4,
            },
        ],
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("path pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(vb_desc)],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}
