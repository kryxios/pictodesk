//! Window runner: GPU surface, the event loop, and window creation.
use super::*;
use crate::tools::{DragMode, Tool};

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum SpecialKey {
    Escape,
    Enter,
    Backspace,
}

pub(crate) struct Gpu {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) layout: wgpu::BindGroupLayout,
    pub(crate) sampler: wgpu::Sampler,
    pub(crate) texture: Option<(wgpu::Texture, wgpu::BindGroup, (u32, u32))>,
    pub(crate) format: wgpu::TextureFormat,
}

impl Gpu {
    pub(crate) async fn new_for_surface(
        instance: &wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
    ) -> Option<Self> {
        // Prefer a real GPU adapter. Forcing the software fallback
        // adapter leaves some machines (notably macOS) with a surface
        // that can never present — the window opens white and stays
        // white. Only fall back to software if no GPU adapter exists.
        let adapter = match instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: surface,
                force_fallback_adapter: false,
            })
            .await
        {
            Some(a) => Some(a),
            None => {
                instance
                    .request_adapter(&wgpu::RequestAdapterOptions {
                        power_preference: wgpu::PowerPreference::default(),
                        compatible_surface: surface,
                        force_fallback_adapter: true,
                    })
                    .await
            }
        }?;
        let info = adapter.get_info();
        eprintln!(
            "PictoDesk graphics: {} ({:?}, {:?})",
            info.name, info.backend, info.device_type
        );
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("pictodesk-device"),
                    required_features: wgpu::Features::empty(),
                    // The adapter's own limits, NOT downlevel
                    // defaults: downlevel caps textures at 2048px,
                    // which a Retina window (2560x1600 physical)
                    // exceeds — configuring the surface then panics.
                    required_limits: adapter.limits(),
                },
                None,
            )
            .await
            .ok()?;
        let format = surface
            .and_then(|s| s.get_capabilities(&adapter).formats.first().copied())
            .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui-shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui-bgl"),
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui-pl"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs",
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs",
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ui-sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Some(Gpu {
            device,
            queue,
            pipeline,
            layout,
            sampler,
            texture: None,
            format,
        })
    }

    pub(crate) fn upload(&mut self, frame: &Frame) {
        let size = (frame.w, frame.h);
        if self.texture.as_ref().map(|t| t.2) != Some(size) {
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("ui-frame"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = tex.create_view(&Default::default());
            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ui-bg"),
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
            self.texture = Some((tex, bg, size));
        }
        let (tex, _, _) = self.texture.as_ref().unwrap();
        let pitch = size.0 * 4;
        let padded_pitch = pitch.div_ceil(256) * 256;
        if padded_pitch == pitch {
            self.queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &frame.px,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(pitch),
                    rows_per_image: Some(size.1),
                },
                wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
            );
        } else {
            let mut staged = vec![0u8; (padded_pitch * size.1) as usize];
            for y in 0..size.1 as usize {
                staged[y * padded_pitch as usize..y * padded_pitch as usize + pitch as usize]
                    .copy_from_slice(&frame.px[y * pitch as usize..(y + 1) * pitch as usize]);
            }
            self.queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &staged,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_pitch),
                    rows_per_image: Some(size.1),
                },
                wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    pub(crate) fn draw_to(&self, target: &wgpu::TextureView) {
        let (_, bg, _) = self.texture.as_ref().expect("upload before draw");
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, bg, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([enc.finish()]);
    }
}

pub(crate) struct Runner {
    pub(crate) app: App,
    pub(crate) window: Option<Arc<Window>>,
    pub(crate) gpu: Option<Gpu>,
    pub(crate) surface: Option<wgpu::Surface<'static>>,
    pub(crate) frame: Frame,
    pub(crate) last_ants: std::time::Instant,
    pub(crate) last_air: std::time::Instant,
    /// Display scale factor (2.0 on Retina). The UI is drawn on the
    /// CPU at LOGICAL size and the GPU stretches it over the physical
    /// surface — drawing at physical size made every control half
    /// size on high-DPI displays and quadrupled the per-frame CPU
    /// and upload cost.
    pub(crate) scale: f64,
}

impl Runner {
    pub(crate) fn logical_size(&self, phys_w: u32, phys_h: u32) -> (u32, u32) {
        let s = if self.scale > 0.0 { self.scale } else { 1.0 };
        (
            ((phys_w as f64 / s).round() as u32).max(1),
            ((phys_h as f64 / s).round() as u32).max(1),
        )
    }

