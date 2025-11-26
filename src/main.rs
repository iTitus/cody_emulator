use clap::Parser;
use clap_num::maybe_hex;
use cody_emulator::assembler::disassemble;
use cody_emulator::device::vid;
use std::env;
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Binary file
    file: PathBuf,

    /// Load the binary file as a cartridge, expects the file to have a cartridge header
    #[arg(long, default_value_t = false)]
    as_cartridge: bool,

    /// Load address, default value is 0xE000
    #[arg(long, value_parser=maybe_hex::<u16>)]
    load_address: Option<u16>,

    /// Override Reset Vector (0xFFFC)
    #[arg(long, value_parser=maybe_hex::<u16>)]
    reset_vector: Option<u16>,

    /// Override Interrupt Vector (0xFFFE)
    #[arg(long, value_parser=maybe_hex::<u16>)]
    irq_vector: Option<u16>,

    /// Override Non-maskable Interrupt Vector (0xFFFA)
    #[arg(long, value_parser=maybe_hex::<u16>)]
    nmi_vector: Option<u16>,

    /// Path of file used to fill the UART1 receive buffer with bytes
    #[arg(long)]
    uart1_source: Option<PathBuf>,

    /// This option will normalize newlines when reading text data for the UART.
    ///
    /// Use this when your input text file might have CRLF-style line endings or to make sure it works for CodyBASIC's LOAD 1,0 command.
    #[arg(long, default_value_t = false)]
    fix_newlines: bool,

    /// Emulate the keyboard by physically mapping the cody keyboard, without respecting the host's layout.
    #[arg(long, default_value_t = false)]
    physical_keyboard: bool,

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

    vid::start(
        &cli.file,
        cli.as_cartridge,
        cli.load_address,
        cli.reset_vector,
        cli.irq_vector,
        cli.nmi_vector,
        cli.uart1_source.as_deref(),
        cli.fix_newlines,
        cli.physical_keyboard,
    );
}

#[allow(dead_code)]
fn dis(data: &[u8]) {
    let instructions = disassemble(data);
    for insn in instructions {
        println!("{insn}");
    }
}
