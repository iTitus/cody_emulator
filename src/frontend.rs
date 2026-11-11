use crate::cpu;
use crate::cpu::Cpu;
use crate::device::blanking::BlankingRegister;
use crate::device::keyboard::{Keyboard, KeyboardEmulation};
use crate::device::uart::{UART_END, UART1_BASE, UART2_BASE, Uart, UartSource};
use crate::device::via::Via;
use crate::device::vid;
use crate::device::vid::{HEIGHT, WIDTH};
use crate::memory::Memory;
use crate::memory::contiguous::Contiguous;
use crate::memory::mapped::MappedMemory;
use log::{info, trace};
use pixels::{Pixels, ScalingMode, SurfaceTexture};
use std::io::{BufRead, Cursor};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, DeviceId, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};
use winit_input_helper::WinitInputHelper;

pub const RAM_SIZE: u16 = 0xA000;
pub const PROPELLER_RAM_SIZE: u16 = 0x4000;
pub const ROM_SIZE: u16 = 0x2000;
pub const RAM_START: u16 = 0x0000;
pub const PROPELLER_RAM_START: u16 = RAM_START + RAM_SIZE;
pub const ROM_START: u16 = RAM_START + RAM_SIZE + PROPELLER_RAM_SIZE;
pub const FREE_RAM_START: u16 = 0x0200;

#[derive(Debug)]
pub struct FrontendOptions {
    pub memory_map: Vec<MemoryMapEntry>,
    pub uart1: Option<DataSource>,
    pub physical_keyboard: bool,
    pub fast: bool,
    pub writable_vectors: bool,
    /// Try to guess reset vector based on memory map
    pub guess_reset_vector: bool,
}

#[derive(Debug)]
pub struct MemoryMapEntry {
    pub source: DataSource,
    pub target: MemoryMapEntryTarget,
}

#[derive(Debug)]
pub enum DataSource {
    Path(PathBuf),
    Data(Vec<u8>),
    Offset {
        source: Box<DataSource>,
        pos: usize,
        length: Option<usize>,
    },
    NormalizedLineEndings(Box<DataSource>),
}

impl DataSource {
    fn read(&self) -> std::io::Result<Vec<u8>> {
        match self {
            DataSource::Path(path) => std::fs::read(path),
            DataSource::Data(bytes) => Ok(bytes.clone()),
            DataSource::Offset {
                source,
                pos,
                length,
            } => {
                let all_bytes = source.read()?;
                if *pos >= all_bytes.len() {
                    return Err(std::io::Error::other("out of range".to_string()));
                }
                if let Some(length) = length {
                    let end = all_bytes.len().min(*pos + *length);
                    Ok(all_bytes[*pos..end].to_vec())
                } else {
                    Ok(all_bytes[*pos..].to_vec())
                }
            }
            DataSource::NormalizedLineEndings(source) => {
                let mut bytes = Cursor::new(source.read()?);
                let mut result = vec![];
                loop {
                    let mut buf = vec![];
                    let bytes_read = bytes.read_until(b'\n', &mut buf)?;
                    if bytes_read == 0 {
                        break;
                    }

                    if buf.ends_with(b"\n") {
                        buf.pop();
                        if buf.ends_with(b"\r") {
                            buf.pop();
                        }
                    }

                    // CodyBASIC does not accept empty lines
                    if buf.is_empty() {
                        continue;
                    }

                    result.append(&mut buf);
                    result.push(b'\n');
                }
                result.push(b'\n'); // CodyBASIC requires an additional newline at the end
                Ok(result)
            }
        }
    }
}

#[derive(Debug)]
pub enum MemoryMapEntryTarget {
    Cartridge,
    Target { pos: u16, end: Option<u16> },
}

