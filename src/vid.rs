use crate::cpu::Cpu;
use crate::interrupt::{InterruptTrigger, SimpleInterruptProvider};
use crate::memory::{Contiguous, Memory, MemoryDevice, OverlayMemory};
use glam::{Mat4, Quat, Vec2, Vec3};
use std::fs::File;
use std::io::Read;
use std::mem::offset_of;
use std::sync::{Arc, Mutex};
use std::thread;
use wgpu;
use wgpu::util::DeviceExt;
use winit::event::ElementState;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

const CONTENT_WIDTH: u32 = 160;
const CONTENT_HEIGHT: u32 = 200;
const BORDER_X: u32 = 4;
const BORDER_Y: u32 = 8;
const WIDTH: u32 = CONTENT_WIDTH + 2 * BORDER_X;
const HEIGHT: u32 = CONTENT_HEIGHT + 2 * BORDER_Y;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Color {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl Default for Color {
    fn default() -> Self {
        Color::BLACK
    }
}

impl Color {
    const BLACK: Color = Color::rgb(0x000000);
    const WHITE: Color = Color::rgb(0xFFFFFF);
    const RED: Color = Color::rgb(0xFF0000);
    const GREEN: Color = Color::rgb(0x00FF00);
    const BLUE: Color = Color::rgb(0x0000FF);
    const YELLOW: Color = Color::rgb(0xFFFF00);
    const PURPLE: Color = Color::rgb(0xFF00FF);
    const CYAN: Color = Color::rgb(0x00FFFF);
    const ORANGE: Color = Color::rgb(0xFF8000);
    const BROWN: Color = Color::rgb(0xAA5555);
    const GRAY: Color = Color::rgb(0x808080);
    const LIGHT_GRAY: Color = Color::rgb(0xC0C0C0);
    const DARK_GRAY: Color = Color::rgb(0x404040);
    const LIGHT_RED: Color = Color::rgb(0xFF8080);
    const LIGHT_GREEN: Color = Color::rgb(0x80FF80);
    const LIGHT_BLUE: Color = Color::rgb(0x8080FF);

    const PALETTE: [Self; 16] = [
        Self::BLACK,
        Self::WHITE,
        Self::RED,
        Self::CYAN,
        Self::PURPLE,
        Self::GREEN,
        Self::BLUE,
        Self::YELLOW,
        Self::ORANGE,
        Self::BROWN,
        Self::LIGHT_RED,
        Self::DARK_GRAY,
        Self::GRAY,
        Self::LIGHT_GREEN,
        Self::LIGHT_BLUE,
        Self::LIGHT_GRAY,
    ];

    const fn rgb(color: u32) -> Self {
        Self {
            r: ((color >> 16) & 0xFF) as u8,
            g: ((color >> 8) & 0xFF) as u8,
            b: (color & 0xFF) as u8,
            a: 255,
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    pos: Vec2,
    tex: Vec2,
}

const VERTICES: &[Vertex] = &[
    Vertex {
        pos: Vec2::new(0.0, 0.0),
        tex: Vec2::new(0.0, 1.0),
    }, // bottom left
    Vertex {
        pos: Vec2::new(1.0, 0.0),
        tex: Vec2::new(1.0, 1.0),
    }, // bottom right
    Vertex {
        pos: Vec2::new(1.0, 1.0),
        tex: Vec2::new(1.0, 0.0),
    }, // top right
    Vertex {
        pos: Vec2::new(0.0, 1.0),
        tex: Vec2::new(0.0, 0.0),
    }, // top left
];

const INDICES: &[u16] = &[
    0, 1, 2, // first triangle
    0, 2, 3, // second triangle
];

#[repr(C)]
#[derive(Debug, Copy, Clone, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniform {
    transform: Mat4,
}

struct State {
    window: Arc<Window>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    size: winit::dpi::PhysicalSize<u32>,
    surface: wgpu::Surface<'static>,
    surface_format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    cody_screen: wgpu::Texture,
    raw_pixels: Box<[Color; WIDTH as usize * HEIGHT as usize]>,
    cody_screen_bind_group: wgpu::BindGroup,
    uniform: Uniform,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
}

impl State {
    async fn new(window: Arc<Window>) -> State {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .unwrap();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .unwrap();

        let size = window.inner_size();

        let surface = instance.create_surface(window.clone()).unwrap();
        let cap = surface.get_capabilities(&adapter);
        let surface_format = cap.formats[0];

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertices"),
            contents: bytemuck::cast_slice(VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("indices"),
            contents: bytemuck::cast_slice(INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        let cody_screen = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cody screen"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let raw_pixels = Box::new([Color::default(); WIDTH as usize * HEIGHT as usize]);
        let cody_screen_view = cody_screen.create_view(&wgpu::TextureViewDescriptor::default());
        let cody_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("cody sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let cody_screen_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("cody screen bind group layout"),
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
        let cody_screen_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cody screen bind group"),
            layout: &cody_screen_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&cody_screen_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&cody_sampler),
                },
            ],
        });

        let uniform = Uniform::default();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("uniform bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("uniform bind group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline layout"),
            bind_group_layouts: &[&cody_screen_bind_group_layout, &uniform_bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: size_of::<Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: offset_of!(Vertex, pos) as wgpu::BufferAddress,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: offset_of!(Vertex, tex) as wgpu::BufferAddress,
                            shader_location: 1,
                        },
                    ],
                }],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(surface_format.into())],
            }),
            multiview: None,
            cache: None,
        });

        let state = State {
            window,
            device,
            queue,
            size,
            surface,
            surface_format,
            pipeline,
            vertex_buffer,
            index_buffer,
            cody_screen,
            raw_pixels,
            cody_screen_bind_group,
            uniform,
            uniform_buffer,
            uniform_bind_group,
        };

        // Configure surface for the first time
        state.configure_surface();

        state
    }

    fn get_window(&self) -> &Window {
        &self.window
    }

    fn configure_surface(&self) {
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: self.surface_format,
            // Request compatibility with the sRGB-format texture view we‘re going to create later.
            view_formats: vec![self.surface_format.add_srgb_suffix()],
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            width: self.size.width,
            height: self.size.height,
            desired_maximum_frame_latency: 2,
            present_mode: wgpu::PresentMode::AutoVsync,
        };
        self.surface.configure(&self.device, &surface_config);
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.size = new_size;

        // reconfigure the surface
        self.configure_surface();
    }

    fn render_pixels(&mut self, memory: &Arc<Mutex<impl Memory>>) {
        let mut memory = memory.lock().unwrap();

        // set blanking register to 0 because we are drawing now
        memory.write_u8(0xD000, 0);

        let control = memory.read_u8(0xD001);
        let color = memory.read_u8(0xD002);

        let border_color = Color::PALETTE[(color & 0xF) as usize];
        self.raw_pixels.fill(border_color);

        // check screen disable flag
        if (control & 0x1) != 0 {
            return;
        }

        let base = memory.read_u8(0xD003);
        let _scroll = memory.read_u8(0xD004);
        let screen_color = memory.read_u8(0xD005);
        let _sprite = memory.read_u8(0xD006);

        let color_memory_start = 0xA000u16.wrapping_add(0x400 * (color >> 4) as u16);
        let screen_memory_start = 0xA000u16.wrapping_add(0x400 * (base >> 4) as u16);
        let character_memory_start = 0xA000u16.wrapping_add(0x800 * (base & 0xF) as u16);

        for i in 0..1000 {
            let cx = i % 40;
            let cy = i / 40;

            let character = memory.read_u8(screen_memory_start.wrapping_add(i));
            let local_color = memory.read_u8(color_memory_start.wrapping_add(i));

            for yy in 0..8 {
                let char_row_data =
                    memory.read_u8(character_memory_start.wrapping_add(8 * character as u16 + yy));
                for xx in 0..4 {
                    let char_pixel_data = (char_row_data >> (2 * (3 - xx))) & 0x3;
                    let palette_index = match char_pixel_data {
                        0 => local_color & 0xF,
                        1 => local_color >> 4,
                        2 => screen_color & 0xF,
                        3 => screen_color >> 4,
                        _ => unreachable!(),
                    };

                    let pixel_pos = (cy as usize * 8 + yy as usize + BORDER_Y as usize)
                        * WIDTH as usize
                        + (cx as usize * 4 + xx as usize + BORDER_X as usize);
                    let pixel = &mut self.raw_pixels[pixel_pos];
                    *pixel = Color::PALETTE[palette_index as usize];
                }
            }
        }
    }

    fn render(&mut self, memory: &Arc<Mutex<impl Memory>>) {
        // update uniform
        {
            const TARGET_WIDTH: u32 = 640;
            const TARGET_HEIGHT: u32 = 480;

            let size = self.window.inner_size();
            let w = if size.width > 0 {
                size.width as f32
            } else {
                1.0
            };
            let h = if size.height > 0 {
                size.height as f32
            } else {
                1.0
            };

            let width_scale = w / TARGET_WIDTH as f32;
            let height_scale = h / TARGET_HEIGHT as f32;
            let (scale, offset_x, offset_y) = if width_scale <= height_scale {
                let height = width_scale * TARGET_HEIGHT as f32;
                (width_scale, 0.0, (h - height) / 2.0)
            } else {
                let width = height_scale * TARGET_WIDTH as f32;
                (height_scale, (w - width) / 2.0, 0.0)
            };

            let projection = Mat4::orthographic_rh(0.0, w, 0.0, h, 0.0, 1.0);
            let transform = Mat4::from_scale_rotation_translation(
                Vec3::new(
                    TARGET_WIDTH as f32 * scale,
                    TARGET_HEIGHT as f32 * scale,
                    1.0,
                ),
                Quat::IDENTITY,
                Vec3::new(offset_x, offset_y, 0.0),
            );
            self.uniform.transform = projection * transform;
        }

        self.render_pixels(memory);

        // Create texture view
        let surface_texture = self
            .surface
            .get_current_texture()
            .expect("failed to acquire next swapchain texture");
        let texture_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor {
                // Without add_srgb_suffix() the image we will be working with
                // might not be "gamma correct".
                format: Some(self.surface_format.add_srgb_suffix()),
                ..Default::default()
            });

        // upload raw pixel data
        self.queue.write_texture(
            self.cody_screen.as_image_copy(),
            bytemuck::cast_slice(&*self.raw_pixels),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(WIDTH * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
        );

        // upload uniform data
        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.uniform]),
        );

        // Renders a GREEN screen
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            // Create the renderpass which will clear the screen.
            let mut renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear screen"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &texture_view,
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

            // If you wanted to call any drawing commands, they would go here.
            renderpass.set_pipeline(&self.pipeline);
            renderpass.set_bind_group(0, &self.cody_screen_bind_group, &[]);
            renderpass.set_bind_group(1, &self.uniform_bind_group, &[]);
            renderpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            renderpass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            renderpass.draw_indexed(0..INDICES.len() as u32, 0, 0..1);
        }

        // Submit the command in the queue to execute
        self.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        surface_texture.present();
    }

    fn on_keyboard_input(&self, via_device: &Arc<Mutex<Via>>, code: KeyCode, pressed: bool) {
        let mut via_device = via_device.lock().unwrap();
        let cody_code = match code {
            KeyCode::KeyQ => 1,
            KeyCode::KeyE => 2,
            KeyCode::KeyT => 3,
            KeyCode::KeyU => 4,
            KeyCode::KeyO => 5,
            KeyCode::KeyA => 6,
            KeyCode::KeyD => 7,
            KeyCode::KeyG => 8,
            KeyCode::KeyJ => 9,
            KeyCode::KeyL => 10,
            KeyCode::ShiftLeft | KeyCode::ShiftRight => 11, // cody modifier (makes numbers)
            KeyCode::KeyX => 12,
            KeyCode::KeyV => 13,
            KeyCode::KeyN => 14,
            KeyCode::ControlLeft | KeyCode::ControlRight => 15, // meta modifier (makes punctuation)
            KeyCode::KeyZ => 16,
            KeyCode::KeyC => 17,
            KeyCode::KeyB => 18,
            KeyCode::KeyM => 19,
            KeyCode::Enter => 20, // arrow key
            KeyCode::KeyS => 21,
            KeyCode::KeyF => 22,
            KeyCode::KeyH => 23,
            KeyCode::KeyK => 24,
            KeyCode::Space => 25,
            KeyCode::KeyW => 26,
            KeyCode::KeyR => 27,
            KeyCode::KeyY => 28,
            KeyCode::KeyI => 29,
            KeyCode::KeyP => 30,
            _ => 0,
        };
        if cody_code > 0 {
            via_device.set_pressed(cody_code, pressed);
        }
    }
}

