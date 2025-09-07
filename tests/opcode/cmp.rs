use cody_emulator::assembler::{MnemonicDSL, Parameter, assemble};
use cody_emulator::cpu;
use cody_emulator::cpu::Cpu;
use cody_emulator::interrupt::NoopInterruptProvider;
use cody_emulator::memory::{Memory, Sparse};
use cody_emulator::opcode::Opcode;

fn cmp_check_immediates(a: u8, b: u8) {
    let program = [
        Opcode::CMP.with(Parameter::Immediate(b)),
        Opcode::STP.instruction(),
    ];
    let mut memory = Sparse::default();
    assemble(&program, &mut memory.memory).unwrap();
    memory.write_u16(cpu::RESET_VECTOR, 0x0200);
    let mut cpu = Cpu::new(memory, NoopInterruptProvider);
    cpu.a = a;
    let prev_p = cpu.p;
    cpu.run();

    // keep other flags
    assert_eq!(prev_p.overflow(), cpu.p.overflow());
    assert_eq!(prev_p.brk_command(), cpu.p.brk_command());
    assert_eq!(prev_p.decimal_mode(), cpu.p.decimal_mode());
    assert_eq!(prev_p.irqb_disable(), cpu.p.irqb_disable());

    // check NZC flags
    let z = a == b;
    let c = a >= b;
    let n = (a as i8) < (b as i8);
    assert_eq!(
        z,
        cpu.p.zero(),
        "Z: {a}<>{b} | expected={n}, actual={}",
        cpu.p.zero()
    );
    assert_eq!(
        c,
        cpu.p.carry(),
        "C: {a}<>{b} | expected={n}, actual={}",
        cpu.p.carry()
    );
    assert_eq!(
        n,
        cpu.p.negative(),
        "N: {a}<>{b} | expected={n}, actual={}",
        cpu.p.negative()
    );
}

#[test]
fn immediates() {
    for a in 0..=255 {
        for b in 0..=255 {
            cmp_check_immediates(a, b);
        }
    }
}
