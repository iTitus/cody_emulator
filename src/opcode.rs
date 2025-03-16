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
    const fn insn0(self, opcode: u8) -> InstructionMeta {
        InstructionMeta {
            byte: opcode,
            opcode: self,
            parameter_1: AddressingMode::None,
            parameter_2: AddressingMode::None,
        }
    }

    const fn insn1(self, opcode: u8, parameter_1: AddressingMode) -> InstructionMeta {
        InstructionMeta {
            byte: opcode,
            opcode: self,
            parameter_1,
            parameter_2: AddressingMode::None,
        }
    }

    const fn insn2(
        self,
        opcode: u8,
        parameter_1: AddressingMode,
        parameter_2: AddressingMode,
    ) -> InstructionMeta {
        InstructionMeta {
            byte: opcode,
            opcode: self,
            parameter_1,
            parameter_2,
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
}

impl InstructionMeta {
    pub fn parameter_width(&self) -> u16 {
        if self.opcode == Opcode::BRK {
            1
        } else {
            self.parameter_1.width() + self.parameter_2.width()
        }
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
    Opcode::ADC.insn1(0x69, AddressingMode::Immediate),
    Opcode::ADC.insn1(0x6D, AddressingMode::Absolute),
    Opcode::ADC.insn1(0x7D, AddressingMode::AbsoluteIndexedX),
    Opcode::ADC.insn1(0x79, AddressingMode::AbsoluteIndexedY),
    Opcode::ADC.insn1(0x65, AddressingMode::ZeroPage),
    Opcode::ADC.insn1(0x75, AddressingMode::ZeroPageIndexedX),
    Opcode::ADC.insn1(0x61, AddressingMode::ZeroPageIndexedIndirectX),
    Opcode::ADC.insn1(0x72, AddressingMode::ZeroPageIndirect),
    Opcode::ADC.insn1(0x71, AddressingMode::ZeroPageIndirectIndexedY),
    Opcode::AND.insn1(0x29, AddressingMode::Immediate),
    Opcode::AND.insn1(0x2D, AddressingMode::Absolute),
    Opcode::AND.insn1(0x3D, AddressingMode::AbsoluteIndexedX),
    Opcode::AND.insn1(0x39, AddressingMode::AbsoluteIndexedY),
    Opcode::AND.insn1(0x25, AddressingMode::ZeroPage),
    Opcode::AND.insn1(0x35, AddressingMode::ZeroPageIndexedX),
    Opcode::AND.insn1(0x21, AddressingMode::ZeroPageIndexedIndirectX),
    Opcode::AND.insn1(0x32, AddressingMode::ZeroPageIndirect),
    Opcode::AND.insn1(0x31, AddressingMode::ZeroPageIndirectIndexedY),
    Opcode::ASL.insn1(0x0A, AddressingMode::Accumulator),
    Opcode::ASL.insn1(0x0E, AddressingMode::Absolute),
    Opcode::ASL.insn1(0x1E, AddressingMode::AbsoluteIndexedX),
    Opcode::ASL.insn1(0x06, AddressingMode::ZeroPage),
    Opcode::ASL.insn1(0x16, AddressingMode::ZeroPageIndexedX),
    Opcode::BBR0.insn2(
        0x0F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBR1.insn2(
        0x1F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBR2.insn2(
        0x2F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBR3.insn2(
        0x3F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBR4.insn2(
        0x4F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBR5.insn2(
        0x5F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBR6.insn2(
        0x6F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBR7.insn2(
        0x7F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBS0.insn2(
        0x8F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBS1.insn2(
        0x9F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBS2.insn2(
        0xAF,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBS3.insn2(
        0xBF,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBS4.insn2(
        0xCF,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBS5.insn2(
        0xDF,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBS6.insn2(
        0xEF,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBS7.insn2(
        0xFF,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BCC.insn1(0x90, AddressingMode::ProgramCounterRelative),
    Opcode::BCS.insn1(0xB0, AddressingMode::ProgramCounterRelative),
    Opcode::BEQ.insn1(0xF0, AddressingMode::ProgramCounterRelative),
    Opcode::BIT.insn1(0x89, AddressingMode::Immediate),
    Opcode::BIT.insn1(0x2C, AddressingMode::Absolute),
    Opcode::BIT.insn1(0x3C, AddressingMode::AbsoluteIndexedX),
    Opcode::BIT.insn1(0x24, AddressingMode::ZeroPage),
    Opcode::BIT.insn1(0x34, AddressingMode::ZeroPageIndexedX),
    Opcode::BMI.insn1(0x30, AddressingMode::ProgramCounterRelative),
    Opcode::BNE.insn1(0xD0, AddressingMode::ProgramCounterRelative),
    Opcode::BPL.insn1(0x10, AddressingMode::ProgramCounterRelative),
    Opcode::BRA.insn1(0x80, AddressingMode::ProgramCounterRelative),
    Opcode::BRK.insn0(0x00), // was Stack
    Opcode::BVC.insn1(0x50, AddressingMode::ProgramCounterRelative),
    Opcode::BVS.insn1(0x70, AddressingMode::ProgramCounterRelative),
    Opcode::CLC.insn0(0x18), // was Implied
    Opcode::CLD.insn0(0xD8), // was Implied
    Opcode::CLI.insn0(0x58), // was Implied
    Opcode::CLV.insn0(0xB8), // was Implied
    Opcode::CMP.insn1(0xC9, AddressingMode::Immediate),
    Opcode::CMP.insn1(0xCD, AddressingMode::Absolute),
    Opcode::CMP.insn1(0xDD, AddressingMode::AbsoluteIndexedX),
    Opcode::CMP.insn1(0xD9, AddressingMode::AbsoluteIndexedY),
    Opcode::CMP.insn1(0xC5, AddressingMode::ZeroPage),
    Opcode::CMP.insn1(0xD5, AddressingMode::ZeroPageIndexedX),
    Opcode::CMP.insn1(0xC1, AddressingMode::ZeroPageIndexedIndirectX),
    Opcode::CMP.insn1(0xD2, AddressingMode::ZeroPageIndirect),
    Opcode::CMP.insn1(0xD1, AddressingMode::ZeroPageIndirectIndexedY),
    Opcode::CPX.insn1(0xE0, AddressingMode::Immediate),
    Opcode::CPX.insn1(0xEC, AddressingMode::Absolute),
    Opcode::CPX.insn1(0xE4, AddressingMode::ZeroPage),
    Opcode::CPY.insn1(0xC0, AddressingMode::Immediate),
    Opcode::CPY.insn1(0xCC, AddressingMode::Absolute),
    Opcode::CPY.insn1(0xC4, AddressingMode::ZeroPage),
    Opcode::DEC.insn1(0x3A, AddressingMode::Accumulator),
    Opcode::DEC.insn1(0xCE, AddressingMode::Absolute),
    Opcode::DEC.insn1(0xDE, AddressingMode::AbsoluteIndexedX),
    Opcode::DEC.insn1(0xC6, AddressingMode::ZeroPage),
    Opcode::DEC.insn1(0xD6, AddressingMode::ZeroPageIndexedX),
    Opcode::DEX.insn0(0xCA), // was Implied
    Opcode::DEY.insn0(0x88), // was Implied
    Opcode::EOR.insn1(0x49, AddressingMode::Immediate),
    Opcode::EOR.insn1(0x4D, AddressingMode::Absolute),
    Opcode::EOR.insn1(0x5D, AddressingMode::AbsoluteIndexedX),
    Opcode::EOR.insn1(0x59, AddressingMode::AbsoluteIndexedY),
    Opcode::EOR.insn1(0x45, AddressingMode::ZeroPage),
    Opcode::EOR.insn1(0x55, AddressingMode::ZeroPageIndexedX),
    Opcode::EOR.insn1(0x41, AddressingMode::ZeroPageIndexedIndirectX),
    Opcode::EOR.insn1(0x52, AddressingMode::ZeroPageIndirect),
    Opcode::EOR.insn1(0x51, AddressingMode::ZeroPageIndirectIndexedY),
    Opcode::INC.insn1(0x1A, AddressingMode::Accumulator),
    Opcode::INC.insn1(0xEE, AddressingMode::Absolute),
    Opcode::INC.insn1(0xFE, AddressingMode::AbsoluteIndexedX),
    Opcode::INC.insn1(0xE6, AddressingMode::ZeroPage),
    Opcode::INC.insn1(0xF6, AddressingMode::ZeroPageIndexedX),
    Opcode::INX.insn0(0xE8), // was Implied
    Opcode::INY.insn0(0xC8), // was Implied
    Opcode::JMP.insn1(0x4C, AddressingMode::Absolute),
    Opcode::JMP.insn1(0x7C, AddressingMode::AbsoluteIndexedIndirectX),
    Opcode::JMP.insn1(0x6C, AddressingMode::AbsoluteIndirect),
    Opcode::JSR.insn1(0x20, AddressingMode::Absolute),
    Opcode::LDA.insn1(0xA9, AddressingMode::Immediate),
    Opcode::LDA.insn1(0xAD, AddressingMode::Absolute),
    Opcode::LDA.insn1(0xBD, AddressingMode::AbsoluteIndexedX),
    Opcode::LDA.insn1(0xB9, AddressingMode::AbsoluteIndexedY),
    Opcode::LDA.insn1(0xA5, AddressingMode::ZeroPage),
    Opcode::LDA.insn1(0xB5, AddressingMode::ZeroPageIndexedX),
    Opcode::LDA.insn1(0xA1, AddressingMode::ZeroPageIndexedIndirectX),
    Opcode::LDA.insn1(0xB2, AddressingMode::ZeroPageIndirect),
    Opcode::LDA.insn1(0xB1, AddressingMode::ZeroPageIndirectIndexedY),
    Opcode::LDX.insn1(0xA2, AddressingMode::Immediate),
    Opcode::LDX.insn1(0xAE, AddressingMode::Absolute),
    Opcode::LDX.insn1(0xBE, AddressingMode::AbsoluteIndexedY),
    Opcode::LDX.insn1(0xA6, AddressingMode::ZeroPage),
    Opcode::LDX.insn1(0xB6, AddressingMode::ZeroPageIndexedY),
    Opcode::LDY.insn1(0xA0, AddressingMode::Immediate),
    Opcode::LDY.insn1(0xAC, AddressingMode::Absolute),
    Opcode::LDY.insn1(0xBC, AddressingMode::AbsoluteIndexedX),
    Opcode::LDY.insn1(0xA4, AddressingMode::ZeroPage),
    Opcode::LDY.insn1(0xB4, AddressingMode::ZeroPageIndexedX),
    Opcode::LSR.insn1(0x4A, AddressingMode::Accumulator),
    Opcode::LSR.insn1(0x4E, AddressingMode::Absolute),
    Opcode::LSR.insn1(0x5E, AddressingMode::AbsoluteIndexedX),
    Opcode::LSR.insn1(0x46, AddressingMode::ZeroPage),
    Opcode::LSR.insn1(0x56, AddressingMode::ZeroPageIndexedX),
    Opcode::NOP.insn0(0xEA), // was Implied
    Opcode::ORA.insn1(0x09, AddressingMode::Immediate),
    Opcode::ORA.insn1(0x0D, AddressingMode::Absolute),
    Opcode::ORA.insn1(0x1D, AddressingMode::AbsoluteIndexedX),
    Opcode::ORA.insn1(0x19, AddressingMode::AbsoluteIndexedY),
    Opcode::ORA.insn1(0x05, AddressingMode::ZeroPage),
    Opcode::ORA.insn1(0x15, AddressingMode::ZeroPageIndexedX),
    Opcode::ORA.insn1(0x01, AddressingMode::ZeroPageIndexedIndirectX),
    Opcode::ORA.insn1(0x12, AddressingMode::ZeroPageIndirect),
    Opcode::ORA.insn1(0x11, AddressingMode::ZeroPageIndirectIndexedY),
    Opcode::PHA.insn0(0x48), // was Stack
    Opcode::PHP.insn0(0x08), // was Stack
    Opcode::PHX.insn0(0xDA), // was Stack
    Opcode::PHY.insn0(0x5A), // was Stack
    Opcode::PLA.insn0(0x68), // was Stack
    Opcode::PLP.insn0(0x28), // was Stack
    Opcode::PLX.insn0(0xFA), // was Stack
    Opcode::PLY.insn0(0x7A), // was Stack
    Opcode::RMB0.insn1(0x07, AddressingMode::ZeroPage),
    Opcode::RMB1.insn1(0x17, AddressingMode::ZeroPage),
    Opcode::RMB2.insn1(0x27, AddressingMode::ZeroPage),
    Opcode::RMB3.insn1(0x37, AddressingMode::ZeroPage),
    Opcode::RMB4.insn1(0x47, AddressingMode::ZeroPage),
    Opcode::RMB5.insn1(0x57, AddressingMode::ZeroPage),
    Opcode::RMB6.insn1(0x67, AddressingMode::ZeroPage),
    Opcode::RMB7.insn1(0x77, AddressingMode::ZeroPage),
    Opcode::ROL.insn1(0x2A, AddressingMode::Accumulator),
    Opcode::ROL.insn1(0x2E, AddressingMode::Absolute),
    Opcode::ROL.insn1(0x3E, AddressingMode::AbsoluteIndexedX),
    Opcode::ROL.insn1(0x26, AddressingMode::ZeroPage),
    Opcode::ROL.insn1(0x36, AddressingMode::ZeroPageIndexedX),
    Opcode::ROR.insn1(0x6A, AddressingMode::Accumulator),
    Opcode::ROR.insn1(0x6E, AddressingMode::Absolute),
    Opcode::ROR.insn1(0x7E, AddressingMode::AbsoluteIndexedX),
    Opcode::ROR.insn1(0x66, AddressingMode::ZeroPage),
    Opcode::ROR.insn1(0x76, AddressingMode::ZeroPageIndexedX),
    Opcode::RTI.insn0(0x40), // was Stack
    Opcode::RTS.insn0(0x60), // was Stack
    Opcode::SBC.insn1(0xE9, AddressingMode::Immediate),
    Opcode::SBC.insn1(0xED, AddressingMode::Absolute),
    Opcode::SBC.insn1(0xFD, AddressingMode::AbsoluteIndexedX),
    Opcode::SBC.insn1(0xF9, AddressingMode::AbsoluteIndexedY),
    Opcode::SBC.insn1(0xE5, AddressingMode::ZeroPage),
    Opcode::SBC.insn1(0xF5, AddressingMode::ZeroPageIndexedX),
    Opcode::SBC.insn1(0xE1, AddressingMode::ZeroPageIndexedIndirectX),
    Opcode::SBC.insn1(0xF2, AddressingMode::ZeroPageIndirect),
    Opcode::SBC.insn1(0xF1, AddressingMode::ZeroPageIndirectIndexedY),
    Opcode::SEC.insn0(0x38), // was Implied
    Opcode::SED.insn0(0xF8), // was Implied
    Opcode::SEI.insn0(0x78), // was Implied
    Opcode::SMB0.insn1(0x87, AddressingMode::ZeroPage),
    Opcode::SMB1.insn1(0x97, AddressingMode::ZeroPage),
    Opcode::SMB2.insn1(0xA7, AddressingMode::ZeroPage),
    Opcode::SMB3.insn1(0xB7, AddressingMode::ZeroPage),
    Opcode::SMB4.insn1(0xC7, AddressingMode::ZeroPage),
    Opcode::SMB5.insn1(0xD7, AddressingMode::ZeroPage),
    Opcode::SMB6.insn1(0xE7, AddressingMode::ZeroPage),
    Opcode::SMB7.insn1(0xF7, AddressingMode::ZeroPage),
    Opcode::STA.insn1(0x8D, AddressingMode::Absolute),
    Opcode::STA.insn1(0x9D, AddressingMode::AbsoluteIndexedX),
    Opcode::STA.insn1(0x99, AddressingMode::AbsoluteIndexedY),
    Opcode::STA.insn1(0x85, AddressingMode::ZeroPage),
    Opcode::STA.insn1(0x95, AddressingMode::ZeroPageIndexedX),
    Opcode::STA.insn1(0x81, AddressingMode::ZeroPageIndexedIndirectX),
    Opcode::STA.insn1(0x92, AddressingMode::ZeroPageIndirect),
    Opcode::STA.insn1(0x91, AddressingMode::ZeroPageIndirectIndexedY),
    Opcode::STP.insn0(0xDB), // was Implied
    Opcode::STX.insn1(0x8E, AddressingMode::Absolute),
    Opcode::STX.insn1(0x86, AddressingMode::ZeroPage),
    Opcode::STX.insn1(0x96, AddressingMode::ZeroPageIndexedY),
    Opcode::STY.insn1(0x8C, AddressingMode::Absolute),
    Opcode::STY.insn1(0x84, AddressingMode::ZeroPage),
    Opcode::STY.insn1(0x94, AddressingMode::ZeroPageIndexedX),
    Opcode::STZ.insn1(0x9C, AddressingMode::Absolute),
    Opcode::STZ.insn1(0x9E, AddressingMode::AbsoluteIndexedX),
    Opcode::STZ.insn1(0x64, AddressingMode::ZeroPage),
    Opcode::STZ.insn1(0x74, AddressingMode::ZeroPageIndexedX),
    Opcode::TAX.insn0(0xAA), // was Implied
    Opcode::TAY.insn0(0xA8), // was Implied
    Opcode::TRB.insn1(0x1C, AddressingMode::Absolute),
    Opcode::TRB.insn1(0x14, AddressingMode::ZeroPage),
    Opcode::TSB.insn1(0x0C, AddressingMode::Absolute),
    Opcode::TSB.insn1(0x04, AddressingMode::ZeroPage),
    Opcode::TSX.insn0(0xBA), // was Implied
    Opcode::TXA.insn0(0x8A), // was Implied
    Opcode::TXS.insn0(0x9A), // was Implied
    Opcode::TYA.insn0(0x98), // was Implied
    Opcode::WAI.insn0(0xCB), // was Implied
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