    pub(crate) fn redraw(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        let logical = self.logical_size(size.width, size.height);
        self.app.win_size = logical;
        if self.frame.w != logical.0 || self.frame.h != logical.1 {
            self.frame.resize(logical.0, logical.1);
        }
        self.app.draw(&mut self.frame);
        if let (Some(gpu), Some(surface)) = (&mut self.gpu, &self.surface) {
            gpu.upload(&self.frame);
            match surface.get_current_texture() {
                Ok(frame) => {
                    let view = frame
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());
                    gpu.draw_to(&view);
                    frame.present();
                }
                Err(e) => {
                    // Any surface error (Lost, Outdated, Timeout,
                    // OutOfMemory): reconfigure and let the next event
                    // present a fresh frame, instead of swallowing the
                    // error and leaving the window blank forever.
                    eprintln!("PictoDesk surface: {e:?} — reconfiguring");
                    let sz = window.inner_size();
                    if sz.width > 0 && sz.height > 0 {
                        surface.configure(
                            &gpu.device,
                            &wgpu::SurfaceConfiguration {
                                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                                format: gpu.format,
                                width: sz.width.min(gpu.device.limits().max_texture_dimension_2d),
                                height: sz.height.min(gpu.device.limits().max_texture_dimension_2d),
                                present_mode: wgpu::PresentMode::Fifo,
                                desired_maximum_frame_latency: 2,
                                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                                view_formats: vec![],
                            },
                        );
                    }
                }
            }
        }
        if self.app.quit {
            std::process::exit(0);
        }
    }
}

