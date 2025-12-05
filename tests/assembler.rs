use cody_emulator::assembler::{MnemonicDSL, Parameter, assemble};
use cody_emulator::cpu;
use cody_emulator::cpu::Cpu;
use cody_emulator::memory::Memory;
use cody_emulator::memory::contiguous::Contiguous;
use cody_emulator::opcode::Opcode;

#[test]
pub fn test_assemble_labels_1() {
    let program = [
        Opcode::BRA.with(Parameter::label("set_1")),
        Opcode::LDA.labelled_with("set_1", Parameter::Immediate(1)),
        Opcode::BRA.with(Parameter::label("exit")),
        Opcode::LDA.labelled_with("set_2", Parameter::Immediate(2)),
        Opcode::BRA.with(Parameter::label("exit")),
        Opcode::STP.labelled("exit"),
    ];
    let mut memory = Contiguous::new_ram(0x10000);
    assemble(&program, &mut *memory.memory).unwrap();
    memory.write_u16(cpu::RESET_VECTOR, 0x0200);
    let mut cpu = Cpu::new(memory);
    cpu.run();

    assert_eq!(cpu.a, 1);
}

#[test]
pub fn test_assemble_labels_2() {
    let program = [
        Opcode::BRA.with(Parameter::label("set_2")),
        Opcode::LDA.labelled_with("set_1", Parameter::Immediate(1)),
        Opcode::BRA.with(Parameter::label("exit")),
        Opcode::LDA.labelled_with("set_2", Parameter::Immediate(2)),
        Opcode::BRA.with(Parameter::label("exit")),
        Opcode::STP.labelled("exit"),
    ];
    let mut memory = Contiguous::new_ram(0x10000);
    assemble(&program, &mut *memory.memory).unwrap();
    memory.write_u16(cpu::RESET_VECTOR, 0x0200);
    let mut cpu = Cpu::new(memory);
    cpu.run();

    assert_eq!(cpu.a, 2);
}

#[test]
pub fn test_assemble_bbs_labels() {
    let program = [
        Opcode::LDA.with(Parameter::Immediate(1)),
        Opcode::STA.with(Parameter::Absolute(0)),
        Opcode::BBS0.with(Parameter::list([
            Parameter::Absolute(0),
            Parameter::label("set_2"),
        ])),
        Opcode::LDA.labelled_with("set_1", Parameter::Immediate(1)),
        Opcode::BRA.with(Parameter::label("exit")),
        Opcode::LDA.labelled_with("set_2", Parameter::Immediate(2)),
        Opcode::BRA.with(Parameter::label("exit")),
        Opcode::STP.labelled("exit"),
    ];
    let mut memory = Contiguous::new_ram(0x10000);
    assemble(&program, &mut *memory.memory).unwrap();
    memory.write_u16(cpu::RESET_VECTOR, 0x0200);
    let mut cpu = Cpu::new(memory);
    cpu.run();

    assert_eq!(cpu.a, 2);
}