#[derive(Default)]
struct App<M> {
    state: Option<State>,
    memory: Arc<Mutex<M>>,
    via_device: Arc<Mutex<Via>>,
}

impl<M: Memory> ApplicationHandler for App<M> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Create window object
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes())
                .unwrap(),
        );

        let state = pollster::block_on(State::new(window.clone()));
        self.state = Some(state);

        window.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let state = self.state.as_mut().unwrap();
        match event {
            WindowEvent::CloseRequested => {
                println!("The close button was pressed; stopping");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                // Emits a new redraw requested event.
                state.get_window().request_redraw();
                state.render(&self.memory);
            }
            WindowEvent::Resized(size) => {
                // Reconfigures the size of the surface. We do not re-render
                // here as this event is always followed up by redraw request.
                state.resize(size);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    state.on_keyboard_input(
                        &self.via_device,
                        code,
                        event.state == ElementState::Pressed,
                    );
                }
            }
            _ => {}
        }
    }
}

#[derive(Debug, Copy, Clone, Default)]
struct Via {
    registers: [u8; 16],
    key_state: [u8; 8],
}

impl Via {
    fn read_iora(&mut self) -> u8 {
        let ddr = self.registers[3];
        let ior = self.registers[1];
        assert_eq!(ddr, 0x7);
        let output = ior & ddr;
        self.key_state[output as usize] | output
    }

