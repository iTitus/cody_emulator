use std::fs::File;
use std::io::Read;
use cody_emulator::opcode::disasm;

pub fn main() {
    let mut f = File::open("codybasic.bin").unwrap();
    let mut data = vec![];
    f.read_to_end(&mut data).unwrap();
    let instructions = disasm(&data[..]);
    for insn in instructions {
        println!("{insn}");
    }
}
