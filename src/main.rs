use clap::Parser;
use clap_num::maybe_hex;
use cody_emulator::assembler::disassemble;
use cody_emulator::device::vid;
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Binary file
    file: PathBuf,

    /// Load given file as cartridge in addition to binary
    #[arg(long)]
    cartridge: Option<PathBuf>,

    /// Load address
    #[arg(long, value_parser=maybe_hex::<u16>, default_value = "0xE000")]
    load_address: u16,

    /// Override Reset Vector (0xFFFC)
    #[arg(long, value_parser=maybe_hex::<u16>)]
    reset_vector: Option<u16>,

    /// Override Interrupt Vector (0xFFFE)
    #[arg(long, value_parser=maybe_hex::<u16>)]
    irq_vector: Option<u16>,

    /// Override Non-maskable Interrupt Vector (0xFFFA)
    #[arg(long, value_parser=maybe_hex::<u16>)]
    nmi_vector: Option<u16>,
}

pub fn main() {
    // To change the log level, set the `RUST_LOG` environment variable. See the `env_logger`
    // documentation for more information.
    env_logger::init();

    let cli = Cli::parse();
    vid::start(
        &cli.file,
        cli.cartridge.as_deref(),
        cli.load_address,
        cli.reset_vector,
        cli.irq_vector,
        cli.nmi_vector,
    );
    /*let mut f = File::open("codybasic.bin").unwrap();
    let mut data = vec![];
    f.read_to_end(&mut data).unwrap();
    dis(&data);*/
}

#[allow(dead_code)]
fn dis(data: &[u8]) {
    let instructions = disassemble(data);
    for insn in instructions {
        println!("{insn}");
    }
}