    fn set_pressed(&mut self, code: u8, pressed: bool) {
        let bit = (code % 5) + 3;
        let index = code / 5;
        let mask = 1 << bit;
        if pressed {
            self.key_state[index as usize] &= !mask;
        } else {
            self.key_state[index as usize] |= mask;
        }
    }
}

impl MemoryDevice for Via {
    fn read(&mut self, address: u16) -> Option<u8> {
        match address {
            0x9F01 => Some(self.read_iora()),
            0x9F00..=0x9F0F => Some(self.registers[(address - 0x9F00) as usize]),
            _ => None,
        }
    }

    fn write(&mut self, address: u16, value: u8) -> Option<()> {
        match address {
            0x9F00..=0x9F0F => {
                self.registers[(address - 0x9F00) as usize] = value;
                Some(())
            }
            _ => None,
        }
    }
}

pub fn start() {
    // wgpu uses `log` for all of our logging, so we initialize a logger with the `env_logger` crate.
    //
    // To change the log level, set the `RUST_LOG` environment variable. See the `env_logger`
    // documentation for more information.
    env_logger::init();

    let mut f = File::open("codybasic.bin").unwrap();
    let mut data = vec![];
    f.read_to_end(&mut data).unwrap();
    let memory = Arc::new(Mutex::new(OverlayMemory::from_memory(
        Contiguous::from_bytes_at(&data, 0xE000),
    )));
    let via_device = Arc::new(Mutex::new(Via::default()));
    memory.lock().unwrap().add_overlay(Arc::clone(&via_device));

    let interrupt_provider = Arc::new(Mutex::new(SimpleInterruptProvider::default()));
    let mut cpu = Cpu::new(Arc::clone(&memory), Arc::clone(&interrupt_provider));
    thread::spawn(move || {
        cpu.run();
    });

    // TODO: make this use the VIA registers for configuration
    let mut interrupt_trigger = Arc::clone(&interrupt_provider);
    thread::spawn(move || {
        loop {
            thread::sleep(std::time::Duration::from_secs_f64(1.0 / 60.0));
            interrupt_trigger.trigger_irq();
        }
    });

    let event_loop = EventLoop::new().unwrap();

    // When the current loop iteration finishes, immediately begin a new
    // iteration regardless of whether or not new events are available to
    // process. Preferred for applications that want to render as fast as
    // possible, like games.
    event_loop.set_control_flow(ControlFlow::Poll);

    // When the current loop iteration finishes, suspend the thread until
    // another event arrives. Helps keeping CPU utilization low if nothing
    // is happening, which is preferred if the application might be idling in
    // the background.
    // event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App {
        state: None,
        memory,
        via_device,
    };
    event_loop.run_app(&mut app).unwrap();
}