pub fn start(options: FrontendOptions) {
    let mut memory = MappedMemory::new();
    {
        let ram = Contiguous::new_ram(RAM_SIZE);
        let propeller_ram = Contiguous::new_ram(PROPELLER_RAM_SIZE);
        let rom = Contiguous::new_rom(ROM_SIZE);
        memory.add_memory(RAM_START, PROPELLER_RAM_START - 1, ram);
        memory.add_memory(PROPELLER_RAM_START, ROM_START - 1, propeller_ram);
        memory.add_memory(ROM_START, 0xFFFF, rom);
        if options.writable_vectors {
            // make vectors writable
            memory.add_memory(
                cpu::NMI_VECTOR,
                0xFFFF,
                Contiguous::new_ram(0xFFFF - cpu::NMI_VECTOR + 1),
            );
        }
    }

    let mut load_addresses = vec![];
    let mut written_to_reset_vector = false;
    for e in &options.memory_map {
        info!("Reading data from {:?}", e.source);
        let mut data = e.source.read().expect("io error");
        info!("Got {} bytes", data.len());
        assert!(!data.is_empty());

        let (start, end) = match &e.target {
            MemoryMapEntryTarget::Cartridge => {
                info!("Interpreting as cartridge");
                assert!(data.len() >= 4, "cartridge header must be at least 4 bytes");
                let cartridge_load_address = u16::from_le_bytes(
                    data[0..2]
                        .try_into()
                        .expect("cartridge header must be at least 4 bytes"),
                );
                let cartridge_end_address = u16::from_le_bytes(
                    data[2..4]
                        .try_into()
                        .expect("cartridge header must be at least 4 bytes"),
                );
                let len = (cartridge_end_address as usize)
                    .checked_sub(cartridge_load_address as usize)
                    .and_then(|len| len.checked_add(1))
                    .expect("cartridge start address must be <= end address");
                assert!(
                    data.len() - 4 >= len,
                    "cartridge data len {} must be >= implied header len {len}",
                    data.len() - 4
                );
                data = data[4..len + 4].to_vec();
                (cartridge_load_address, cartridge_end_address)
            }
            MemoryMapEntryTarget::Target { pos, end } => (
                *pos,
                if let Some(end) = end {
                    (*end).min((*pos as usize + data.len() - 1) as u16)
                } else {
                    (*pos as usize + data.len() - 1) as u16
                },
            ),
        };

        assert!(
            start <= end && ((end - start) as usize) < data.len(),
            "target area must be of equals or smaller size as the source data"
        );
        info!(
            "Writing {} bytes to 0x{start:04X}-0x{end:04X}",
            end as usize - start as usize + 1
        );

        // heuristic for valid memory locations
        if start <= FREE_RAM_START && end >= FREE_RAM_START {
            // everything before this address is zeropage/stack and should not contain executable code
            load_addresses.push(FREE_RAM_START);
        } else if (FREE_RAM_START..cpu::NMI_VECTOR).contains(&start) {
            // the vectors at the end of memory should not contain executable code
            load_addresses.push(start);
        }

        for (target_address, value) in (start..=end).zip(data) {
            if (cpu::RESET_VECTOR..=cpu::RESET_VECTOR + 1).contains(&target_address) {
                written_to_reset_vector = true;
            }

            memory.force_write_u8(target_address, value);
        }
    }

    if options.guess_reset_vector && !written_to_reset_vector && load_addresses.len() == 1 {
        let load_address = load_addresses[0];
        info!(
            "Using load address 0x{load_address:04X} as reset vector, because the reset vector was not written to"
        );
        memory.force_write_u16(cpu::RESET_VECTOR, load_address);
    }
    drop(load_addresses);

    let via = Via::default();
    let key_state = Rc::clone(via.get_key_state());
    memory.add_memory(0x9F00, 0x9FFF, via);

    // TODO: better UART support
    let uart1_data: Vec<u8> = if let Some(source) = &options.uart1 {
        info!("Loading UART1 from {source:?}");
        source.read().expect("io error")
    } else {
        vec![]
    };
    let uart1 = Uart::new(UartSource::new(uart1_data));
    let (_uart1_rx, _uart1_tx) = (
        Rc::clone(uart1.get_receive_buffer()),
        Rc::clone(uart1.get_transmit_buffer()),
    );
    memory.add_memory(UART1_BASE, UART1_BASE + UART_END - 1, uart1);
    let uart2 = Uart::new(UartSource::empty());
    let (_uart2_rx, _uart2_tx) = (
        Rc::clone(uart2.get_receive_buffer()),
        Rc::clone(uart2.get_transmit_buffer()),
    );
    memory.add_memory(UART2_BASE, UART2_BASE + UART_END - 1, uart2);

    memory.add_memory(0xD000, 0xD000, BlankingRegister::default());

    let mut app = App {
        state: None,
        cpu: Cpu::new(memory),
        keyboard: Keyboard::new(
            if options.physical_keyboard {
                KeyboardEmulation::Physical
            } else {
                KeyboardEmulation::Logical
            },
            key_state,
        ),
        fast: options.fast,
        last_frame_start: Instant::now(),
        input: WinitInputHelper::new(),
    };

    info!("Starting event loop");
    let event_loop = EventLoop::new().expect("event loop created");
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut app).expect("application running");
}

