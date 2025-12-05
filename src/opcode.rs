use lazy_static::lazy_static;
use std::collections::HashMap;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Opcode {
    ADC,
    AND,
    ASL,
    BBR0,
    BBR1,
    BBR2,
    BBR3,
    BBR4,
    BBR5,
    BBR6,
    BBR7,
    BBS0,
    BBS1,
    BBS2,
    BBS3,
    BBS4,
    BBS5,
    BBS6,
    BBS7,
    BCC,
    BCS,
    BEQ,
    BIT,
    BMI,
    BNE,
    BPL,
    BRA,
    BRK,
    BVC,
    BVS,
    CLC,
    CLD,
    CLI,
    CLV,
    CMP,
    CPX,
    CPY,
    DEC,
    DEX,
    DEY,
    EOR,
    INC,
    INX,
    INY,
    JMP,
    JSR,
    LDA,
    LDX,
    LDY,
    LSR,
    NOP,
    ORA,
    PHA,
    PHP,
    PHX,
    PHY,
    PLA,
    PLP,
    PLX,
    PLY,
    RMB0,
    RMB1,
    RMB2,
    RMB3,
    RMB4,
    RMB5,
    RMB6,
    RMB7,
    ROL,
    ROR,
    RTI,
    RTS,
    SBC,
    SEC,
    SED,
    SEI,
    SMB0,
    SMB1,
    SMB2,
    SMB3,
    SMB4,
    SMB5,
    SMB6,
    SMB7,
    STA,
    STP,
    STX,
    STY,
    STZ,
    TAX,
    TAY,
    TRB,
    TSB,
    TSX,
    TXA,
    TXS,
    TYA,
    WAI,
}

impl Opcode {
    const fn insn0(self, opcode: u8, cycles: u8) -> InstructionMeta {
        InstructionMeta {
            byte: opcode,
            opcode: self,
            parameter_1: AddressingMode::None,
            parameter_2: AddressingMode::None,
            cycles,
        }
    }

    const fn insn1(self, opcode: u8, parameter_1: AddressingMode, cycles: u8) -> InstructionMeta {
        InstructionMeta {
            byte: opcode,
            opcode: self,
            parameter_1,
            parameter_2: AddressingMode::None,
            cycles,
        }
    }

