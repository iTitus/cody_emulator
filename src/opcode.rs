use lazy_static::lazy_static;
use std::fmt::{Display, Formatter};
use std::io::{Read, Write};
use strum::IntoStaticStr;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, IntoStaticStr)]
pub enum Mnemonic {
    Invalid,
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

impl Mnemonic {
    const fn opc(self, opcode: u8, parameter_1: AddressingMode) -> Opcode {
        Opcode {
            opcode,
            mnemonic: self,
            parameter_1,
            parameter_2: None,
        }
    }

    const fn wopc(
        self,
        opcode: u8,
        parameter_1: AddressingMode,
        parameter_2: AddressingMode,
    ) -> Opcode {
        Opcode {
            opcode,
            mnemonic: self,
            parameter_1,
            parameter_2: Some(parameter_2),
        }
    }

    pub fn iinsn(self) -> Instruction {
        Instruction {
            mnemonic: self,
            parameter_1: InstructionParameter::None,
            parameter_2: InstructionParameter::None,
        }
    }

    pub fn insn(self, parameter: InstructionParameter) -> Instruction {
        Instruction {
            mnemonic: self,
            parameter_1: parameter,
            parameter_2: InstructionParameter::None,
        }
    }

    pub fn winsn(
        self,
        parameter1: InstructionParameter,
        parameter2: InstructionParameter,
    ) -> Instruction {
        Instruction {
            mnemonic: self,
            parameter_1: parameter1,
            parameter_2: parameter2,
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
    pub fn read(&self, mut r: impl Read) -> std::io::Result<InstructionParameter> {
        Ok(match self {
            AddressingMode::Implied | AddressingMode::Accumulator | AddressingMode::Stack => {
                InstructionParameter::None
            }
            AddressingMode::Immediate => {
                let mut buf = [0];
                r.read_exact(&mut buf)?;
                InstructionParameter::Immediate(buf[0])
            }
            AddressingMode::Absolute => {
                let mut buf = [0, 0];
                r.read_exact(&mut buf)?;
                InstructionParameter::Absolute(u16::from_le_bytes(buf))
            }
            AddressingMode::AbsoluteIndexedX => {
                let mut buf = [0, 0];
                r.read_exact(&mut buf)?;
                InstructionParameter::AbsoluteIndexedX(u16::from_le_bytes(buf))
            }
            AddressingMode::AbsoluteIndexedY => {
                let mut buf = [0, 0];
                r.read_exact(&mut buf)?;
                InstructionParameter::AbsoluteIndexedY(u16::from_le_bytes(buf))
            }
            AddressingMode::AbsoluteIndirect => {
                let mut buf = [0, 0];
                r.read_exact(&mut buf)?;
                InstructionParameter::AbsoluteIndirect(u16::from_le_bytes(buf))
            }
            AddressingMode::AbsoluteIndexedIndirectX => {
                let mut buf = [0, 0];
                r.read_exact(&mut buf)?;
                InstructionParameter::AbsoluteIndexedIndirectX(u16::from_le_bytes(buf))
            }
            AddressingMode::ProgramCounterRelative => {
                let mut buf = [0];
                r.read_exact(&mut buf)?;
                InstructionParameter::ProgramCounterRelative(buf[0] as i8)
            }
            AddressingMode::ZeroPage => {
                let mut buf = [0];
                r.read_exact(&mut buf)?;
                InstructionParameter::ZeroPage(buf[0])
            }
            AddressingMode::ZeroPageIndexedX => {
                let mut buf = [0];
                r.read_exact(&mut buf)?;
                InstructionParameter::ZeroPageIndexedX(buf[0])
            }
            AddressingMode::ZeroPageIndexedY => {
                let mut buf = [0];
                r.read_exact(&mut buf)?;
                InstructionParameter::ZeroPageIndexedY(buf[0])
            }
            AddressingMode::ZeroPageIndirect => {
                let mut buf = [0];
                r.read_exact(&mut buf)?;
                InstructionParameter::ZeroPageIndirect(buf[0])
            }
            AddressingMode::ZeroPageIndexedIndirectX => {
                let mut buf = [0];
                r.read_exact(&mut buf)?;
                InstructionParameter::ZeroPageIndexedIndirectX(buf[0])
            }
            AddressingMode::ZeroPageIndirectIndexedY => {
                let mut buf = [0];
                r.read_exact(&mut buf)?;
                InstructionParameter::ZeroPageIndirectIndexedY(buf[0])
            }
        })
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum InstructionParameter {
    /// i, A, s
    None,
    /// \#
    Immediate(u8),
    /// a
    Absolute(u16),
    /// a,x
    AbsoluteIndexedX(u16),
    /// a,y
    AbsoluteIndexedY(u16),
    /// (a)
    AbsoluteIndirect(u16),
    /// (a,x)
    AbsoluteIndexedIndirectX(u16),
    /// r
    ProgramCounterRelative(i8),
    /// zp
    ZeroPage(u8),
    /// zp,x
    ZeroPageIndexedX(u8),
    /// zp,y
    ZeroPageIndexedY(u8),
    /// (zp)
    ZeroPageIndirect(u8),
    /// (zp,x)
    ZeroPageIndexedIndirectX(u8),
    /// (zp),y
    ZeroPageIndirectIndexedY(u8),
}

impl Display for InstructionParameter {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            InstructionParameter::None => write!(f, "?"),
            InstructionParameter::Immediate(n) => write!(f, "#${n:02X}"),
            InstructionParameter::Absolute(n) => write!(f, "${n:04X}"),
            InstructionParameter::AbsoluteIndexedX(n) => write!(f, "${n:04X},X"),
            InstructionParameter::AbsoluteIndexedY(n) => write!(f, "${n:04X},Y"),
            InstructionParameter::AbsoluteIndirect(n) => write!(f, "(${n:04X})"),
            InstructionParameter::AbsoluteIndexedIndirectX(n) => write!(f, "(${n:04X},X)"),
            InstructionParameter::ProgramCounterRelative(n) => write!(f, "pc{n:+}"),
            InstructionParameter::ZeroPage(n) => write!(f, "${n:02X}"),
            InstructionParameter::ZeroPageIndexedX(n) => write!(f, "${n:02X},X"),
            InstructionParameter::ZeroPageIndexedY(n) => write!(f, "${n:02X},Y"),
            InstructionParameter::ZeroPageIndirect(n) => write!(f, "(${n:02X})"),
            InstructionParameter::ZeroPageIndexedIndirectX(n) => write!(f, "(${n:02X},X)"),
            InstructionParameter::ZeroPageIndirectIndexedY(n) => write!(f, "(${n:02X}),X"),
        }
    }
}

impl InstructionParameter {
    fn matches(&self, addressing_mode: AddressingMode) -> bool {
        matches!(
            (self, addressing_mode),
            (
                InstructionParameter::None,
                AddressingMode::Implied | AddressingMode::Accumulator | AddressingMode::Stack,
            ) | (
                InstructionParameter::Immediate(_),
                AddressingMode::Immediate
            ) | (InstructionParameter::Absolute(_), AddressingMode::Absolute)
                | (
                    InstructionParameter::AbsoluteIndexedX(_),
                    AddressingMode::AbsoluteIndexedX
                )
                | (
                    InstructionParameter::AbsoluteIndexedY(_),
                    AddressingMode::AbsoluteIndexedY
                )
                | (
                    InstructionParameter::AbsoluteIndirect(_),
                    AddressingMode::AbsoluteIndirect
                )
                | (
                    InstructionParameter::AbsoluteIndexedIndirectX(_),
                    AddressingMode::AbsoluteIndexedIndirectX,
                )
                | (
                    InstructionParameter::ProgramCounterRelative(_),
                    AddressingMode::ProgramCounterRelative,
                )
                | (InstructionParameter::ZeroPage(_), AddressingMode::ZeroPage)
                | (
                    InstructionParameter::ZeroPageIndexedX(_),
                    AddressingMode::ZeroPageIndexedX
                )
                | (
                    InstructionParameter::ZeroPageIndexedY(_),
                    AddressingMode::ZeroPageIndexedY
                )
                | (
                    InstructionParameter::ZeroPageIndirect(_),
                    AddressingMode::ZeroPageIndirect
                )
                | (
                    InstructionParameter::ZeroPageIndexedIndirectX(_),
                    AddressingMode::ZeroPageIndexedIndirectX,
                )
                | (
                    InstructionParameter::ZeroPageIndirectIndexedY(_),
                    AddressingMode::ZeroPageIndirectIndexedY,
                )
        )
    }

    pub fn write(&self, mut w: impl Write) -> std::io::Result<()> {
        match self {
            InstructionParameter::None => {}
            InstructionParameter::Immediate(value)
            | InstructionParameter::ZeroPage(value)
            | InstructionParameter::ZeroPageIndexedX(value)
            | InstructionParameter::ZeroPageIndexedY(value)
            | InstructionParameter::ZeroPageIndirect(value)
            | InstructionParameter::ZeroPageIndexedIndirectX(value)
            | InstructionParameter::ZeroPageIndirectIndexedY(value) => {
                w.write_all(&value.to_le_bytes())?
            }
            InstructionParameter::ProgramCounterRelative(value) => {
                w.write_all(&value.to_le_bytes())?
            }
            InstructionParameter::Absolute(value)
            | InstructionParameter::AbsoluteIndexedX(value)
            | InstructionParameter::AbsoluteIndexedY(value)
            | InstructionParameter::AbsoluteIndirect(value)
            | InstructionParameter::AbsoluteIndexedIndirectX(value) => {
                w.write_all(&value.to_le_bytes())?
            }
        }
        Ok(())
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct Instruction {
    mnemonic: Mnemonic,
    parameter_1: InstructionParameter,
    parameter_2: InstructionParameter,
}

impl Display for Instruction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.mnemonic)?;
        if self.parameter_1 != InstructionParameter::None {
            write!(f, " {}", self.parameter_1)?;
        }
        if self.parameter_2 != InstructionParameter::None {
            write!(f, ", {}", self.parameter_2)?;
        }
        Ok(())
    }
}

impl Instruction {
    fn get_opcode(&self) -> Option<&'static Opcode> {
        // TODO: optimize
        OPCODES.iter().find(|&opc| {
            self.mnemonic == opc.mnemonic
                && self.parameter_1.matches(opc.parameter_1)
                && self
                    .parameter_2
                    .matches(opc.parameter_2.unwrap_or(AddressingMode::Implied))
        })
    }

