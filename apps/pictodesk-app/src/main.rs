//! PictoDesk desktop shell (Phase 0 window): a wgpu window displaying
//! the core engine's flattened render on a GPU texture.
//!
//! Controls: mouse wheel = zoom at cursor, left-drag = pan, R = fit
//! to window, Esc = quit.
//!
//! Verification mode: `pictodesk-app --shot out.ppm` renders one frame
//! offscreen through the same pipeline and writes it as a PPM, no
//! window needed — that's how the render path is tested headlessly.

mod viewport;

use pictodesk_core::blend::BlendMode;
use pictodesk_core::document::Document;
use pictodesk_core::pixel::Pixel16;
use viewport::Viewport;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    offset: [f32; 2],
    scale: f32,
    _pad: f32,
    doc_size: [f32; 2],
    win_size: [f32; 2],
}

/// The demo document: the same scene the CLI harness builds — gradient
/// background + a Screen-blended accent block, two layers, via the
/// core engine only.
fn demo_rgba() -> (Vec<u8>, u32, u32) {
    let mut doc = Document::new(640, 400);
    for y in 0..400i32 {
        let t = y as f32 / 399.0;
        let p = Pixel16::from_f32s(0.10 + 0.20 * t, 0.25, 0.55 - 0.30 * t, 1.0);
        doc.layers[0].fill_rect(0, y, 640, 1, p);
    }
    let top = doc.add_layer("Accent");
    doc.layers[top].blend_mode = BlendMode::Screen;
    doc.layers[top].fill_rect(220, 120, 200, 160, Pixel16::from_rgba8(180, 60, 30, 255));
    (doc.render_full_rgba8(), doc.width, doc.height)
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buf: wgpu::Buffer,
    doc_size: (u32, u32),
}

fn request_adapter(instance: &wgpu::Instance, surface: Option<&wgpu::Surface>) -> wgpu::Adapter {
    pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: surface,
        force_fallback_adapter: true,
    }))
    .expect("a GPU adapter (software rendering is fine)")
}

fn build_gpu(
    adapter: &wgpu::Adapter,
    format: wgpu::TextureFormat,
    rgba: &[u8],
    doc_w: u32,
    doc_h: u32,
) -> Gpu {
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("pictodesk device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
        },
        None,
    ))
    .expect("device");

    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("document texture"),
        size: wgpu::Extent3d {
            width: doc_w,
            height: doc_h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(4 * doc_w),
            rows_per_image: Some(doc_h),
        },
        wgpu::Extent3d {
            width: doc_w,
            height: doc_h,
            depth_or_array_layers: 1,
        },
    );
    let tex_view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("canvas sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("canvas uniforms"),
        size: std::mem::size_of::<Uniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("canvas layout"),
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
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("canvas bind group"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&tex_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform_buf,
                    offset: 0,
                    size: None,
                }),
            },
        ],
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("canvas shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("canvas pipeline layout"),
        bind_group_layouts: &[&layout],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("canvas pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });
    Gpu {
        device,
        queue,
        pipeline,
        bind_group,
        uniform_buf,
        doc_size: (doc_w, doc_h),
    }
}

impl Gpu {
    fn write_uniforms(&self, vp: Viewport, win: (u32, u32)) {
        let u = Uniforms {
            offset: [vp.offset.0, vp.offset.1],
            scale: vp.scale,
            _pad: 0.0,
            doc_size: [self.doc_size.0 as f32, self.doc_size.1 as f32],
            win_size: [win.0 as f32, win.1 as f32],
        };
        self.queue
            .write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&u));
    }

    fn draw(&self, view: &wgpu::TextureView) {
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("canvas pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.16,
                            g: 0.16,
                            b: 0.17,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([enc.finish()]);
    }
}

