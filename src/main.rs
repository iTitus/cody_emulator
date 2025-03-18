use cody_emulator::assembler::disassemble;
use cody_emulator::vid;

pub fn main() {
    vid::start();
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
