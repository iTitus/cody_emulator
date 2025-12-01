use crate::cpu;
use crate::cpu::Cpu;
use crate::device::keyboard::{Keyboard, KeyboardEmulation};
use crate::device::uart::{UART1_BASE, UART2_BASE, Uart};
use crate::device::via::Via;
use crate::device::vid::{HEIGHT, Vid, WIDTH};
use crate::interrupt::{InterruptTrigger, SimpleInterruptProvider};
use crate::memory::Memory;
use crate::memory::contiguous::{Contiguous, Ram};
use crate::memory::mapped::MappedMemory;
use log::{debug, info};
use pixels::{Pixels, SurfaceTexture};
use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
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
) {
    let path = path.as_ref();
    info!(
        "Loading binary {}{}",
        path.display(),
        if as_cartridge { " as cartridge" } else { "" }
    );
    let mut data = {
        let mut buf = vec![];
        let mut f = File::open(path).unwrap();
        f.read_to_end(&mut buf).unwrap();
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
    memory.add_memory(0x9F00, 0x0100, Via::default());

    let uart1_device = Arc::new(Mutex::new(Uart::new(UART1_BASE)));
    memory.lock().unwrap().add_device(Arc::clone(&uart1_device));
    let uart2_device = Arc::new(Mutex::new(Uart::new(UART2_BASE)));
    memory.lock().unwrap().add_device(Arc::clone(&uart2_device));

    let interrupt_provider = SimpleInterruptProvider::default();
    let cpu = Cpu::new(memory, interrupt_provider);

    {
        let mut cpu = Cpu::new(Arc::clone(&memory), Arc::clone(&interrupt_provider));
        info!("Starting cpu");
        thread::spawn(move || {
            cpu.run();
        });
    }

    {
        // TODO: make this use the VIA registers for configuration
        info!("Starting interrupt timer");
        let mut interrupt_trigger = Arc::clone(&interrupt_provider);
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_secs_f64(1.0 / 60.0));
                interrupt_trigger.trigger_irq();
                println!("irq");
            }
        });
    }

    {
        // TODO: better UART support
        let uart1 = Arc::clone(&uart1_device);
        let mut uart1_source: VecDeque<u8> = if let Some(path) = uart1_source {
            let f = File::open(path.as_ref()).unwrap();
            let mut r = BufReader::new(f);
            if fix_newlines {
                let mut data = VecDeque::new();
                for l in r.lines().map_while(Result::ok).filter(|l| !l.is_empty()) {
                    data.extend(l.bytes());
                    data.push_back(b'\n');
                }
                // CodyBASIC requires an empty line to terminate the LOAD command
                data.push_back(b'\n');
                data
            } else {
                let mut buf = vec![];
                r.read_to_end(&mut buf).unwrap();
                VecDeque::from(buf)
            }
        } else {
            VecDeque::new()
        };
        let source_size = uart1_source.len();
        info!("Starting UART thread");
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_secs_f64(7.0 / 19200.0));
                let mut uart1 = uart1.lock().unwrap();
                uart1.update_state();
                if uart1.is_enabled() {
                    // transmit
                    while let Some(c) = uart1.transmit_buffer.pop() {
                        // discard
                        debug!("UART1 tx: {:?} ({c})", c as char);
                    }

                    // receive
                    while !uart1.receive_buffer.is_full() {
                        if let Some(value) = uart1_source.pop_front() {
                            uart1.receive_buffer.push(value);
                            debug!(
                                "UART1 rx: push byte {:?} ({value}), remaining {}/{source_size}",
                                value as char,
                                uart1_source.len()
                            )
                        } else {
                            break;
                        }
                    }
                }
                drop(uart1);
            }
        });
    }

    let vid_device = Arc::new(Mutex::new(Vid::new(Arc::clone(&memory))));
    {
        const FPS: u64 = 60;
        const NANOS_PER_SEC: u64 = Duration::from_secs(1).as_nanos() as u64;
        const DELTA_TIME: Duration = Duration::from_nanos(NANOS_PER_SEC / FPS);
        const DRAWING_INTERVAL: Duration = Duration::from_nanos(NANOS_PER_SEC / FPS * 480 / 525);
        const BLANKING_INTERVAL: Duration = DELTA_TIME.checked_sub(DRAWING_INTERVAL).unwrap();

        // fake blanking register
        let mut memory = Arc::clone(&memory);
        thread::spawn(move || {
            loop {
                // set blanking register to 0 because we are "drawing" now
                memory.write_u8(0xD000, 0);
                thread::sleep(DRAWING_INTERVAL);

                // reset blanking register to 1 for blanking interval
                memory.write_u8(0xD000, 1);
                println!("vblank");
                thread::sleep(BLANKING_INTERVAL);
            }
        });
    }

    let mut app = App {
        state: None,
        memory,
        vid_device,
        keyboard_device: Arc::new(Mutex::new(Keyboard::new(
            if physical_keyboard {
                KeyboardEmulation::Physical
            } else {
                KeyboardEmulation::Logical
            },
            via_device,
        ))),
        last_frame: None,
        input: WinitInputHelper::new(),
    };

    info!("Starting window event loop");
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut app).unwrap();
}

struct App<M> {
    state: Option<State>,
    memory: Arc<Mutex<M>>,
    vid_device: Arc<Mutex<Vid<M>>>,
    keyboard_device: Arc<Mutex<Keyboard>>,
    last_frame: Option<Instant>,
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
                .unwrap(),
        );
        let pixels = {
            let window_size = window.inner_size();
            let surface_texture =
                SurfaceTexture::new(window_size.width, window_size.height, Arc::clone(&window));
            Pixels::new(WIDTH, HEIGHT, surface_texture).unwrap()
        };
        self.state = Some(State { window, pixels });
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if self.input.process_window_event(&event) {
            let Some(state) = &mut self.state else {
                return;
            };

            let raw_pixels = state.pixels.frame_mut();
            self.vid_device
                .lock()
                .unwrap()
                .render_pixels(bytemuck::cast_slice_mut(raw_pixels));
            state.pixels.render().unwrap();
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        self.input.process_device_event(&event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.input.end_step();
        // println!("step duration: {:?}", self.input.delta_time());

        if self.input.close_requested() || self.input.destroyed() {
            event_loop.exit();
            return;
        }

        self.keyboard_device.lock().unwrap().update(&self.input);
        println!("key update");

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
                .unwrap();
        }

        state.window.request_redraw();
    }
}
