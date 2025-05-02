use clap::Parser;
use cody_emulator::assembler::disassemble;
use cody_emulator::device::vid;
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Binary file
    file: PathBuf,
}

pub fn main() {
    let cli = Cli::parse();
    vid::start(&cli.file);
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
