use clap::Parser;
use clap_num::maybe_hex;
use cody_emulator::assembler::disassemble;
use cody_emulator::cpu;
use cody_emulator::frontend;
use log::info;
use std::env;
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Load the given binary file into memory (by default at address 0xE000), load address can be changed with the --load-address option
    file: Option<PathBuf>,

    /// DEPRECATED (use --cartridge instead): Load the binary file as a cartridge, expects the file to have a cartridge header
    #[arg(long, default_value_t = false)]
    #[deprecated = "use --cartridge instead"]
    as_cartridge: bool,

    /// Change the load address of the binary file, default is 0xE000
    #[arg(long, value_parser=maybe_hex::<u16>)]
    load_address: Option<u16>,

    /// Load the given binary file into the RAM (address 0x0200)
    #[arg(long)]
    ram: Option<PathBuf>,

    /// Load the given binary file into the shared RAM (address 0xA000)
    #[arg(long)]
    shared_ram: Option<PathBuf>,

    /// Load the given binary file into the ROM (address 0xE000)
    #[arg(long)]
    rom: Option<PathBuf>,

    /// Load the given binary file as a cartridge, expects the file to have a cartridge header, can be used multiple times
    #[arg(long)]
    cartridge: Vec<PathBuf>,

    /// Override Non-Maskable Interrupt Vector (0xFFFA)
    #[arg(long, value_parser=maybe_hex::<u16>)]
    nmi_vector: Option<u16>,

    /// Override Reset Vector (0xFFFC)
    #[arg(long, value_parser=maybe_hex::<u16>)]
    reset_vector: Option<u16>,

    /// Override Interrupt Vector (0xFFFE)
    #[arg(long, value_parser=maybe_hex::<u16>)]
    irq_vector: Option<u16>,

    /// Path of file used to fill the UART1 receive buffer with bytes
    #[arg(long)]
    uart1_source: Option<PathBuf>,

    /// This option will normalize newlines when reading text data for the UART (always use with BASIC code!).
    ///
    /// Use this when your input text file might have CRLF-style line endings or to make sure it works for CodyBASIC's LOAD 1,0 command.
    #[arg(long, default_value_t = false)]
    fix_newlines: bool,

    /// Emulate the keyboard by physically mapping the cody keyboard, without respecting the host's layout.
    #[arg(long, default_value_t = false)]
    physical_keyboard: bool,

    /// Run the cpu as fast as possible.
    #[arg(long, default_value_t = false)]
    fast: bool,

    /// Make the NMI/RESET/IRQ vectors writable during runtime.
    #[arg(long, default_value_t = false)]
    writable_vectors: bool,

    /// Each time this option is added increases the default logging level
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

pub fn main() {
    let cli = Cli::parse();

    // To change the log level, set the `RUST_LOG` environment variable. See the `env_logger`
    // documentation for more information.
    unsafe {
        if env::var(env_logger::DEFAULT_FILTER_ENV).is_err() {
            match cli.verbose {
                0 => env::set_var(env_logger::DEFAULT_FILTER_ENV, "warn"),
                1 => env::set_var(env_logger::DEFAULT_FILTER_ENV, "info"),
                2 => env::set_var(env_logger::DEFAULT_FILTER_ENV, "debug"),
                3.. => env::set_var(env_logger::DEFAULT_FILTER_ENV, "trace"),
            }
        }
    }
    env_logger::init();

    let mut memory_map = vec![];

    #[allow(deprecated)]
    if let Some(rom) = &cli.file {
        memory_map.push(frontend::MemoryMapEntry {
            source: frontend::DataSource::Path(rom.clone()),
            target: if cli.as_cartridge {
                frontend::MemoryMapEntryTarget::Cartridge
            } else {
                frontend::MemoryMapEntryTarget::Target {
                    pos: cli.load_address.unwrap_or(frontend::ROM_START),
                    end: None,
                }
            },
        });
    }
    if let Some(ram) = &cli.ram {
        memory_map.push(frontend::MemoryMapEntry {
            source: frontend::DataSource::Path(ram.clone()),
            target: frontend::MemoryMapEntryTarget::Target {
                pos: frontend::FREE_RAM_START,
                end: None,
            },
        });
    }
    if let Some(shared_ram) = &cli.shared_ram {
        memory_map.push(frontend::MemoryMapEntry {
            source: frontend::DataSource::Path(shared_ram.clone()),
            target: frontend::MemoryMapEntryTarget::Target {
                pos: frontend::PROPELLER_RAM_SIZE,
                end: None,
            },
        });
    }
    if let Some(rom) = &cli.rom {
        memory_map.push(frontend::MemoryMapEntry {
            source: frontend::DataSource::Path(rom.clone()),
            target: frontend::MemoryMapEntryTarget::Target {
                pos: frontend::ROM_START,
                end: None,
            },
        });
    }
    for cartridge in &cli.cartridge {
        memory_map.push(frontend::MemoryMapEntry {
            source: frontend::DataSource::Path(cartridge.clone()),
            target: frontend::MemoryMapEntryTarget::Cartridge,
        });
    }

    if let Some(nmi_vector) = cli.nmi_vector {
        info!("Setting nmi vector to 0x{nmi_vector:04X}");
        memory_map.push(frontend::MemoryMapEntry {
            source: frontend::DataSource::Data(nmi_vector.to_le_bytes().into()),
            target: frontend::MemoryMapEntryTarget::Target {
                pos: cpu::NMI_VECTOR,
                end: None,
            },
        });
    }
    if let Some(reset_vector) = cli.reset_vector {
        info!("Setting reset vector to 0x{reset_vector:04X}");
        memory_map.push(frontend::MemoryMapEntry {
            source: frontend::DataSource::Data(reset_vector.to_le_bytes().into()),
            target: frontend::MemoryMapEntryTarget::Target {
                pos: cpu::RESET_VECTOR,
                end: None,
            },
        });
    }
    if let Some(irq_vector) = cli.irq_vector {
        info!("Setting irq vector to 0x{irq_vector:04X}");
        memory_map.push(frontend::MemoryMapEntry {
            source: frontend::DataSource::Data(irq_vector.to_le_bytes().into()),
            target: frontend::MemoryMapEntryTarget::Target {
                pos: cpu::IRQ_VECTOR,
                end: None,
            },
        });
    }

    frontend::start(frontend::FrontendOptions {
        memory_map,
        uart1: if let Some(uart1_source) = cli.uart1_source {
            let data = frontend::DataSource::Path(uart1_source);
            Some(if cli.fix_newlines {
                frontend::DataSource::NormalizedLineEndings(data.into())
            } else {
                data
            })
        } else {
            None
        },
        physical_keyboard: cli.physical_keyboard,
        fast: cli.fast,
        writable_vectors: cli.writable_vectors,
        guess_reset_vector: true,
    });
}

#[allow(dead_code)]
fn dis(data: &[u8]) {
    let instructions = disassemble(data);
    for insn in instructions {
        println!("{insn}");
    }
}
