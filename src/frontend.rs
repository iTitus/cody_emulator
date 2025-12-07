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
use pixels::{Pixels, SurfaceTexture};
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
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

#[allow(clippy::too_many_arguments)]
pub fn start(
    path: impl AsRef<Path>,
    as_cartridge: bool,
    mut load_address: Option<u16>,
    reset_vector: Option<u16>,
    irq_vector: Option<u16>,
    nmi_vector: Option<u16>,
    uart1_source: Option<impl AsRef<Path>>,
    fix_newlines: bool,
    physical_keyboard: bool,
    fast: bool,
) {
    let path = path.as_ref();
    info!(
        "Loading binary {}{}",
        path.display(),
        if as_cartridge { " as cartridge" } else { "" }
    );
    let mut data = {
        let mut buf = vec![];
        let mut f = File::open(path).expect("error opening binary");
        f.read_to_end(&mut buf).expect("io error reading binary");
        buf
    };

    if as_cartridge {
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

        data = data.drain(4..(len + 4)).collect();
        load_address = load_address.or(Some(cartridge_load_address));
    }

    assert!(!data.is_empty(), "data must not be empty");
    let load_address = load_address.unwrap_or(0xE000);
    info!(
        "Loading data at address 0x{load_address:04X}-0x{:04X}",
        (load_address as usize + data.len() - 1).min(0xFFFF)
    );

    let mut ram = Contiguous::new_ram(0xA000);
    let mut propeller_ram = Contiguous::new_ram(0x4000);
    let mut rom = Contiguous::new_rom(0x2000);

    if load_address >= 0xE000 {
        rom.force_write_all(load_address - 0xE000, &data);
    } else if load_address >= 0xA000 {
        let address = load_address - 0xA000;

        let mut remaining = data.len();
        let to_copy = remaining.min((0x4000 - address) as usize);
        propeller_ram.force_write_all(address, &data[..to_copy]);

        remaining -= to_copy;
        if remaining > 0 {
            rom.force_write_all(0, &data[to_copy..]);
        }
    } else {
        let mut remaining = data.len();
        let to_copy = remaining.min((0xA000 - load_address) as usize);
        ram.force_write_all(load_address, &data[..to_copy]);

        let mut offset = to_copy;
        remaining -= to_copy;
        let to_copy = remaining.min(0x4000);
        if remaining > 0 {
            propeller_ram.force_write_all(0, &data[offset..(offset + to_copy)]);

            offset += to_copy;
            remaining -= to_copy;
            if remaining > 0 {
                rom.force_write_all(0, &data[offset..]);
            }
        }
    }
    drop(data);

    if let Some(reset_vector) = reset_vector.or(if as_cartridge {
        Some(load_address)
    } else {
        None
    }) {
        info!("Setting reset vector to 0x{reset_vector:04X}");
        rom.force_write_u16(cpu::RESET_VECTOR - 0xE000, reset_vector);
    }
    if let Some(irq_vector) = irq_vector {
        info!("Setting irq vector to 0x{irq_vector:04X}");
        rom.force_write_u16(cpu::IRQ_VECTOR - 0xE000, irq_vector);
    }
    if let Some(nmi_vector) = nmi_vector {
        info!("Setting nmi vector to 0x{nmi_vector:04X}");
        rom.force_write_u16(cpu::NMI_VECTOR - 0xE000, nmi_vector);
    }

    let mut memory = MappedMemory::new();
    memory.add_memory(0x0000, 0xA000, ram);
    memory.add_memory(0xA000, 0x4000, propeller_ram);
    memory.add_memory(0xE000, 0x2000, rom);

    let via = Via::default();
    let key_state = Rc::clone(via.get_key_state());
    memory.add_memory(0x9F00, 0x0100, via);

    // TODO: better UART support
    let uart1_data: Vec<u8> = if let Some(path) = uart1_source {
        let f = File::open(path.as_ref()).expect("error opening uart1 data file");
        let mut r = BufReader::new(f);
        if fix_newlines {
            let mut data = vec![];
            for l in r.lines().map_while(Result::ok).filter(|l| !l.is_empty()) {
                data.extend(l.bytes());
                data.push(b'\n');
            }
            // CodyBASIC requires an empty line to terminate the LOAD command
            data.push(b'\n');
            data
        } else {
            let mut buf = vec![];
            r.read_to_end(&mut buf)
                .expect("error reading uart1 data file");
            buf
        }
    } else {
        vec![]
    };
    let uart1 = Uart::new(UartSource::new(uart1_data));
    let (_uart1_rx, _uart1_tx) = (
        Rc::clone(uart1.get_receive_buffer()),
        Rc::clone(uart1.get_transmit_buffer()),
    );
    memory.add_memory(UART1_BASE, UART_END, uart1);
    let uart2 = Uart::new(UartSource::empty());
    let (_uart2_rx, _uart2_tx) = (
        Rc::clone(uart2.get_receive_buffer()),
        Rc::clone(uart2.get_transmit_buffer()),
    );
    memory.add_memory(UART2_BASE, UART_END, uart2);

    memory.add_memory(0xD000, 0x1, BlankingRegister::default());

    let mut app = App {
        state: None,
        cpu: Cpu::new(memory),
        keyboard: Keyboard::new(
            if physical_keyboard {
                KeyboardEmulation::Physical
            } else {
                KeyboardEmulation::Logical
            },
            key_state,
        ),
        fast,
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
    window: Arc<Window>,
    pixels: Pixels<'static>,
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
        let pixels = {
            let window_size = window.inner_size();
            let surface_texture =
                SurfaceTexture::new(window_size.width, window_size.height, Arc::clone(&window));
            Pixels::new(WIDTH, HEIGHT, surface_texture).expect("pixels framebuffer created")
        };
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
