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
    const fn insn(self, opcode: u8, parameter_1: AddressingMode) -> InstructionMeta {
        InstructionMeta {
            byte: opcode,
            opcode: self,
            parameter_1,
            parameter_2: None,
        }
    }

    const fn winsn(
        self,
        opcode: u8,
        parameter_1: AddressingMode,
        parameter_2: AddressingMode,
    ) -> InstructionMeta {
        InstructionMeta {
            byte: opcode,
            opcode: self,
            parameter_1,
            parameter_2: Some(parameter_2),
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum AddressingMode {
    /// i
    Implied,
    /// A
    Accumulator,
    /// s
    Stack,
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
            AddressingMode::Implied | AddressingMode::Accumulator | AddressingMode::Stack => 0,
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
    pub parameter_2: Option<AddressingMode>,
}

impl InstructionMeta {
    pub fn parameter_width(&self) -> u16 {
        if self.opcode == Opcode::BRK {
            1
        } else {
            self.parameter_1.width() + self.parameter_2.map(|p| p.width()).unwrap_or(0)
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
    Opcode::ADC.insn(0x69, AddressingMode::Immediate),
    Opcode::ADC.insn(0x6D, AddressingMode::Absolute),
    Opcode::ADC.insn(0x7D, AddressingMode::AbsoluteIndexedX),
    Opcode::ADC.insn(0x79, AddressingMode::AbsoluteIndexedY),
    Opcode::ADC.insn(0x65, AddressingMode::ZeroPage),
    Opcode::ADC.insn(0x75, AddressingMode::ZeroPageIndexedX),
    Opcode::ADC.insn(0x61, AddressingMode::ZeroPageIndexedIndirectX),
    Opcode::ADC.insn(0x72, AddressingMode::ZeroPageIndirect),
    Opcode::ADC.insn(0x71, AddressingMode::ZeroPageIndirectIndexedY),
    Opcode::AND.insn(0x29, AddressingMode::Immediate),
    Opcode::AND.insn(0x2D, AddressingMode::Absolute),
    Opcode::AND.insn(0x3D, AddressingMode::AbsoluteIndexedX),
    Opcode::AND.insn(0x39, AddressingMode::AbsoluteIndexedY),
    Opcode::AND.insn(0x25, AddressingMode::ZeroPage),
    Opcode::AND.insn(0x35, AddressingMode::ZeroPageIndexedX),
    Opcode::AND.insn(0x21, AddressingMode::ZeroPageIndexedIndirectX),
    Opcode::AND.insn(0x32, AddressingMode::ZeroPageIndirect),
    Opcode::AND.insn(0x31, AddressingMode::ZeroPageIndirectIndexedY),
    Opcode::ASL.insn(0x0A, AddressingMode::Accumulator),
    Opcode::ASL.insn(0x0E, AddressingMode::Absolute),
    Opcode::ASL.insn(0x1E, AddressingMode::AbsoluteIndexedX),
    Opcode::ASL.insn(0x06, AddressingMode::ZeroPage),
    Opcode::ASL.insn(0x16, AddressingMode::ZeroPageIndexedX),
    Opcode::BBR0.winsn(
        0x0F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBR1.winsn(
        0x1F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBR2.winsn(
        0x2F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBR3.winsn(
        0x3F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBR4.winsn(
        0x4F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBR5.winsn(
        0x5F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBR6.winsn(
        0x6F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBR7.winsn(
        0x7F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBS0.winsn(
        0x8F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBS1.winsn(
        0x9F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBS2.winsn(
        0xAF,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBS3.winsn(
        0xBF,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBS4.winsn(
        0xCF,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBS5.winsn(
        0xDF,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBS6.winsn(
        0xEF,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BBS7.winsn(
        0xFF,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Opcode::BCC.insn(0x90, AddressingMode::ProgramCounterRelative),
    Opcode::BCS.insn(0xB0, AddressingMode::ProgramCounterRelative),
    Opcode::BEQ.insn(0xF0, AddressingMode::ProgramCounterRelative),
    Opcode::BIT.insn(0x89, AddressingMode::Immediate),
    Opcode::BIT.insn(0x2C, AddressingMode::Absolute),
    Opcode::BIT.insn(0x3C, AddressingMode::AbsoluteIndexedX),
    Opcode::BIT.insn(0x24, AddressingMode::ZeroPage),
    Opcode::BIT.insn(0x34, AddressingMode::ZeroPageIndexedX),
    Opcode::BMI.insn(0x30, AddressingMode::ProgramCounterRelative),
    Opcode::BNE.insn(0xD0, AddressingMode::ProgramCounterRelative),
    Opcode::BPL.insn(0x10, AddressingMode::ProgramCounterRelative),
    Opcode::BRA.insn(0x80, AddressingMode::ProgramCounterRelative),
    Opcode::BRK.insn(0x00, AddressingMode::Stack),
    Opcode::BVC.insn(0x50, AddressingMode::ProgramCounterRelative),
    Opcode::BVS.insn(0x70, AddressingMode::ProgramCounterRelative),
    Opcode::CLC.insn(0x18, AddressingMode::Implied),
    Opcode::CLD.insn(0xD8, AddressingMode::Implied),
    Opcode::CLI.insn(0x58, AddressingMode::Implied),
    Opcode::CLV.insn(0xB8, AddressingMode::Implied),
    Opcode::CMP.insn(0xC9, AddressingMode::Immediate),
    Opcode::CMP.insn(0xCD, AddressingMode::Absolute),
    Opcode::CMP.insn(0xDD, AddressingMode::AbsoluteIndexedX),
    Opcode::CMP.insn(0xD9, AddressingMode::AbsoluteIndexedY),
    Opcode::CMP.insn(0xC5, AddressingMode::ZeroPage),
    Opcode::CMP.insn(0xD5, AddressingMode::ZeroPageIndexedX),
    Opcode::CMP.insn(0xC1, AddressingMode::ZeroPageIndexedIndirectX),
    Opcode::CMP.insn(0xD2, AddressingMode::ZeroPageIndirect),
    Opcode::CMP.insn(0xD1, AddressingMode::ZeroPageIndirectIndexedY),
    Opcode::CPX.insn(0xE0, AddressingMode::Immediate),
    Opcode::CPX.insn(0xEC, AddressingMode::Absolute),
    Opcode::CPX.insn(0xE4, AddressingMode::ZeroPage),
    Opcode::CPY.insn(0xC0, AddressingMode::Immediate),
    Opcode::CPY.insn(0xCC, AddressingMode::Absolute),
    Opcode::CPY.insn(0xC4, AddressingMode::ZeroPage),
    Opcode::DEC.insn(0x3A, AddressingMode::Accumulator),
    Opcode::DEC.insn(0xCE, AddressingMode::Absolute),
    Opcode::DEC.insn(0xDE, AddressingMode::AbsoluteIndexedX),
    Opcode::DEC.insn(0xC6, AddressingMode::ZeroPage),
    Opcode::DEC.insn(0xD6, AddressingMode::ZeroPageIndexedX),
    Opcode::DEX.insn(0xCA, AddressingMode::Implied),
    Opcode::DEY.insn(0x88, AddressingMode::Implied),
    Opcode::EOR.insn(0x49, AddressingMode::Immediate),
    Opcode::EOR.insn(0x4D, AddressingMode::Absolute),
    Opcode::EOR.insn(0x5D, AddressingMode::AbsoluteIndexedX),
    Opcode::EOR.insn(0x59, AddressingMode::AbsoluteIndexedY),
    Opcode::EOR.insn(0x45, AddressingMode::ZeroPage),
    Opcode::EOR.insn(0x55, AddressingMode::ZeroPageIndexedX),
    Opcode::EOR.insn(0x41, AddressingMode::ZeroPageIndexedIndirectX),
    Opcode::EOR.insn(0x52, AddressingMode::ZeroPageIndirect),
    Opcode::EOR.insn(0x51, AddressingMode::ZeroPageIndirectIndexedY),
    Opcode::INC.insn(0x1A, AddressingMode::Accumulator),
    Opcode::INC.insn(0xEE, AddressingMode::Absolute),
    Opcode::INC.insn(0xFE, AddressingMode::AbsoluteIndexedX),
    Opcode::INC.insn(0xE6, AddressingMode::ZeroPage),
    Opcode::INC.insn(0xF6, AddressingMode::ZeroPageIndexedX),
    Opcode::INX.insn(0xE8, AddressingMode::Implied),
    Opcode::INY.insn(0xC8, AddressingMode::Implied),
    Opcode::JMP.insn(0x4C, AddressingMode::Absolute),
    Opcode::JMP.insn(0x7C, AddressingMode::AbsoluteIndexedIndirectX),
    Opcode::JMP.insn(0x6C, AddressingMode::AbsoluteIndirect),
    Opcode::JSR.insn(0x20, AddressingMode::Absolute),
    Opcode::LDA.insn(0xA9, AddressingMode::Immediate),
    Opcode::LDA.insn(0xAD, AddressingMode::Absolute),
    Opcode::LDA.insn(0xBD, AddressingMode::AbsoluteIndexedX),
    Opcode::LDA.insn(0xB9, AddressingMode::AbsoluteIndexedY),
    Opcode::LDA.insn(0xA5, AddressingMode::ZeroPage),
    Opcode::LDA.insn(0xB5, AddressingMode::ZeroPageIndexedX),
    Opcode::LDA.insn(0xA1, AddressingMode::ZeroPageIndexedIndirectX),
    Opcode::LDA.insn(0xB2, AddressingMode::ZeroPageIndirect),
    Opcode::LDA.insn(0xB1, AddressingMode::ZeroPageIndirectIndexedY),
    Opcode::LDX.insn(0xA2, AddressingMode::Immediate),
    Opcode::LDX.insn(0xAE, AddressingMode::Absolute),
    Opcode::LDX.insn(0xBE, AddressingMode::AbsoluteIndexedY),
    Opcode::LDX.insn(0xA6, AddressingMode::ZeroPage),
    Opcode::LDX.insn(0xB6, AddressingMode::ZeroPageIndexedY),
    Opcode::LDY.insn(0xA0, AddressingMode::Immediate),
    Opcode::LDY.insn(0xAC, AddressingMode::Absolute),
    Opcode::LDY.insn(0xBC, AddressingMode::AbsoluteIndexedX),
    Opcode::LDY.insn(0xA4, AddressingMode::ZeroPage),
    Opcode::LDY.insn(0xB4, AddressingMode::ZeroPageIndexedX),
    Opcode::LSR.insn(0x4A, AddressingMode::Accumulator),
    Opcode::LSR.insn(0x4E, AddressingMode::Absolute),
    Opcode::LSR.insn(0x5E, AddressingMode::AbsoluteIndexedX),
    Opcode::LSR.insn(0x46, AddressingMode::ZeroPage),
    Opcode::LSR.insn(0x56, AddressingMode::ZeroPageIndexedX),
    Opcode::NOP.insn(0xEA, AddressingMode::Implied),
    Opcode::ORA.insn(0x09, AddressingMode::Immediate),
    Opcode::ORA.insn(0x0D, AddressingMode::Absolute),
    Opcode::ORA.insn(0x1D, AddressingMode::AbsoluteIndexedX),
    Opcode::ORA.insn(0x19, AddressingMode::AbsoluteIndexedY),
    Opcode::ORA.insn(0x05, AddressingMode::ZeroPage),
    Opcode::ORA.insn(0x15, AddressingMode::ZeroPageIndexedX),
    Opcode::ORA.insn(0x01, AddressingMode::ZeroPageIndexedIndirectX),
    Opcode::ORA.insn(0x12, AddressingMode::ZeroPageIndirect),
    Opcode::ORA.insn(0x11, AddressingMode::ZeroPageIndirectIndexedY),
    Opcode::PHA.insn(0x48, AddressingMode::Stack),
    Opcode::PHP.insn(0x08, AddressingMode::Stack),
    Opcode::PHX.insn(0xDA, AddressingMode::Stack),
    Opcode::PHY.insn(0x5A, AddressingMode::Stack),
    Opcode::PLA.insn(0x68, AddressingMode::Stack),
    Opcode::PLP.insn(0x28, AddressingMode::Stack),
    Opcode::PLX.insn(0xFA, AddressingMode::Stack),
    Opcode::PLY.insn(0x7A, AddressingMode::Stack),
    Opcode::RMB0.insn(0x07, AddressingMode::ZeroPage),
    Opcode::RMB1.insn(0x17, AddressingMode::ZeroPage),
    Opcode::RMB2.insn(0x27, AddressingMode::ZeroPage),
    Opcode::RMB3.insn(0x37, AddressingMode::ZeroPage),
    Opcode::RMB4.insn(0x47, AddressingMode::ZeroPage),
    Opcode::RMB5.insn(0x57, AddressingMode::ZeroPage),
    Opcode::RMB6.insn(0x67, AddressingMode::ZeroPage),
    Opcode::RMB7.insn(0x77, AddressingMode::ZeroPage),
    Opcode::ROL.insn(0x2A, AddressingMode::Accumulator),
    Opcode::ROL.insn(0x2E, AddressingMode::Absolute),
    Opcode::ROL.insn(0x3E, AddressingMode::AbsoluteIndexedX),
    Opcode::ROL.insn(0x26, AddressingMode::ZeroPage),
    Opcode::ROL.insn(0x36, AddressingMode::ZeroPageIndexedX),
    Opcode::ROR.insn(0x6A, AddressingMode::Accumulator),
    Opcode::ROR.insn(0x6E, AddressingMode::Absolute),
    Opcode::ROR.insn(0x7E, AddressingMode::AbsoluteIndexedX),
    Opcode::ROR.insn(0x66, AddressingMode::ZeroPage),
    Opcode::ROR.insn(0x76, AddressingMode::ZeroPageIndexedX),
    Opcode::RTI.insn(0x40, AddressingMode::Stack),
    Opcode::RTS.insn(0x60, AddressingMode::Stack),
    Opcode::SBC.insn(0xE9, AddressingMode::Immediate),
    Opcode::SBC.insn(0xED, AddressingMode::Absolute),
    Opcode::SBC.insn(0xFD, AddressingMode::AbsoluteIndexedX),
    Opcode::SBC.insn(0xF9, AddressingMode::AbsoluteIndexedY),
    Opcode::SBC.insn(0xE5, AddressingMode::ZeroPage),
    Opcode::SBC.insn(0xF5, AddressingMode::ZeroPageIndexedX),
    Opcode::SBC.insn(0xE1, AddressingMode::ZeroPageIndexedIndirectX),
    Opcode::SBC.insn(0xF2, AddressingMode::ZeroPageIndirect),
    Opcode::SBC.insn(0xF1, AddressingMode::ZeroPageIndirectIndexedY),
    Opcode::SEC.insn(0x38, AddressingMode::Implied),
    Opcode::SED.insn(0xF8, AddressingMode::Implied),
    Opcode::SEI.insn(0x78, AddressingMode::Implied),
    Opcode::SMB0.insn(0x87, AddressingMode::ZeroPage),
    Opcode::SMB1.insn(0x97, AddressingMode::ZeroPage),
    Opcode::SMB2.insn(0xA7, AddressingMode::ZeroPage),
    Opcode::SMB3.insn(0xB7, AddressingMode::ZeroPage),
    Opcode::SMB4.insn(0xC7, AddressingMode::ZeroPage),
    Opcode::SMB5.insn(0xD7, AddressingMode::ZeroPage),
    Opcode::SMB6.insn(0xE7, AddressingMode::ZeroPage),
    Opcode::SMB7.insn(0xF7, AddressingMode::ZeroPage),
    Opcode::STA.insn(0x8D, AddressingMode::Absolute),
    Opcode::STA.insn(0x9D, AddressingMode::AbsoluteIndexedX),
    Opcode::STA.insn(0x99, AddressingMode::AbsoluteIndexedY),
    Opcode::STA.insn(0x85, AddressingMode::ZeroPage),
    Opcode::STA.insn(0x95, AddressingMode::ZeroPageIndexedX),
    Opcode::STA.insn(0x81, AddressingMode::ZeroPageIndexedIndirectX),
    Opcode::STA.insn(0x92, AddressingMode::ZeroPageIndirect),
    Opcode::STA.insn(0x91, AddressingMode::ZeroPageIndirectIndexedY),
    Opcode::STP.insn(0xDB, AddressingMode::Implied),
    Opcode::STX.insn(0x8E, AddressingMode::Absolute),
    Opcode::STX.insn(0x86, AddressingMode::ZeroPage),
    Opcode::STX.insn(0x96, AddressingMode::ZeroPageIndexedY),
    Opcode::STY.insn(0x8C, AddressingMode::Absolute),
    Opcode::STY.insn(0x84, AddressingMode::ZeroPage),
    Opcode::STY.insn(0x94, AddressingMode::ZeroPageIndexedX),
    Opcode::STZ.insn(0x9C, AddressingMode::Absolute),
    Opcode::STZ.insn(0x9E, AddressingMode::AbsoluteIndexedX),
    Opcode::STZ.insn(0x64, AddressingMode::ZeroPage),
    Opcode::STZ.insn(0x74, AddressingMode::ZeroPageIndexedX),
    Opcode::TAX.insn(0xAA, AddressingMode::Implied),
    Opcode::TAY.insn(0xA8, AddressingMode::Implied),
    Opcode::TRB.insn(0x1C, AddressingMode::Absolute),
    Opcode::TRB.insn(0x14, AddressingMode::ZeroPage),
    Opcode::TSB.insn(0x0C, AddressingMode::Absolute),
    Opcode::TSB.insn(0x04, AddressingMode::ZeroPage),
    Opcode::TSX.insn(0xBA, AddressingMode::Implied),
    Opcode::TXA.insn(0x8A, AddressingMode::Implied),
    Opcode::TXS.insn(0x9A, AddressingMode::Implied),
    Opcode::TYA.insn(0x98, AddressingMode::Implied),
    Opcode::WAI.insn(0xCB, AddressingMode::Implied),
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