struct App<M> {
    state: Option<State>,
    cpu: Cpu<M>,
    keyboard: Keyboard,
    fast: bool,
    last_frame_start: Instant,
    input: WinitInputHelper,
}

struct State {
    pixels: Pixels<'static>,
    window: Arc<Window>,
}

impl<M: Memory> ApplicationHandler for App<M> {
    fn new_events(&mut self, _: &ActiveEventLoop, _: StartCause) {
        self.input.step();
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Cody")
                        .with_min_inner_size(LogicalSize::new(WIDTH, HEIGHT)),
                )
                .expect("window created"),
        );
        let mut pixels = {
            let window_size = window.inner_size();
            let surface_texture =
                SurfaceTexture::new(window_size.width, window_size.height, Arc::clone(&window));
            Pixels::new(WIDTH, HEIGHT, surface_texture).expect("pixels framebuffer created")
        };
        pixels.set_scaling_mode(ScalingMode::Fill);
        self.state = Some(State { window, pixels });
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if self.input.process_window_event(&event) {
            let Some(state) = &mut self.state else {
                return;
            };

            let raw_pixels = state.pixels.frame_mut();
            vid::render_pixels(&mut self.cpu.memory, bytemuck::cast_slice_mut(raw_pixels));
            state.pixels.render().expect("render error");
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        self.input.process_device_event(&event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.input.end_step();

        if self.input.close_requested() || self.input.destroyed() {
            // Drop GPU/surface resources while the event loop is still alive.
            self.state = None;
            event_loop.exit();
            return;
        }

        self.keyboard.update(&self.input);

        let Some(state) = &mut self.state else {
            return;
        };

        if let Some(size) = self.input.window_resized()
            && size.width > 0
            && size.height > 0
        {
            state
                .pixels
                .resize_surface(size.width, size.height)
                .expect("framebuffer resized");
        }

        const FPS: f64 = 60.0 / 1.001;
        const FRAME_NANOS: f64 = FPS / 1000000000.0;
        const FRAME_DURATION: Duration = Duration::from_nanos((1.0 / FRAME_NANOS) as u64);
        const _: () = assert!(FRAME_DURATION.as_nanos() > 0);

        let mut total_cycles = 0;
        let mut total_instructions = 0usize;
        let frame_time = if self.fast {
            while self.last_frame_start.elapsed() < FRAME_DURATION {
                total_cycles += self.cpu.step_instruction() as usize;
                total_instructions += 1;
            }
            let elapsed = self.last_frame_start.elapsed();
            self.last_frame_start = Instant::now();
            elapsed
        } else {
            // sleep to get to ~60 fps
            let elapsed = self.last_frame_start.elapsed();
            if elapsed < FRAME_DURATION {
                sleep(FRAME_DURATION - elapsed);
            }

            const CYCLE_FREQUENCY: f64 = 1000000.0;
            const CYCLE_FREQUENCY_NANOS: f64 = CYCLE_FREQUENCY / 1000000000.0;
            const CYCLE_DURATION: Duration =
                Duration::from_nanos((1.0 / CYCLE_FREQUENCY_NANOS) as u64);
            const _: () = assert!(CYCLE_DURATION.as_nanos() > 0);

            let now = Instant::now();
            let realtime_elapsed = now - self.last_frame_start;
            self.last_frame_start = now;
            let mut catchup = Duration::ZERO;
            while catchup < realtime_elapsed {
                let cycles = self.cpu.step_instruction();
                total_cycles += cycles as usize;
                total_instructions += 1;
                catchup += CYCLE_DURATION * cycles as u32;
            }

            realtime_elapsed
        };
        trace!(
            "frame time: {frame_time:?}, instructions: {total_instructions}, cycles: {total_cycles}"
        );

        state.window.request_redraw();
    }
}
