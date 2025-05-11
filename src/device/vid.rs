use crate::cpu::Cpu;
use crate::device::via::Via;
use crate::interrupt::{InterruptTrigger, SimpleInterruptProvider};
use crate::memory::{Contiguous, MappedMemory, Memory};
use glam::{Mat4, Quat, Vec2, Vec3};
use log::info;
use std::fs::File;
use std::io::Read;
use std::mem::offset_of;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
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

const CONTENT_WIDTH: u8 = 160;
const CONTENT_HEIGHT: u8 = 200;
const BORDER_X: u32 = 4;
const BORDER_Y: u32 = 8;
const WIDTH: u32 = CONTENT_WIDTH as u32 + 2 * BORDER_X;
const HEIGHT: u32 = CONTENT_HEIGHT as u32 + 2 * BORDER_Y;

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
    const RED: Color = Color::rgb(0x880000);
    const GREEN: Color = Color::rgb(0x00CC55);
    const BLUE: Color = Color::rgb(0x0000AA);
    const YELLOW: Color = Color::rgb(0xEEEE77);
    const PURPLE: Color = Color::rgb(0xCC44CC);
    const CYAN: Color = Color::rgb(0xAAFFEE);
    const ORANGE: Color = Color::rgb(0xDD8855);
    const BROWN: Color = Color::rgb(0x664400);
    const GRAY: Color = Color::rgb(0x777777);
    const LIGHT_GRAY: Color = Color::rgb(0xBBBBBB);
    const DARK_GRAY: Color = Color::rgb(0x333333);
    const LIGHT_RED: Color = Color::rgb(0xFF7777);
    const LIGHT_GREEN: Color = Color::rgb(0xAAFF66);
    const LIGHT_BLUE: Color = Color::rgb(0x0088FF);

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
            .request_device(&wgpu::DeviceDescriptor::default())
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
        let raw_pixels = vec![Color::default(); WIDTH as usize * HEIGHT as usize]
            .try_into()
            .unwrap();
        let cody_screen_view = cody_screen.create_view(&wgpu::TextureViewDescriptor::default());
        let cody_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("cody sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
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
                compilation_options: wgpu::PipelineCompilationOptions::default(),
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
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
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

        let control = memory.read_u8(0xD001);
        let color = memory.read_u8(0xD002);

        let border_color = Color::PALETTE[(color & 0xF) as usize];
        self.raw_pixels.fill(border_color);

        // check screen disable flag
        if (control & 0x1) != 0 {
            return;
        }
        let enable_v_scroll = (control & 0x2) != 0;
        let enable_h_scroll = (control & 0x4) != 0;
        let _enable_row_effects = (control & 0x8) != 0;
        let bitmap_mode = (control & 0x10) != 0;

        let base = memory.read_u8(0xD003);
        let (v_scroll_amount, h_scroll_amount) = {
            let scroll = memory.read_u8(0xD004);
            (
                if enable_v_scroll { scroll & 0x7 } else { 0 },
                if enable_h_scroll {
                    (scroll >> 4) & 0x3
                } else {
                    0
                },
            )
        };
        let screen_colors = memory.read_u8(0xD005);

        let color_memory_start = 0xA000u16.wrapping_add(0x400 * (color >> 4) as u16);
        let screen_memory_start = 0xA000u16.wrapping_add(0x400 * (base >> 4) as u16);
        let character_memory_start = 0xA000u16.wrapping_add(0x800 * (base & 0xF) as u16);

        // fine scrolling
        let width = CONTENT_WIDTH - if enable_h_scroll { 2 * 4 } else { 0 };
        let height = CONTENT_HEIGHT - if enable_v_scroll { 8 } else { 0 };
        let border_x = BORDER_X as usize + if enable_h_scroll { 2 * 2 } else { 0 };
        let border_y = BORDER_Y as usize + if enable_v_scroll { 4 } else { 0 };

        // background
        let cx_end = if enable_h_scroll { 39 } else { 40 };
        let cy_end = 25;
        for cy in 0..cy_end {
            for cx in 0..cx_end {
                let i = cy * 40 + cx;

                let character = if bitmap_mode {
                    // for bitmap mode: unused
                    0
                } else {
                    // for normal mode: the character code (CODSCII)
                    memory.read_u8(screen_memory_start.wrapping_add(i)) as u16
                };
                let local_colors = memory.read_u8(color_memory_start.wrapping_add(i));

                let xx_start = if enable_h_scroll && cx == 0 {
                    h_scroll_amount
                } else {
                    0
                };
                let xx_end = if enable_h_scroll && cx == cx_end - 1 {
                    h_scroll_amount
                } else {
                    4
                };
                let yy_start = if enable_v_scroll && cy == 0 {
                    v_scroll_amount
                } else {
                    0
                };
                let yy_end = if enable_v_scroll && cy == cy_end - 1 {
                    v_scroll_amount
                } else {
                    8
                };
                for yy in yy_start..yy_end {
                    let char_row_data = if bitmap_mode {
                        memory.read_u8(screen_memory_start.wrapping_add(8 * i + yy as u16))
                    } else {
                        memory
                            .read_u8(character_memory_start.wrapping_add(8 * character + yy as u16))
                    };
                    for xx in xx_start..xx_end {
                        let char_pixel_data = (char_row_data >> (2 * (3 - xx))) & 0x3;
                        let palette_index = match char_pixel_data {
                            0 => local_colors & 0xF,
                            1 => local_colors >> 4,
                            2 => screen_colors & 0xF,
                            3 => screen_colors >> 4,
                            _ => unreachable!(),
                        };

                        let pixel_pos = (cy as usize * 8 + yy as usize + border_y
                            - v_scroll_amount as usize)
                            * WIDTH as usize
                            + (cx as usize * 4 + xx as usize + border_x - h_scroll_amount as usize);
                        let pixel = &mut self.raw_pixels[pixel_pos];
                        *pixel = Color::PALETTE[palette_index as usize];
                    }
                }
            }
        }

        // sprites
        const SPRITE_WIDTH: u8 = 12;
        const SPRITE_HEIGHT: u8 = 21;

        let sprite = memory.read_u8(0xD006);
        let sprite_common_color = sprite & 0xF;
        let sprite_bank_start = 0xD080u16.wrapping_add(0x20 * ((sprite >> 4) as u16));
        for sprite_index in 0..8 {
            let sprite_data_start = sprite_bank_start.wrapping_add(4 * sprite_index);
            let sprite_pos_x = memory.read_u8(sprite_data_start);
            let sprite_pos_y = memory.read_u8(sprite_data_start.wrapping_add(1));
            let sprite_colors = memory.read_u8(sprite_data_start.wrapping_add(2));
            let sprite_location = 0xA000u16
                .wrapping_add(0x40 * memory.read_u8(sprite_data_start.wrapping_add(3)) as u16);

            if sprite_pos_x == 0 || sprite_pos_x >= width + SPRITE_WIDTH {
                continue;
            }
            if sprite_pos_y == 0 || sprite_pos_y >= height + SPRITE_HEIGHT {
                continue;
            }
            let sprite_start_x = SPRITE_WIDTH.saturating_sub(sprite_pos_x);
            let sprite_end_x = SPRITE_WIDTH - (sprite_pos_x.saturating_sub(width));
            let sprite_start_y = SPRITE_HEIGHT.saturating_sub(sprite_pos_y);
            let sprite_end_y = SPRITE_HEIGHT - (sprite_pos_y.saturating_sub(height));

            for sprite_y in sprite_start_y..sprite_end_y {
                for sprite_x in sprite_start_x..sprite_end_x {
                    let sprite_pixel_index = sprite_y * SPRITE_WIDTH + sprite_x;
                    let sprite_byte_index = sprite_pixel_index / 4;
                    let sprite_byte_bit_shift = 2 * (3 - (sprite_pixel_index % 4));
                    let sprite_pixel_data = (memory
                        .read_u8(sprite_location.wrapping_add(sprite_byte_index as u16))
                        >> sprite_byte_bit_shift)
                        & 0x3;
                    let palette_index = match sprite_pixel_data {
                        0 => continue, // transparent
                        1 => sprite_colors & 0xF,
                        2 => sprite_colors >> 4,
                        3 => sprite_common_color,
                        _ => unreachable!(),
                    };

                    let pixel_pos = (sprite_pos_y as usize + sprite_y as usize + border_y
                        - SPRITE_HEIGHT as usize)
                        * WIDTH as usize
                        + (sprite_pos_x as usize + sprite_x as usize + border_x
                            - SPRITE_WIDTH as usize);
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
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
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
            via_device.set_pressed(cody_code - 1, pressed);
        }
    }
}

struct App<M> {
    state: Option<State>,
    memory: Arc<Mutex<M>>,
    via_device: Arc<Mutex<Via>>,
    last_frame: Option<Instant>,
}

impl<M: Memory> ApplicationHandler for App<M> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Create window object
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes().with_title("Cody"))
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
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                // Emits a new redraw requested event.
                state.get_window().request_redraw();

                const FPS: u64 = 30;
                const FPS_DELTA_TIME: Duration =
                    Duration::from_nanos((Duration::from_secs(1).as_nanos() as u64) / FPS);
                const FPS_DRAWING_INTERVAL: Duration = Duration::from_nanos(
                    (Duration::from_secs(1).as_nanos() as u64) / FPS * 480 / 525,
                );
                let now = Instant::now();
                if self
                    .last_frame
                    .is_none_or(|last_frame| (now - last_frame) >= FPS_DELTA_TIME)
                {
                    self.last_frame = Some(now);

                    // set blanking register to 0 because we are drawing now
                    self.memory.write_u8(0xD000, 0);

                    state.render(&self.memory);
                } else if self
                    .last_frame
                    .is_some_and(|last_frame| (now - last_frame) >= FPS_DRAWING_INTERVAL)
                {
                    // reset blanking register to 1 for blanking interval
                    self.memory.write_u8(0xD000, 1);
                }
            }
            WindowEvent::Resized(size) => {
                // Reconfigures the size of the surface. We do not re-render
                // here as this event is always followed up by redraw request.
                if size.width > 0 && size.height > 0 {
                    state.resize(size);
                }
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

pub fn start(
    path: impl AsRef<Path>,
    cartridge: Option<impl AsRef<Path>>,
    load_address: u16,
    reset_vector: Option<u16>,
    irq_vector: Option<u16>,
    nmi_vector: Option<u16>,
) {
    info!("Loading binary {}", path.as_ref().display());
    let mut f = File::open(path).unwrap();
    let mut data = vec![];
    f.read_to_end(&mut data).unwrap();
    drop(f);

    info!("Loading data at address 0x{load_address:04X}");
    let mut memory = Contiguous::from_bytes_at(&data, load_address);

    if let Some(_cartridge) = cartridge {
        todo!("cartridges not implemented yet");
        /*info!("Loading cartridge {}", cartridge.as_ref().display());let mut fc = File::open(cartridge).unwrap();
        let mut cartridge_data = vec![];
        fc.read_to_end(&mut cartridge_data).unwrap();
        drop(fc);
        assert!(
            cartridge_data.len() >= 4,
            "cartridge binary must have at least 4 bytes"
        );

        // TODO: implement this via SPI instead and let codybasic load it
        let first = u16::from_le_bytes(cartridge_data[0..2].try_into().unwrap());
        let last = u16::from_le_bytes(cartridge_data[2..4].try_into().unwrap());
        let expected_len = last
            .checked_sub(first)
            .expect("illegal cartridge header: last > first") as usize
            + 1;
        assert_eq!(
            expected_len,
            cartridge_data.len() - 4,
            "illegal cartridge header: expected size does not match actual size"
        );
        info!("Override memory with cartridge data at 0x{first:04X}-0x{last:04X}");
        memory.0[first as usize..=last as usize].copy_from_slice(&cartridge_data[4..]);
        info!("Override reset vector with 0x{first:04X}");
        reset_vector = Some(first);*/
    }

    if let Some(reset_vector) = reset_vector {
        info!("Setting reset vector to 0x{reset_vector:04X}");
        memory.write_u16(0xFFFC, reset_vector);
    }
    if let Some(irq_vector) = irq_vector {
        info!("Setting irq vector to 0x{irq_vector:04X}");
        memory.write_u16(0xFFFE, irq_vector);
    }
    if let Some(nmi_vector) = nmi_vector {
        info!("Setting nmi vector to 0x{nmi_vector:04X}");
        memory.write_u16(0xFFFA, nmi_vector);
    }

    let memory = Arc::new(Mutex::new(MappedMemory::from_memory(memory)));
    let via_device = Arc::new(Mutex::new(Via::default()));
    memory.lock().unwrap().add_device(Arc::clone(&via_device));

    let interrupt_provider = Arc::new(Mutex::new(SimpleInterruptProvider::default()));
    let mut cpu = Cpu::new(Arc::clone(&memory), Arc::clone(&interrupt_provider));
    info!("Starting cpu");
    thread::spawn(move || {
        cpu.run();
    });

    // TODO: make this use the VIA registers for configuration
    info!("Starting interrupt timer");
    let mut interrupt_trigger = Arc::clone(&interrupt_provider);
    thread::spawn(move || {
        loop {
            thread::sleep(std::time::Duration::from_secs_f64(1.0 / 60.0));
            interrupt_trigger.trigger_irq();
        }
    });

    info!("Starting window event loop");
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
        last_frame: None,
    };
    event_loop.run_app(&mut app).unwrap();
}
