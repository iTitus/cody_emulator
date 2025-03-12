#![allow(dead_code)]

use cody_emulator::cpu::Cpu;
use std::fs::File;
use std::io::Read;
use cody_emulator::assembler::disassemble;
use cody_emulator::memory::Contiguous;

pub fn main() {
    let mut f = File::open("codybasic.bin").unwrap();
    let mut data = vec![];
    f.read_to_end(&mut data).unwrap();
    dis(&data);
    // run(&data, 0xE000);
}

fn dis(data: &[u8]) {
    let instructions = disassemble(data);
    for insn in instructions {
        println!("{insn}");
    }
}

fn run(data: &[u8], load_address: u16) {
    let mut cpu = Cpu::new(Contiguous::from_bytes_at(data, load_address));
    cpu.run();
}
