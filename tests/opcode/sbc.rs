use cody_emulator::assembler::{MnemonicDSL, Parameter, assemble};
use cody_emulator::cpu::Cpu;
use cody_emulator::memory::{Memory, Sparse};
use cody_emulator::opcode::Opcode;

fn sbc_check_immediates(a: u8, b: u8, carry: bool) {
    let program = [
        Opcode::SBC.with(Parameter::Immediate(b)),
        Opcode::STP.instruction(),
    ];
    let mut memory = Sparse::default();
    assemble(&program, &mut memory.memory).unwrap();
    memory.write_u16(0xFFFC, 0x0200);
    let mut cpu = Cpu::new(memory);
    cpu.a = a;
    cpu.p.set_carry(carry);
    cpu.run();

    let result_unsigned = a as i16 - b as i16 - !carry as i16;
    let result_signed = (a as i8) as i16 - (b as i8) as i16 - !carry as i16;
    let expected_result = a.wrapping_sub(b).wrapping_sub(!carry as u8);
    let expected_zero = expected_result == 0;
    let expected_negative = (expected_result & 0x80) != 0;
    let expected_carry = (0..=255).contains(&result_unsigned);
    let expected_overflow = !(-128..=127).contains(&result_signed);
    assert_eq!(
        cpu.a, expected_result,
        "result({a}-{b}-~{carry}) | expected={expected_result}, actual={}",
        cpu.a
    );
    assert_eq!(
        cpu.p.zero(),
        expected_zero,
        "zero({a}-{b}-~{carry}={expected_result}) | expected={expected_zero}, actual={}",
        cpu.p.zero()
    );
    assert_eq!(
        cpu.p.negative(),
        expected_negative,
        "negative({a}-{b}-~{carry}={expected_result}) | expected={expected_negative}, actual={}",
        cpu.p.negative()
    );
    assert_eq!(
        cpu.p.carry(),
        expected_carry,
        "carry({a}-{b}-~{carry}={expected_result}) | expected={expected_carry}, actual={}",
        cpu.p.carry()
    );
    assert_eq!(
        cpu.p.overflow(),
        expected_overflow,
        "overflow({a}-{b}-~{carry}={expected_result}) | expected={expected_overflow}, actual={}",
        cpu.p.overflow()
    );
}

#[test]
fn immediates() {
    for a in 0..=255 {
        for b in 0..=255 {
            for c in [false, true] {
                sbc_check_immediates(a, b, c);
            }
        }
    }
}
