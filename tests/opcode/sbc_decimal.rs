use cody_emulator::assembler::{MnemonicDSL, Parameter, assemble};
use cody_emulator::cpu;
use cody_emulator::cpu::Cpu;
use cody_emulator::memory::Memory;
use cody_emulator::memory::contiguous::Contiguous;
use cody_emulator::opcode::Opcode;

fn sbc_check_immediates(a: u8, b: u8, carry: bool) -> Cpu<Contiguous> {
    let program = [
        Opcode::SBC.with(Parameter::Immediate(b)),
        Opcode::STP.instruction(),
    ];
    let mut memory = Contiguous::new_ram(0x10000);
    assemble(&program, &mut *memory.memory).unwrap();
    memory.write_u16(cpu::RESET_VECTOR, 0x0200);
    let mut cpu = Cpu::new(memory);
    cpu.a = a;
    cpu.p.set_carry(carry);
    cpu.p.set_decimal_mode(true);
    cpu.run();

    cpu
}

#[test]
fn sbc_bcd_0_0() {
    let cpu = sbc_check_immediates(0, 0, true);
    assert_eq!(cpu.a, 0);
}