    pub fn write(&self, mut w: impl Write) -> std::io::Result<()> {
        if let Some(opcode) = self.get_opcode() {
            w.write_all(&[opcode.opcode])?;
            self.parameter_1.write(w.by_ref())?;
            self.parameter_2.write(w.by_ref())?;
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Invalid opcode",
            ))
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct Opcode {
    pub opcode: u8,
    pub mnemonic: Mnemonic,
    pub parameter_1: AddressingMode,
    pub parameter_2: Option<AddressingMode>,
}

impl Opcode {
    pub fn read_parameters(&self, mut r: impl Read) -> std::io::Result<Instruction> {
        let parameter_1 = self.parameter_1.read(r.by_ref())?;
        let parameter_2 = if let Some(p) = self.parameter_2 {
            p.read(r.by_ref())?
        } else {
            InstructionParameter::None
        };
        Ok(Instruction {
            mnemonic: self.mnemonic,
            parameter_1,
            parameter_2,
        })
    }
}

pub fn disasm(mut r: impl Read) -> Vec<Instruction> {
    let mut instructions = vec![];
    loop {
        let mut buf = [0];
        let result = r.read_exact(&mut buf);
        if let Err(e) = &result {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                break;
            }
        }
        result.unwrap();
        let opcode = buf[0];

        if let Some(opcode) = OPCODE_LOOKUP[opcode as usize] {
            if let Ok(instruction) = opcode.read_parameters(r.by_ref()) {
                instructions.push(instruction);
            } else {
                instructions.push(Mnemonic::Invalid.iinsn());
            }
        } else {
            instructions.push(Mnemonic::NOP.iinsn());
        }
    }
    instructions
}

/// Unordered list of opcodes, do not use for opcode lookup!
pub static OPCODES: [Opcode; 212] = [
    Mnemonic::ADC.opc(0x69, AddressingMode::Immediate),
    Mnemonic::ADC.opc(0x6D, AddressingMode::Absolute),
    Mnemonic::ADC.opc(0x7D, AddressingMode::AbsoluteIndexedX),
    Mnemonic::ADC.opc(0x79, AddressingMode::AbsoluteIndexedY),
    Mnemonic::ADC.opc(0x65, AddressingMode::ZeroPage),
    Mnemonic::ADC.opc(0x75, AddressingMode::ZeroPageIndexedX),
    Mnemonic::ADC.opc(0x61, AddressingMode::ZeroPageIndexedIndirectX),
    Mnemonic::ADC.opc(0x72, AddressingMode::ZeroPageIndirect),
    Mnemonic::ADC.opc(0x71, AddressingMode::ZeroPageIndirectIndexedY),
    Mnemonic::AND.opc(0x29, AddressingMode::Immediate),
    Mnemonic::AND.opc(0x2D, AddressingMode::Absolute),
    Mnemonic::AND.opc(0x3D, AddressingMode::AbsoluteIndexedX),
    Mnemonic::AND.opc(0x39, AddressingMode::AbsoluteIndexedY),
    Mnemonic::AND.opc(0x25, AddressingMode::ZeroPage),
    Mnemonic::AND.opc(0x35, AddressingMode::ZeroPageIndexedX),
    Mnemonic::AND.opc(0x21, AddressingMode::ZeroPageIndexedIndirectX),
    Mnemonic::AND.opc(0x32, AddressingMode::ZeroPageIndirect),
    Mnemonic::AND.opc(0x31, AddressingMode::ZeroPageIndirectIndexedY),
    Mnemonic::ASL.opc(0x0A, AddressingMode::Accumulator),
    Mnemonic::ASL.opc(0x0E, AddressingMode::Absolute),
    Mnemonic::ASL.opc(0x1E, AddressingMode::AbsoluteIndexedX),
    Mnemonic::ASL.opc(0x06, AddressingMode::ZeroPage),
    Mnemonic::ASL.opc(0x16, AddressingMode::ZeroPageIndexedX),
    Mnemonic::BBR0.wopc(
        0x0F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Mnemonic::BBR1.wopc(
        0x1F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Mnemonic::BBR2.wopc(
        0x2F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Mnemonic::BBR3.wopc(
        0x3F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Mnemonic::BBR4.wopc(
        0x4F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Mnemonic::BBR5.wopc(
        0x5F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Mnemonic::BBR6.wopc(
        0x6F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Mnemonic::BBR7.wopc(
        0x7F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Mnemonic::BBS0.wopc(
        0x8F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Mnemonic::BBS1.wopc(
        0x9F,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Mnemonic::BBS2.wopc(
        0xAF,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Mnemonic::BBS3.wopc(
        0xBF,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Mnemonic::BBS4.wopc(
        0xCF,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Mnemonic::BBS5.wopc(
        0xDF,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Mnemonic::BBS6.wopc(
        0xEF,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Mnemonic::BBS7.wopc(
        0xFF,
        AddressingMode::ZeroPage,
        AddressingMode::ProgramCounterRelative,
    ),
    Mnemonic::BCC.opc(0x90, AddressingMode::ProgramCounterRelative),
    Mnemonic::BCS.opc(0xB0, AddressingMode::ProgramCounterRelative),
    Mnemonic::BEQ.opc(0xF0, AddressingMode::ProgramCounterRelative),
    Mnemonic::BIT.opc(0x89, AddressingMode::Immediate),
    Mnemonic::BIT.opc(0x2C, AddressingMode::Absolute),
    Mnemonic::BIT.opc(0x3C, AddressingMode::AbsoluteIndexedX),
    Mnemonic::BIT.opc(0x24, AddressingMode::ZeroPage),
    Mnemonic::BIT.opc(0x34, AddressingMode::ZeroPageIndexedX),
    Mnemonic::BMI.opc(0x30, AddressingMode::ProgramCounterRelative),
    Mnemonic::BNE.opc(0xD0, AddressingMode::ProgramCounterRelative),
    Mnemonic::BPL.opc(0x10, AddressingMode::ProgramCounterRelative),
    Mnemonic::BRA.opc(0x80, AddressingMode::ProgramCounterRelative),
    Mnemonic::BRK.opc(0x00, AddressingMode::Stack),
    Mnemonic::BVC.opc(0x50, AddressingMode::ProgramCounterRelative),
    Mnemonic::BVS.opc(0x70, AddressingMode::ProgramCounterRelative),
    Mnemonic::CLC.opc(0x18, AddressingMode::Implied),
    Mnemonic::CLD.opc(0xD8, AddressingMode::Implied),
    Mnemonic::CLI.opc(0x58, AddressingMode::Implied),
    Mnemonic::CLV.opc(0xB8, AddressingMode::Implied),
    Mnemonic::CMP.opc(0xC9, AddressingMode::Immediate),
    Mnemonic::CMP.opc(0xCD, AddressingMode::Absolute),
    Mnemonic::CMP.opc(0xDD, AddressingMode::AbsoluteIndexedX),
    Mnemonic::CMP.opc(0xD9, AddressingMode::AbsoluteIndexedY),
    Mnemonic::CMP.opc(0xC5, AddressingMode::ZeroPage),
    Mnemonic::CMP.opc(0xD5, AddressingMode::ZeroPageIndexedX),
    Mnemonic::CMP.opc(0xC1, AddressingMode::ZeroPageIndexedIndirectX),
    Mnemonic::CMP.opc(0xD2, AddressingMode::ZeroPageIndirect),
    Mnemonic::CMP.opc(0xD1, AddressingMode::ZeroPageIndirectIndexedY),
    Mnemonic::CPX.opc(0xE0, AddressingMode::Immediate),
    Mnemonic::CPX.opc(0xEC, AddressingMode::Absolute),
    Mnemonic::CPX.opc(0xE4, AddressingMode::ZeroPage),
    Mnemonic::CPY.opc(0xC0, AddressingMode::Immediate),
    Mnemonic::CPY.opc(0xCC, AddressingMode::Absolute),
    Mnemonic::CPY.opc(0xC4, AddressingMode::ZeroPage),
    Mnemonic::DEC.opc(0x3A, AddressingMode::Accumulator),
    Mnemonic::DEC.opc(0xCE, AddressingMode::Absolute),
    Mnemonic::DEC.opc(0xDE, AddressingMode::AbsoluteIndexedX),
    Mnemonic::DEC.opc(0xC6, AddressingMode::ZeroPage),
    Mnemonic::DEC.opc(0xD6, AddressingMode::ZeroPageIndexedX),
    Mnemonic::DEX.opc(0xCA, AddressingMode::Implied),
    Mnemonic::DEY.opc(0x88, AddressingMode::Implied),
    Mnemonic::EOR.opc(0x49, AddressingMode::Immediate),
    Mnemonic::EOR.opc(0x4D, AddressingMode::Absolute),
    Mnemonic::EOR.opc(0x5D, AddressingMode::AbsoluteIndexedX),
    Mnemonic::EOR.opc(0x59, AddressingMode::AbsoluteIndexedY),
    Mnemonic::EOR.opc(0x45, AddressingMode::ZeroPage),
    Mnemonic::EOR.opc(0x55, AddressingMode::ZeroPageIndexedX),
    Mnemonic::EOR.opc(0x41, AddressingMode::ZeroPageIndexedIndirectX),
    Mnemonic::EOR.opc(0x52, AddressingMode::ZeroPageIndirect),
    Mnemonic::EOR.opc(0x51, AddressingMode::ZeroPageIndirectIndexedY),
    Mnemonic::INC.opc(0x1A, AddressingMode::Accumulator),
    Mnemonic::INC.opc(0xEE, AddressingMode::Absolute),
    Mnemonic::INC.opc(0xFE, AddressingMode::AbsoluteIndexedX),
    Mnemonic::INC.opc(0xE6, AddressingMode::ZeroPage),
    Mnemonic::INC.opc(0xF6, AddressingMode::ZeroPageIndexedX),
    Mnemonic::INX.opc(0xE8, AddressingMode::Implied),
    Mnemonic::INY.opc(0xC8, AddressingMode::Implied),
    Mnemonic::JMP.opc(0x4C, AddressingMode::Absolute),
    Mnemonic::JMP.opc(0x7C, AddressingMode::AbsoluteIndexedIndirectX),
    Mnemonic::JMP.opc(0x6C, AddressingMode::AbsoluteIndirect),
    Mnemonic::JSR.opc(0x20, AddressingMode::Absolute),
    Mnemonic::LDA.opc(0xA9, AddressingMode::Immediate),
    Mnemonic::LDA.opc(0xAD, AddressingMode::Absolute),
    Mnemonic::LDA.opc(0xBD, AddressingMode::AbsoluteIndexedX),
    Mnemonic::LDA.opc(0xB9, AddressingMode::AbsoluteIndexedY),
    Mnemonic::LDA.opc(0xA5, AddressingMode::ZeroPage),
    Mnemonic::LDA.opc(0xB5, AddressingMode::ZeroPageIndexedX),
    Mnemonic::LDA.opc(0xA1, AddressingMode::ZeroPageIndexedIndirectX),
    Mnemonic::LDA.opc(0xB2, AddressingMode::ZeroPageIndirect),
    Mnemonic::LDA.opc(0xB1, AddressingMode::ZeroPageIndirectIndexedY),
    Mnemonic::LDX.opc(0xA2, AddressingMode::Immediate),
    Mnemonic::LDX.opc(0xAE, AddressingMode::Absolute),
    Mnemonic::LDX.opc(0xBE, AddressingMode::AbsoluteIndexedY),
    Mnemonic::LDX.opc(0xA6, AddressingMode::ZeroPage),
    Mnemonic::LDX.opc(0xB6, AddressingMode::ZeroPageIndexedY),
    Mnemonic::LDY.opc(0xA0, AddressingMode::Immediate),
    Mnemonic::LDY.opc(0xAC, AddressingMode::Absolute),
    Mnemonic::LDY.opc(0xBC, AddressingMode::AbsoluteIndexedX),
    Mnemonic::LDY.opc(0xA4, AddressingMode::ZeroPage),
    Mnemonic::LDY.opc(0xB4, AddressingMode::ZeroPageIndexedX),
    Mnemonic::LSR.opc(0x4A, AddressingMode::Accumulator),
    Mnemonic::LSR.opc(0x4E, AddressingMode::Absolute),
    Mnemonic::LSR.opc(0x5E, AddressingMode::AbsoluteIndexedX),
    Mnemonic::LSR.opc(0x46, AddressingMode::ZeroPage),
    Mnemonic::LSR.opc(0x56, AddressingMode::ZeroPageIndexedX),
    Mnemonic::NOP.opc(0xEA, AddressingMode::Implied),
    Mnemonic::ORA.opc(0x09, AddressingMode::Immediate),
    Mnemonic::ORA.opc(0x0D, AddressingMode::Absolute),
    Mnemonic::ORA.opc(0x1D, AddressingMode::AbsoluteIndexedX),
    Mnemonic::ORA.opc(0x19, AddressingMode::AbsoluteIndexedY),
    Mnemonic::ORA.opc(0x05, AddressingMode::ZeroPage),
    Mnemonic::ORA.opc(0x15, AddressingMode::ZeroPageIndexedX),
    Mnemonic::ORA.opc(0x01, AddressingMode::ZeroPageIndexedIndirectX),
    Mnemonic::ORA.opc(0x12, AddressingMode::ZeroPageIndirect),
    Mnemonic::ORA.opc(0x11, AddressingMode::ZeroPageIndirectIndexedY),
    Mnemonic::PHA.opc(0x48, AddressingMode::Stack),
    Mnemonic::PHP.opc(0x08, AddressingMode::Stack),
    Mnemonic::PHX.opc(0xDA, AddressingMode::Stack),
    Mnemonic::PHY.opc(0x5A, AddressingMode::Stack),
    Mnemonic::PLA.opc(0x68, AddressingMode::Stack),
    Mnemonic::PLP.opc(0x28, AddressingMode::Stack),
    Mnemonic::PLX.opc(0xFA, AddressingMode::Stack),
    Mnemonic::PLY.opc(0x7A, AddressingMode::Stack),
    Mnemonic::RMB0.opc(0x07, AddressingMode::ZeroPage),
    Mnemonic::RMB1.opc(0x17, AddressingMode::ZeroPage),
    Mnemonic::RMB2.opc(0x27, AddressingMode::ZeroPage),
    Mnemonic::RMB3.opc(0x37, AddressingMode::ZeroPage),
    Mnemonic::RMB4.opc(0x47, AddressingMode::ZeroPage),
    Mnemonic::RMB5.opc(0x57, AddressingMode::ZeroPage),
    Mnemonic::RMB6.opc(0x67, AddressingMode::ZeroPage),
    Mnemonic::RMB7.opc(0x77, AddressingMode::ZeroPage),
    Mnemonic::ROL.opc(0x2A, AddressingMode::Accumulator),
    Mnemonic::ROL.opc(0x2E, AddressingMode::Absolute),
    Mnemonic::ROL.opc(0x3E, AddressingMode::AbsoluteIndexedX),
    Mnemonic::ROL.opc(0x26, AddressingMode::ZeroPage),
    Mnemonic::ROL.opc(0x36, AddressingMode::ZeroPageIndexedX),
    Mnemonic::ROR.opc(0x6A, AddressingMode::Accumulator),
    Mnemonic::ROR.opc(0x6E, AddressingMode::Absolute),
    Mnemonic::ROR.opc(0x7E, AddressingMode::AbsoluteIndexedX),
    Mnemonic::ROR.opc(0x66, AddressingMode::ZeroPage),
    Mnemonic::ROR.opc(0x76, AddressingMode::ZeroPageIndexedX),
    Mnemonic::RTI.opc(0x40, AddressingMode::Stack),
    Mnemonic::RTS.opc(0x60, AddressingMode::Stack),
    Mnemonic::SBC.opc(0xE9, AddressingMode::Immediate),
    Mnemonic::SBC.opc(0xED, AddressingMode::Absolute),
    Mnemonic::SBC.opc(0xFD, AddressingMode::AbsoluteIndexedX),
    Mnemonic::SBC.opc(0xF9, AddressingMode::AbsoluteIndexedY),
    Mnemonic::SBC.opc(0xE5, AddressingMode::ZeroPage),
    Mnemonic::SBC.opc(0xF5, AddressingMode::ZeroPageIndexedX),
    Mnemonic::SBC.opc(0xE1, AddressingMode::ZeroPageIndexedIndirectX),
    Mnemonic::SBC.opc(0xF2, AddressingMode::ZeroPageIndirect),
    Mnemonic::SBC.opc(0xF1, AddressingMode::ZeroPageIndirectIndexedY),
    Mnemonic::SEC.opc(0x38, AddressingMode::Implied),
    Mnemonic::SED.opc(0xF8, AddressingMode::Implied),
    Mnemonic::SEI.opc(0x78, AddressingMode::Implied),
    Mnemonic::SMB0.opc(0x87, AddressingMode::ZeroPage),
    Mnemonic::SMB1.opc(0x97, AddressingMode::ZeroPage),
    Mnemonic::SMB2.opc(0xA7, AddressingMode::ZeroPage),
    Mnemonic::SMB3.opc(0xB7, AddressingMode::ZeroPage),
    Mnemonic::SMB4.opc(0xC7, AddressingMode::ZeroPage),
    Mnemonic::SMB5.opc(0xD7, AddressingMode::ZeroPage),
    Mnemonic::SMB6.opc(0xE7, AddressingMode::ZeroPage),
    Mnemonic::SMB7.opc(0xF7, AddressingMode::ZeroPage),
    Mnemonic::STA.opc(0x8D, AddressingMode::Absolute),
    Mnemonic::STA.opc(0x9D, AddressingMode::AbsoluteIndexedX),
    Mnemonic::STA.opc(0x99, AddressingMode::AbsoluteIndexedY),
    Mnemonic::STA.opc(0x85, AddressingMode::ZeroPage),
    Mnemonic::STA.opc(0x95, AddressingMode::ZeroPageIndexedX),
    Mnemonic::STA.opc(0x81, AddressingMode::ZeroPageIndexedIndirectX),
    Mnemonic::STA.opc(0x92, AddressingMode::ZeroPageIndirect),
    Mnemonic::STA.opc(0x91, AddressingMode::ZeroPageIndirectIndexedY),
    Mnemonic::STP.opc(0xDB, AddressingMode::Implied),
    Mnemonic::STX.opc(0x8E, AddressingMode::Absolute),
    Mnemonic::STX.opc(0x86, AddressingMode::ZeroPage),
    Mnemonic::STX.opc(0x96, AddressingMode::ZeroPageIndexedY),
    Mnemonic::STY.opc(0x8C, AddressingMode::Absolute),
    Mnemonic::STY.opc(0x84, AddressingMode::ZeroPage),
    Mnemonic::STY.opc(0x94, AddressingMode::ZeroPageIndexedX),
    Mnemonic::STZ.opc(0x9C, AddressingMode::Absolute),
    Mnemonic::STZ.opc(0x9E, AddressingMode::AbsoluteIndexedX),
    Mnemonic::STZ.opc(0x64, AddressingMode::ZeroPage),
    Mnemonic::STZ.opc(0x74, AddressingMode::ZeroPageIndexedX),
    Mnemonic::TAX.opc(0xAA, AddressingMode::Implied),
    Mnemonic::TAY.opc(0xA8, AddressingMode::Implied),
    Mnemonic::TRB.opc(0x1C, AddressingMode::Absolute),
    Mnemonic::TRB.opc(0x14, AddressingMode::ZeroPage),
    Mnemonic::TSB.opc(0x0C, AddressingMode::Absolute),
    Mnemonic::TSB.opc(0x04, AddressingMode::ZeroPage),
    Mnemonic::TSX.opc(0xBA, AddressingMode::Implied),
    Mnemonic::TXA.opc(0x8A, AddressingMode::Implied),
    Mnemonic::TXS.opc(0x9A, AddressingMode::Implied),
    Mnemonic::TYA.opc(0x98, AddressingMode::Implied),
    Mnemonic::WAI.opc(0xCB, AddressingMode::Implied),
];

lazy_static! {
    /// Lookup table for opcodes
    pub static ref OPCODE_LOOKUP: [Option<&'static Opcode>; 256] = {
        let mut opcodes: [Option<&'static Opcode>; 256] = [None; 256];
        for opc in &OPCODES {
            let n = opc.opcode as usize;
            let p = opcodes.get_mut(n).expect("opcode out of bounds");
            if let Some(current) = p {
                panic!("{n:#X}: opcode already present | current={current:?} new={opc:?}");
            } else {
                *p = Some(opc);
            }
        }
        opcodes
    };
}