/// Offscreen verification: render the demo scene at 960x720 through the
/// real pipeline, read the pixels back, write a PPM.
fn headless_shot(path: &str) {
    let (rgba, dw, dh) = demo_rgba();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    let adapter = request_adapter(&instance, None);
    let gpu = build_gpu(&adapter, wgpu::TextureFormat::Rgba8UnormSrgb, &rgba, dw, dh);
    let (w, h) = (960u32, 720u32);
    let vp = Viewport::fit(dw as f32, dh as f32, w as f32, h as f32);
    gpu.write_uniforms(vp, (w, h));

    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shot target"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    gpu.draw(&view);

    let padded_row = (4 * w).div_ceil(256) * 256;
    let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("shot readback"),
        size: (padded_row * h) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut enc = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("copy"),
        });
    enc.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: &buf,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(padded_row),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit([enc.finish()]);
    let slice = buf.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    gpu.device.poll(wgpu::Maintain::Wait);
    rx.recv().unwrap().expect("map readback");
    let data = slice.get_mapped_range();
    let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
    for y in 0..h {
        let row = (y * padded_row) as usize;
        for x in 0..w {
            let i = row + (x * 4) as usize;
            ppm.extend_from_slice(&data[i..i + 3]);
        }
    }
    // Sanity: center of the window must be the document, corner the surround.
    let center = ((h / 2 * padded_row) + (w / 2) * 4) as usize;
    let corner = 0usize;
    println!(
        "shot center rgb: {} {} {}",
        data[center],
        data[center + 1],
        data[center + 2]
    );
    println!(
        "shot corner rgb: {} {} {}",
        data[corner],
        data[corner + 1],
        data[corner + 2]
    );
    assert!(
        data[center] != data[corner],
        "center and corner identical — nothing rendered"
    );
    std::fs::write(path, ppm).expect("write shot ppm");
    println!("SHOT PASS: wrote {path}");
}

// ---------------- windowed app ----------------

struct State {
    window: std::sync::Arc<Window>,
    surface: wgpu::Surface<'static>,
    gpu: Gpu,
    config: wgpu::SurfaceConfiguration,
    vp: Viewport,
}

struct App {
    state: Option<State>,
    cursor: (f32, f32),
    dragging: bool,
    last: (f32, f32),
}

impl App {
    fn new() -> Self {
        App {
            state: None,
            cursor: (0.0, 0.0),
            dragging: false,
            last: (0.0, 0.0),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let window = std::sync::Arc::new(
            event_loop
                .create_window(Window::default_attributes().with_title("PictoDesk — Phase 0"))
                .expect("create window"),
        );
        let size = window.inner_size();
        let (rgba, dw, dh) = demo_rgba();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let surface = instance.create_surface(window.clone()).expect("surface");
        let adapter = request_adapter(&instance, Some(&surface));
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let gpu = build_gpu(&adapter, format, &rgba, dw, dh);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&gpu.device, &config);
        let vp = Viewport::fit(dw as f32, dh as f32, size.width as f32, size.height as f32);
        self.state = Some(State {
            window,
            surface,
            gpu,
            config,
            vp,
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(st) = &mut self.state else { return };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                st.config.width = size.width.max(1);
                st.config.height = size.height.max(1);
                st.surface.configure(&st.gpu.device, &st.config);
                st.window.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let pos = (position.x as f32, position.y as f32);
                if self.dragging {
                    st.vp.pan_by((pos.0 - self.last.0, pos.1 - self.last.1));
                    st.window.request_redraw();
                }
                self.cursor = pos;
                self.last = pos;
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                self.dragging = state == ElementState::Pressed;
                self.last = self.cursor;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let y = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 50.0,
                };
                st.vp.zoom_at(self.cursor, 1.1f32.powf(y));
                st.window.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::Escape) => event_loop.exit(),
                        PhysicalKey::Code(KeyCode::KeyR) => {
                            let size = st.window.inner_size();
                            st.vp = Viewport::fit(
                                st.gpu.doc_size.0 as f32,
                                st.gpu.doc_size.1 as f32,
                                size.width as f32,
                                size.height as f32,
                            );
                            st.window.request_redraw();
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let size = (st.config.width, st.config.height);
                st.gpu.write_uniforms(st.vp, size);
                match st.surface.get_current_texture() {
                    Ok(frame) => {
                        let view = frame
                            .texture
                            .create_view(&wgpu::TextureViewDescriptor::default());
                        st.gpu.draw(&view);
                        frame.present();
                    }
                    Err(wgpu::SurfaceError::Lost) => {
                        st.surface.configure(&st.gpu.device, &st.config);
                    }
                    Err(_) => {}
                }
            }
            _ => {}
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--shot") {
        let path = args.get(i + 1).map(|s| s.as_str()).unwrap_or("shot.ppm");
        headless_shot(path);
        return;
    }
    let event_loop = EventLoop::new().expect("event loop");
    let mut app = App::new();
    event_loop.run_app(&mut app).expect("run app");
}