    const fn insn2(
        self,
        opcode: u8,
        parameter_1: AddressingMode,
        parameter_2: AddressingMode,
        cycles: u8,
    ) -> InstructionMeta {
        InstructionMeta {
            byte: opcode,
            opcode: self,
            parameter_1,
            parameter_2,
            cycles,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum AddressingMode {
    /// i (Implied), s (Stack)
    None,
    /// A
    Accumulator,
    /// \#
    Immediate,
    /// a
    Absolute,
    /// a,x
    AbsoluteIndexedX,
    /// a,y
    AbsoluteIndexedY,
    /// (a)
    AbsoluteIndirect,
    /// (a,x)
    AbsoluteIndexedIndirectX,
    /// r
    ProgramCounterRelative,
    /// zp
    ZeroPage,
    /// zp,x
    ZeroPageIndexedX,
    /// zp,y
    ZeroPageIndexedY,
    /// (zp)
    ZeroPageIndirect,
    /// (zp,x)
    ZeroPageIndexedIndirectX,
    /// (zp),y
    ZeroPageIndirectIndexedY,
}

impl AddressingMode {
    pub fn width(&self) -> u16 {
        match self {
            AddressingMode::None | AddressingMode::Accumulator => 0,
            AddressingMode::Immediate
            | AddressingMode::ProgramCounterRelative
            | AddressingMode::ZeroPage
            | AddressingMode::ZeroPageIndexedX
            | AddressingMode::ZeroPageIndexedY
            | AddressingMode::ZeroPageIndirect
            | AddressingMode::ZeroPageIndexedIndirectX
            | AddressingMode::ZeroPageIndirectIndexedY => 1,
            AddressingMode::Absolute
            | AddressingMode::AbsoluteIndexedX
            | AddressingMode::AbsoluteIndexedY
            | AddressingMode::AbsoluteIndirect
            | AddressingMode::AbsoluteIndexedIndirectX => 2,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct InstructionMeta {
    pub byte: u8,
    pub opcode: Opcode,
    pub parameter_1: AddressingMode,
    pub parameter_2: AddressingMode,
    pub cycles: u8,
}

impl InstructionMeta {
    pub fn parameter_width(&self) -> u16 {
        self.parameter_1.width() + self.parameter_2.width()
    }

    pub fn width(&self) -> u16 {
        1 + self.parameter_width()
    }
}

pub fn get_instruction(opcode: u8) -> Option<&'static InstructionMeta> {
    INSTRUCTION_BY_OPCODE_BYTE[opcode as usize]
}

pub fn get_instructions(opcode: Opcode) -> &'static [&'static InstructionMeta] {
    &INSTRUCTIONS_BY_OPCODE[&opcode]
}

/// Unordered list of opcodes, do not use for opcode lookup!
pub static OPCODES: [InstructionMeta; 212] = [
    Opcode::ADC.insn1(0x69, AddressingMode::Immediate, 2),
    Opcode::ADC.insn1(0x6D, AddressingMode::Absolute, 4),
    Opcode::ADC.insn1(0x7D, AddressingMode::AbsoluteIndexedX, 4),
    Opcode::ADC.insn1(0x79, AddressingMode::AbsoluteIndexedY, 4),
    Opcode::ADC.insn1(0x65, AddressingMode::ZeroPage, 3),
    Opcode::ADC.insn1(0x75, AddressingMode::ZeroPageIndexedX, 4),
    Opcode::ADC.insn1(0x61, AddressingMode::ZeroPageIndexedIndirectX, 6),
    Opcode::ADC.insn1(0x72, AddressingMode::ZeroPageIndirect, 5),
    Opcode::ADC.insn1(0x71, AddressingMode::ZeroPageIndirectIndexedY, 5),
    Opcode::AND.insn1(0x29, AddressingMode::Immediate, 2),
    Opcode::AND.insn1(0x2D, AddressingMode::Absolute, 4),
    Opcode::AND.insn1(0x3D, AddressingMode::AbsoluteIndexedX, 4),
    Opcode::AND.insn1(0x39, AddressingMode::AbsoluteIndexedY, 4),
    Opcode::AND.insn1(0x25, AddressingMode::ZeroPage, 3),
    Opcode::AND.insn1(0x35, AddressingMode::ZeroPageIndexedX, 4),
    Opcode::AND.insn1(0x21, AddressingMode::ZeroPageIndexedIndirectX, 6),
    Opcode::AND.insn1(0x32, AddressingMode::ZeroPageIndirect, 5),
    Opcode::AND.insn1(0x31, AddressingMode::ZeroPageIndirectIndexedY, 5),
    Opcode::ASL.insn1(0x0A, AddressingMode::Accumulator, 2),
    Opcode::ASL.insn1(0x0E, AddressingMode::Absolute, 6),
    Opcode::ASL.insn1(0x1E, AddressingMode::AbsoluteIndexedX, 6),
    Opcode::ASL.insn1(0x06, AddressingMode::ZeroPage, 5),
    Opcode::ASL.insn1(0x16, AddressingMode::ZeroPageIndexedX, 6),
    Opcode::BBR0.insn2(
        0x0F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
        5,
    ),
    Opcode::BBR1.insn2(
        0x1F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
        5,
    ),
    Opcode::BBR2.insn2(
        0x2F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
        5,
    ),
    Opcode::BBR3.insn2(
        0x3F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
        5,
    ),
    Opcode::BBR4.insn2(
        0x4F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
        5,
    ),
    Opcode::BBR5.insn2(
        0x5F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
        5,
    ),
    Opcode::BBR6.insn2(
        0x6F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
        5,
    ),
    Opcode::BBR7.insn2(
        0x7F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
        5,
    ),
    Opcode::BBS0.insn2(
        0x8F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
        5,
    ),
    Opcode::BBS1.insn2(
        0x9F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
        5,
    ),
    Opcode::BBS2.insn2(
        0xAF,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
        5,
    ),
    Opcode::BBS3.insn2(
        0xBF,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
        5,
    ),
    Opcode::BBS4.insn2(
        0xCF,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
        5,
    ),
    Opcode::BBS5.insn2(
        0xDF,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
        5,
    ),
    Opcode::BBS6.insn2(
        0xEF,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
        5,
    ),
    Opcode::BBS7.insn2(
        0xFF,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
        5,
    ),
    Opcode::BCC.insn1(0x90, AddressingMode::ProgramCounterRelative, 2),
    Opcode::BCS.insn1(0xB0, AddressingMode::ProgramCounterRelative, 2),
    Opcode::BEQ.insn1(0xF0, AddressingMode::ProgramCounterRelative, 2),
    Opcode::BIT.insn1(0x89, AddressingMode::Immediate, 2),
    Opcode::BIT.insn1(0x2C, AddressingMode::Absolute, 4),
    Opcode::BIT.insn1(0x3C, AddressingMode::AbsoluteIndexedX, 4),
    Opcode::BIT.insn1(0x24, AddressingMode::ZeroPage, 3),
    Opcode::BIT.insn1(0x34, AddressingMode::ZeroPageIndexedX, 4),
    Opcode::BMI.insn1(0x30, AddressingMode::ProgramCounterRelative, 2),
    Opcode::BNE.insn1(0xD0, AddressingMode::ProgramCounterRelative, 2),
    Opcode::BPL.insn1(0x10, AddressingMode::ProgramCounterRelative, 2),
    Opcode::BRA.insn1(0x80, AddressingMode::ProgramCounterRelative, 2),
    Opcode::BRK.insn1(0x00, AddressingMode::Immediate, 7), // was Stack
    Opcode::BVC.insn1(0x50, AddressingMode::ProgramCounterRelative, 2),
    Opcode::BVS.insn1(0x70, AddressingMode::ProgramCounterRelative, 2),
    Opcode::CLC.insn0(0x18, 2), // was Implied
    Opcode::CLD.insn0(0xD8, 2), // was Implied
    Opcode::CLI.insn0(0x58, 2), // was Implied
    Opcode::CLV.insn0(0xB8, 2), // was Implied
    Opcode::CMP.insn1(0xC9, AddressingMode::Immediate, 2),
    Opcode::CMP.insn1(0xCD, AddressingMode::Absolute, 4),
    Opcode::CMP.insn1(0xDD, AddressingMode::AbsoluteIndexedX, 4),
    Opcode::CMP.insn1(0xD9, AddressingMode::AbsoluteIndexedY, 4),
    Opcode::CMP.insn1(0xC5, AddressingMode::ZeroPage, 3),
    Opcode::CMP.insn1(0xD5, AddressingMode::ZeroPageIndexedX, 4),
    Opcode::CMP.insn1(0xC1, AddressingMode::ZeroPageIndexedIndirectX, 6),
    Opcode::CMP.insn1(0xD2, AddressingMode::ZeroPageIndirect, 5),
    Opcode::CMP.insn1(0xD1, AddressingMode::ZeroPageIndirectIndexedY, 5),
    Opcode::CPX.insn1(0xE0, AddressingMode::Immediate, 2),
    Opcode::CPX.insn1(0xEC, AddressingMode::Absolute, 4),
    Opcode::CPX.insn1(0xE4, AddressingMode::ZeroPage, 3),
    Opcode::CPY.insn1(0xC0, AddressingMode::Immediate, 2),
    Opcode::CPY.insn1(0xCC, AddressingMode::Absolute, 4),
    Opcode::CPY.insn1(0xC4, AddressingMode::ZeroPage, 3),
    Opcode::DEC.insn1(0x3A, AddressingMode::Accumulator, 2),
    Opcode::DEC.insn1(0xCE, AddressingMode::Absolute, 6),
    Opcode::DEC.insn1(0xDE, AddressingMode::AbsoluteIndexedX, 7),
    Opcode::DEC.insn1(0xC6, AddressingMode::ZeroPage, 5),
    Opcode::DEC.insn1(0xD6, AddressingMode::ZeroPageIndexedX, 6),
    Opcode::DEX.insn0(0xCA, 2), // was Implied
    Opcode::DEY.insn0(0x88, 2), // was Implied
    Opcode::EOR.insn1(0x49, AddressingMode::Immediate, 2),
    Opcode::EOR.insn1(0x4D, AddressingMode::Absolute, 4),
    Opcode::EOR.insn1(0x5D, AddressingMode::AbsoluteIndexedX, 4),
    Opcode::EOR.insn1(0x59, AddressingMode::AbsoluteIndexedY, 4),
    Opcode::EOR.insn1(0x45, AddressingMode::ZeroPage, 3),
    Opcode::EOR.insn1(0x55, AddressingMode::ZeroPageIndexedX, 4),
    Opcode::EOR.insn1(0x41, AddressingMode::ZeroPageIndexedIndirectX, 6),
    Opcode::EOR.insn1(0x52, AddressingMode::ZeroPageIndirect, 5),
    Opcode::EOR.insn1(0x51, AddressingMode::ZeroPageIndirectIndexedY, 5),
    Opcode::INC.insn1(0x1A, AddressingMode::Accumulator, 2),
    Opcode::INC.insn1(0xEE, AddressingMode::Absolute, 6),
    Opcode::INC.insn1(0xFE, AddressingMode::AbsoluteIndexedX, 7),
    Opcode::INC.insn1(0xE6, AddressingMode::ZeroPage, 5),
    Opcode::INC.insn1(0xF6, AddressingMode::ZeroPageIndexedX, 6),
    Opcode::INX.insn0(0xE8, 2), // was Implied
    Opcode::INY.insn0(0xC8, 2), // was Implied
    Opcode::JMP.insn1(0x4C, AddressingMode::Absolute, 3),
    Opcode::JMP.insn1(0x7C, AddressingMode::AbsoluteIndexedIndirectX, 6),
    Opcode::JMP.insn1(0x6C, AddressingMode::AbsoluteIndirect, 6),
    Opcode::JSR.insn1(0x20, AddressingMode::Absolute, 6),
    Opcode::LDA.insn1(0xA9, AddressingMode::Immediate, 2),
    Opcode::LDA.insn1(0xAD, AddressingMode::Absolute, 4),
    Opcode::LDA.insn1(0xBD, AddressingMode::AbsoluteIndexedX, 4),
    Opcode::LDA.insn1(0xB9, AddressingMode::AbsoluteIndexedY, 4),
    Opcode::LDA.insn1(0xA5, AddressingMode::ZeroPage, 3),
    Opcode::LDA.insn1(0xB5, AddressingMode::ZeroPageIndexedX, 4),
    Opcode::LDA.insn1(0xA1, AddressingMode::ZeroPageIndexedIndirectX, 6),
    Opcode::LDA.insn1(0xB2, AddressingMode::ZeroPageIndirect, 5),
    Opcode::LDA.insn1(0xB1, AddressingMode::ZeroPageIndirectIndexedY, 5),
    Opcode::LDX.insn1(0xA2, AddressingMode::Immediate, 2),
    Opcode::LDX.insn1(0xAE, AddressingMode::Absolute, 4),
    Opcode::LDX.insn1(0xBE, AddressingMode::AbsoluteIndexedY, 4),
    Opcode::LDX.insn1(0xA6, AddressingMode::ZeroPage, 3),
    Opcode::LDX.insn1(0xB6, AddressingMode::ZeroPageIndexedY, 4),
    Opcode::LDY.insn1(0xA0, AddressingMode::Immediate, 2),
    Opcode::LDY.insn1(0xAC, AddressingMode::Absolute, 4),
    Opcode::LDY.insn1(0xBC, AddressingMode::AbsoluteIndexedX, 4),
    Opcode::LDY.insn1(0xA4, AddressingMode::ZeroPage, 3),
    Opcode::LDY.insn1(0xB4, AddressingMode::ZeroPageIndexedX, 4),
    Opcode::LSR.insn1(0x4A, AddressingMode::Accumulator, 2),
    Opcode::LSR.insn1(0x4E, AddressingMode::Absolute, 6),
    Opcode::LSR.insn1(0x5E, AddressingMode::AbsoluteIndexedX, 6),
    Opcode::LSR.insn1(0x46, AddressingMode::ZeroPage, 5),
    Opcode::LSR.insn1(0x56, AddressingMode::ZeroPageIndexedX, 6),
    Opcode::NOP.insn0(0xEA, 2), // was Implied
    Opcode::ORA.insn1(0x09, AddressingMode::Immediate, 2),
    Opcode::ORA.insn1(0x0D, AddressingMode::Absolute, 4),
    Opcode::ORA.insn1(0x1D, AddressingMode::AbsoluteIndexedX, 4),
    Opcode::ORA.insn1(0x19, AddressingMode::AbsoluteIndexedY, 4),
    Opcode::ORA.insn1(0x05, AddressingMode::ZeroPage, 3),
    Opcode::ORA.insn1(0x15, AddressingMode::ZeroPageIndexedX, 4),
    Opcode::ORA.insn1(0x01, AddressingMode::ZeroPageIndexedIndirectX, 6),
    Opcode::ORA.insn1(0x12, AddressingMode::ZeroPageIndirect, 5),
    Opcode::ORA.insn1(0x11, AddressingMode::ZeroPageIndirectIndexedY, 5),
    Opcode::PHA.insn0(0x48, 3), // was Stack
    Opcode::PHP.insn0(0x08, 3), // was Stack
    Opcode::PHX.insn0(0xDA, 3), // was Stack
    Opcode::PHY.insn0(0x5A, 3), // was Stack
    Opcode::PLA.insn0(0x68, 4), // was Stack
    Opcode::PLP.insn0(0x28, 4), // was Stack
    Opcode::PLX.insn0(0xFA, 4), // was Stack
    Opcode::PLY.insn0(0x7A, 4), // was Stack
    Opcode::RMB0.insn1(0x07, AddressingMode::ZeroPage, 5),
    Opcode::RMB1.insn1(0x17, AddressingMode::ZeroPage, 5),
    Opcode::RMB2.insn1(0x27, AddressingMode::ZeroPage, 5),
    Opcode::RMB3.insn1(0x37, AddressingMode::ZeroPage, 5),
    Opcode::RMB4.insn1(0x47, AddressingMode::ZeroPage, 5),
    Opcode::RMB5.insn1(0x57, AddressingMode::ZeroPage, 5),
    Opcode::RMB6.insn1(0x67, AddressingMode::ZeroPage, 5),
    Opcode::RMB7.insn1(0x77, AddressingMode::ZeroPage, 5),
    Opcode::ROL.insn1(0x2A, AddressingMode::Accumulator, 2),
    Opcode::ROL.insn1(0x2E, AddressingMode::Absolute, 6),
    Opcode::ROL.insn1(0x3E, AddressingMode::AbsoluteIndexedX, 6),
    Opcode::ROL.insn1(0x26, AddressingMode::ZeroPage, 5),
    Opcode::ROL.insn1(0x36, AddressingMode::ZeroPageIndexedX, 6),
    Opcode::ROR.insn1(0x6A, AddressingMode::Accumulator, 2),
    Opcode::ROR.insn1(0x6E, AddressingMode::Absolute, 6),
    Opcode::ROR.insn1(0x7E, AddressingMode::AbsoluteIndexedX, 6),
    Opcode::ROR.insn1(0x66, AddressingMode::ZeroPage, 5),
    Opcode::ROR.insn1(0x76, AddressingMode::ZeroPageIndexedX, 6),
    Opcode::RTI.insn0(0x40, 6), // was Stack
    Opcode::RTS.insn0(0x60, 6), // was Stack
    Opcode::SBC.insn1(0xE9, AddressingMode::Immediate, 2),
    Opcode::SBC.insn1(0xED, AddressingMode::Absolute, 4),
    Opcode::SBC.insn1(0xFD, AddressingMode::AbsoluteIndexedX, 4),
    Opcode::SBC.insn1(0xF9, AddressingMode::AbsoluteIndexedY, 4),
    Opcode::SBC.insn1(0xE5, AddressingMode::ZeroPage, 3),
    Opcode::SBC.insn1(0xF5, AddressingMode::ZeroPageIndexedX, 4),
    Opcode::SBC.insn1(0xE1, AddressingMode::ZeroPageIndexedIndirectX, 6),
    Opcode::SBC.insn1(0xF2, AddressingMode::ZeroPageIndirect, 5),
    Opcode::SBC.insn1(0xF1, AddressingMode::ZeroPageIndirectIndexedY, 5),
    Opcode::SEC.insn0(0x38, 2), // was Implied
    Opcode::SED.insn0(0xF8, 2), // was Implied
    Opcode::SEI.insn0(0x78, 2), // was Implied
    Opcode::SMB0.insn1(0x87, AddressingMode::ZeroPage, 5),
    Opcode::SMB1.insn1(0x97, AddressingMode::ZeroPage, 5),
    Opcode::SMB2.insn1(0xA7, AddressingMode::ZeroPage, 5),
    Opcode::SMB3.insn1(0xB7, AddressingMode::ZeroPage, 5),
    Opcode::SMB4.insn1(0xC7, AddressingMode::ZeroPage, 5),
    Opcode::SMB5.insn1(0xD7, AddressingMode::ZeroPage, 5),
    Opcode::SMB6.insn1(0xE7, AddressingMode::ZeroPage, 5),
    Opcode::SMB7.insn1(0xF7, AddressingMode::ZeroPage, 5),
    Opcode::STA.insn1(0x8D, AddressingMode::Absolute, 4),
    Opcode::STA.insn1(0x9D, AddressingMode::AbsoluteIndexedX, 5),
    Opcode::STA.insn1(0x99, AddressingMode::AbsoluteIndexedY, 5),
    Opcode::STA.insn1(0x85, AddressingMode::ZeroPage, 3),
    Opcode::STA.insn1(0x95, AddressingMode::ZeroPageIndexedX, 4),
    Opcode::STA.insn1(0x81, AddressingMode::ZeroPageIndexedIndirectX, 6),
    Opcode::STA.insn1(0x92, AddressingMode::ZeroPageIndirect, 5),
    Opcode::STA.insn1(0x91, AddressingMode::ZeroPageIndirectIndexedY, 6),
    Opcode::STP.insn0(0xDB, 3), // was Implied
    Opcode::STX.insn1(0x8E, AddressingMode::Absolute, 4),
    Opcode::STX.insn1(0x86, AddressingMode::ZeroPage, 3),
    Opcode::STX.insn1(0x96, AddressingMode::ZeroPageIndexedY, 4),
    Opcode::STY.insn1(0x8C, AddressingMode::Absolute, 4),
    Opcode::STY.insn1(0x84, AddressingMode::ZeroPage, 3),
    Opcode::STY.insn1(0x94, AddressingMode::ZeroPageIndexedX, 4),
    Opcode::STZ.insn1(0x9C, AddressingMode::Absolute, 4),
    Opcode::STZ.insn1(0x9E, AddressingMode::AbsoluteIndexedX, 5),
    Opcode::STZ.insn1(0x64, AddressingMode::ZeroPage, 3),
    Opcode::STZ.insn1(0x74, AddressingMode::ZeroPageIndexedX, 4),
    Opcode::TAX.insn0(0xAA, 2), // was Implied
    Opcode::TAY.insn0(0xA8, 2), // was Implied
    Opcode::TRB.insn1(0x1C, AddressingMode::Absolute, 6),
    Opcode::TRB.insn1(0x14, AddressingMode::ZeroPage, 5),
    Opcode::TSB.insn1(0x0C, AddressingMode::Absolute, 6),
    Opcode::TSB.insn1(0x04, AddressingMode::ZeroPage, 5),
    Opcode::TSX.insn0(0xBA, 2), // was Implied
    Opcode::TXA.insn0(0x8A, 2), // was Implied
    Opcode::TXS.insn0(0x9A, 2), // was Implied
    Opcode::TYA.insn0(0x98, 2), // was Implied
    Opcode::WAI.insn0(0xCB, 3), // was Implied
];

lazy_static! {
    /// Lookup table for instructions by opcode byte
    static ref INSTRUCTION_BY_OPCODE_BYTE: [Option<&'static InstructionMeta>; 256] = {
        let mut opcodes: [Option<&'static InstructionMeta>; 256] = [None; 256];
        for opc in &OPCODES {
            let n = opc.byte as usize;
            let p = opcodes.get_mut(n).expect("opcode out of bounds");
            if let Some(current) = p {
                panic!("{n:#X}: opcode already present | current={current:?} new={opc:?}");
            } else {
                *p = Some(opc);
            }
        }
        opcodes
    };

    /// Lookup table for instructions by opcode
    static ref INSTRUCTIONS_BY_OPCODE: HashMap<Opcode, Vec<&'static InstructionMeta>> = {
        let mut map: HashMap<Opcode, Vec<&'static InstructionMeta>> = HashMap::new();
        for opc in &OPCODES {
            map.entry(opc.opcode).or_default().push(opc);
        }
        map
    };
}