impl ApplicationHandler for Runner {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = Arc::new(
            el.create_window(
                Window::default_attributes()
                    .with_title("PictoDesk")
                    .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0)),
            )
            .expect("create window"),
        );
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).ok();
        let gpu = pollster::block_on(Gpu::new_for_surface(&instance, surface.as_ref()));
        if gpu.is_none() {
            // Never sit on a dead white window: say what happened in
            // the terminal and quit.
            eprintln!(
                "PictoDesk could not initialize graphics on this machine \
                 (no usable GPU adapter or device). Nothing was drawn. \
                 Please report this line along with your OS and GPU."
            );
            el.exit();
            return;
        }
        if let (Some(gpu), Some(surface)) = (&gpu, &surface) {
            let sz = window.inner_size();
            if sz.width > 0 && sz.height > 0 {
                surface.configure(
                    &gpu.device,
                    &wgpu::SurfaceConfiguration {
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        format: gpu.format,
                        width: sz.width.min(gpu.device.limits().max_texture_dimension_2d),
                        height: sz.height.min(gpu.device.limits().max_texture_dimension_2d),
                        present_mode: wgpu::PresentMode::Fifo,
                        desired_maximum_frame_latency: 2,
                        alpha_mode: wgpu::CompositeAlphaMode::Auto,
                        view_formats: vec![],
                    },
                );
            }
        }
        self.scale = window.scale_factor();
        self.app.win_size = {
            let s = window.inner_size();
            self.logical_size(s.width, s.height)
        };
        self.app.fit_view(self.app.win_size.0, self.app.win_size.1);
        self.window = Some(window);
        self.gpu = gpu;
        self.surface = surface;
        self.redraw();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.clone() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(sz) => {
                if let (Some(gpu), Some(surface)) = (&self.gpu, &self.surface) {
                    if sz.width > 0 && sz.height > 0 {
                        surface.configure(
                            &gpu.device,
                            &wgpu::SurfaceConfiguration {
                                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                                format: gpu.format,
                                width: sz.width.min(gpu.device.limits().max_texture_dimension_2d),
                                height: sz.height.min(gpu.device.limits().max_texture_dimension_2d),
                                present_mode: wgpu::PresentMode::Fifo,
                                desired_maximum_frame_latency: 2,
                                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                                view_formats: vec![],
                            },
                        );
                    }
                }
                self.app.win_size = self.logical_size(sz.width, sz.height);
                window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = scale_factor;
                window.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let s = if self.scale > 0.0 { self.scale } else { 1.0 };
                self.app
                    .mouse_move((position.x / s) as f32, (position.y / s) as f32);
                window.request_redraw();
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let (x, y) = self.app.cursor;
                match state {
                    ElementState::Pressed => self.app.mouse_down(x, y),
                    ElementState::Released => self.app.mouse_up(x, y),
                }
                window.request_redraw();
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } => {
                let (x, y) = self.app.cursor;
                self.app.right_click(x, y);
                window.request_redraw();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x, y),
                    MouseScrollDelta::PixelDelta(p) => (p.x as f32 / 50.0, p.y as f32 / 50.0),
                };
                self.app.scroll(dx, dy);
                window.request_redraw();
            }
            WindowEvent::ModifiersChanged(mods) => {
                let s: ModifiersState = mods.state();
                self.app.ctrl_down = s.control_key();
                self.app.super_down = s.super_key();
                self.app.alt_down = s.alt_key();
                self.app.shift_down = s.shift_key();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed || event.state == ElementState::Released {
                    let pressed = event.state == ElementState::Pressed;
                    match &event.logical_key {
                        Key::Named(NamedKey::Space) => {
                            self.app.space_down = pressed;
                        }
                        Key::Named(NamedKey::Escape) if pressed => {
                            self.app.key_special(SpecialKey::Escape)
                        }
                        Key::Named(NamedKey::Enter) if pressed => {
                            self.app.key_special(SpecialKey::Enter)
                        }
                        Key::Named(NamedKey::Backspace) if pressed => {
                            self.app.key_special(SpecialKey::Backspace)
                        }
                        Key::Named(NamedKey::Tab) if pressed => {
                            let mut parts: Vec<&str> = Vec::new();
                            if self.app.primary_down() {
                                parts.push("primary");
                            }
                            if self.app.shift_down {
                                parts.push("shift");
                            }
                            parts.push("tab");
                            let combo = parts.join("+");
                            if self.app.prefs_capture.is_some() || self.app.primary_down() {
                                self.app.dispatch_combo(&combo);
                            }
                        }
                        Key::Character(s) if pressed => {
                            for c in s.chars() {
                                self.app.key_char(c);
                            }
                        }
                        _ => {}
                    }
                    window.request_redraw();
                }
            }
            WindowEvent::DroppedFile(path) => {
                self.app.open_path(&path);
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                self.redraw();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        // Hover tooltip wake-up: if a hover is waiting out its
        // delay, wake the loop when the delay elapses so the
        // bubble appears without waiting for the next event.
        if self.app.tooltip.is_none() {
            if let Some(since) = self.app.hover_since {
                if since.elapsed().as_millis() >= 600 {
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                } else {
                    el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
                        since + std::time::Duration::from_millis(600),
                    ));
                }
            }
        }
        self.app.autosave_tick();
        // Airbrush build-up tick (~50 ms).
        if self.last_air.elapsed().as_millis() > 50 {
            self.last_air = std::time::Instant::now();
            if self.app.tool == Tool::Airbrush && self.app.drag == DragMode::Paint {
                self.app.airbrush_tick();
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
        }
        // Marching ants animation tick.
        if self.last_ants.elapsed().as_millis() > 160 {
            self.last_ants = std::time::Instant::now();
            if self.app.sel.is_some() || self.app.drag == DragMode::SelectRect {
                self.app.ants_phase = self.app.ants_phase.wrapping_add(1);
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
        }
    }
}

pub(crate) fn run_window(path: Option<PathBuf>) {
    let mut app = App::new(demo_document(), None);
    let mtime = |p: &std::path::Path| {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    };
    if let Some(p) = path {
        let rec = format::recovery_path_for(&p);
        if rec.exists() && (!p.exists() || mtime(&rec) > mtime(&p)) {
            match format::load(&rec) {
                Ok(doc) => {
                    app.doc = doc;
                    app.file_path = Some(p);
                    app.dirty = true;
                    app.doc_version += 1;
                    app.recovery_file = Some(rec);
                    app.status = "Recovered autosaved work — Save to keep it".into();
                }
                Err(_) => app.replace_doc_from_path(&p),
            }
        } else {
            app.replace_doc_from_path(&p);
        }
    } else {
        let rec = std::env::temp_dir().join("pictodesk-untitled.pdk.recovery");
        if rec.exists() {
            if let Ok(doc) = format::load(&rec) {
                app.doc = doc;
                app.dirty = true;
                app.doc_version += 1;
                app.recovery_file = Some(rec);
                app.status = "Recovered unsaved work — Save to keep it".into();
            }
        }
    }
    let event_loop = EventLoop::new().expect("event loop");
    let mut runner = Runner {
        app,
        window: None,
        gpu: None,
        surface: None,
        frame: Frame::new(1280, 800),
        last_ants: std::time::Instant::now(),
        last_air: std::time::Instant::now(),
        scale: 1.0,
    };
    let _ = event_loop.run_app(&mut runner);
}
