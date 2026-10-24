use anyhow::{Context, Result, ensure};
use image::RgbaImage;
use rusty_spine::BlendMode;
use std::collections::HashMap;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub dark: [f32; 4],
    pub pma: f32,
}
pub struct Draw<'a> {
    pub key: usize,
    pub image: &'a RgbaImage,
    pub vertices: Vec<Vertex>,
    pub blend: BlendMode,
}
struct Target {
    size: (u32, u32),
    texture: wgpu::Texture,
    msaa: wgpu::Texture,
    readback: wgpu::Buffer,
    pitch: u32,
}
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    pipelines: Vec<wgpu::RenderPipeline>,
    textures: HashMap<usize, wgpu::BindGroup>,
    target: Option<Target>,
}
impl Renderer {
    pub fn new() -> Result<Self> {
        pollster::block_on(async {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends: wgpu::Backends::DX12,
                ..wgpu::InstanceDescriptor::new_without_display_handle()
            });
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    force_fallback_adapter: false,
                    compatible_surface: None,
                    ..Default::default()
                })
                .await
                .context("没有可用的动画 GPU")?;
            let info = adapter.get_info();
            ensure!(
                info.device_type != wgpu::DeviceType::Cpu,
                "动画需要硬件 GPU"
            );
            crate::diagnostics::record(format!(
                "animation GPU: {} {:?} {:?}",
                info.name, info.backend, info.device_type
            ));
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("Spine GPU"),
                    ..Default::default()
                })
                .await?;
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Spine textures"),
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
            });
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Spine shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("spine.wgsl").into()),
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
            let attrs = wgpu::vertex_attr_array![0=>Float32x2,1=>Float32x2,2=>Float32x4,3=>Float32x4,4=>Float32];
            let alpha = wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            };
            let mut pipelines = Vec::new();
            for (src, dst) in [
                (wgpu::BlendFactor::One, wgpu::BlendFactor::OneMinusSrcAlpha),
                (wgpu::BlendFactor::One, wgpu::BlendFactor::One),
                (wgpu::BlendFactor::Dst, wgpu::BlendFactor::OneMinusSrcAlpha),
                (wgpu::BlendFactor::One, wgpu::BlendFactor::OneMinusSrc),
            ] {
                pipelines.push(
                    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: Some("Spine triangles"),
                        layout: Some(&pipeline_layout),
                        vertex: wgpu::VertexState {
                            module: &shader,
                            entry_point: Some("vs"),
                            compilation_options: Default::default(),
                            buffers: &[Some(wgpu::VertexBufferLayout {
                                array_stride: std::mem::size_of::<Vertex>() as u64,
                                step_mode: wgpu::VertexStepMode::Vertex,
                                attributes: &attrs,
                            })],
                        },
                        primitive: wgpu::PrimitiveState {
                            cull_mode: None,
                            ..Default::default()
                        },
                        depth_stencil: None,
                        multisample: wgpu::MultisampleState {
                            count: 4,
                            ..Default::default()
                        },
                        fragment: Some(wgpu::FragmentState {
                            module: &shader,
                            entry_point: Some("fs"),
                            compilation_options: Default::default(),
                            targets: &[Some(wgpu::ColorTargetState {
                                format: wgpu::TextureFormat::Bgra8Unorm,
                                blend: Some(wgpu::BlendState {
                                    color: wgpu::BlendComponent {
                                        src_factor: src,
                                        dst_factor: dst,
                                        operation: wgpu::BlendOperation::Add,
                                    },
                                    alpha,
                                }),
                                write_mask: wgpu::ColorWrites::ALL,
                            })],
                        }),
                        multiview_mask: None,
                        cache: None,
                    }),
                );
            }
            Ok(Self {
                device,
                queue,
                layout,
                sampler,
                pipelines,
                textures: HashMap::new(),
                target: None,
            })
        })
    }
    pub fn render(&mut self, draws: &[Draw<'_>], w: u32, h: u32) -> Result<RgbaImage> {
        for draw in draws {
            if !self.textures.contains_key(&draw.key) {
                let image = draw.image;
                let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Spine atlas"),
                    size: wgpu::Extent3d {
                        width: image.width(),
                        height: image.height(),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                self.queue.write_texture(
                    texture.as_image_copy(),
                    image.as_raw(),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(image.width() * 4),
                        rows_per_image: Some(image.height()),
                    },
                    texture.size(),
                );
                let view = texture.create_view(&Default::default());
                let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &self.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                });
                self.textures.insert(draw.key, bind);
            }
        }
        if self.target.as_ref().is_none_or(|t| t.size != (w, h)) {
            let make = |samples, usage| {
                self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Spine frame"),
                    size: wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Bgra8Unorm,
                    usage,
                    view_formats: &[],
                })
            };
            let texture = make(
                1,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            );
            let msaa = make(4, wgpu::TextureUsages::RENDER_ATTACHMENT);
            let pitch = (w * 4).div_ceil(256) * 256;
            let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Spine frame readback"),
                size: pitch as u64 * h as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            self.target = Some(Target {
                size: (w, h),
                texture,
                msaa,
                readback,
                pitch,
            });
        }
        let target = self.target.as_ref().unwrap();
        let mut vertices = Vec::new();
        let mut ranges = Vec::new();
        for draw in draws {
            let start = vertices.len() as u32;
            vertices.extend_from_slice(&draw.vertices);
            ranges.push(start..vertices.len() as u32);
        }
        let buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Spine posed vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let view = target.texture.create_view(&Default::default());
        let msaa = target.msaa.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let attachments = [Some(wgpu::RenderPassColorAttachment {
                view: &msaa,
                depth_slice: None,
                resolve_target: Some(&view),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Discard,
                },
            })];
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Spine render"),
                color_attachments: &attachments,
                ..Default::default()
            });
            pass.set_vertex_buffer(0, buffer.slice(..));
            for (draw, range) in draws.iter().zip(ranges) {
                let pipeline = match draw.blend {
                    BlendMode::Additive => 1,
                    BlendMode::Multiply => 2,
                    BlendMode::Screen => 3,
                    _ => 0,
                };
                pass.set_pipeline(&self.pipelines[pipeline]);
                pass.set_bind_group(0, &self.textures[&draw.key], &[]);
                pass.draw(range, 0..1);
            }
        }
        encoder.copy_texture_to_buffer(
            target.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &target.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(target.pitch),
                    rows_per_image: Some(h),
                },
            },
            target.texture.size(),
        );
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        target
            .readback
            .map_async(wgpu::MapMode::Read, .., move |r| {
                let _ = tx.send(r);
            });
        self.device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()??;
        let mapped = target.readback.get_mapped_range(..)?;
        let mut bytes = Vec::with_capacity((w * h * 4) as usize);
        for row in mapped.chunks_exact(target.pitch as usize) {
            bytes.extend_from_slice(&row[..w as usize * 4]);
        }
        drop(mapped);
        target.readback.unmap();
        for pixel in bytes.as_chunks_mut::<4>().0 {
            let a = pixel[3] as u32;
            if a > 0 && a < 255 {
                for c in &mut pixel[..3] {
                    *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
        RgbaImage::from_raw(w, h, bytes).context("GPU 动画帧无效")
    }
}
