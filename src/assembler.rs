use crate::opcode::{AddressingMode, InstructionMeta, Opcode, get_instructions};
use itertools::Itertools;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::io::{Read, Write};
use strum::Display;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AssemblerError {
    #[error("generic error: {0}")]
    Generic(String),
    #[error("double label: {0}")]
    DoubleLabel(String),
    #[error("unknown label: {0}")]
    UnknownLabel(String),
    #[error("address overflow")]
    AddressOverflow,
    #[error("invalid opcode")]
    InvalidOpcode,
    #[error("parameter mismatch: {0}")]
    ParameterMismatch(String),
    #[error("jump too far")]
    JumpTooFar,
    #[error("io error: {0}")]
    IO(#[from] std::io::Error),
}

pub trait MnemonicDSL: Sized {
    fn labelled(self, label: impl Into<String>) -> Instruction {
        self.labelled_with(label, Parameter::None)
    }

    fn labelled_with(self, label: impl Into<String>, parameter: Parameter) -> Instruction;

    fn with(self, parameter: Parameter) -> Instruction;

    fn instruction(self) -> Instruction {
        self.with(Parameter::None)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Display)]
pub enum Mnemonic {
    Opcode(Opcode),
    PseudoOp(PseudoInstruction),
}

impl<T: Into<Mnemonic>> MnemonicDSL for T {
    fn labelled_with(self, label: impl Into<String>, parameter: Parameter) -> Instruction {
        Instruction {
            label: Some(label.into()),
            mnemonic: self.into(),
            parameter,
        }
    }

    fn with(self, parameter: Parameter) -> Instruction {
        Instruction {
            label: None,
            mnemonic: self.into(),
            parameter,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Display)]
pub enum PseudoInstruction {
    BBR,
    BBS,
    RMB,
    SMB,
}

impl From<PseudoInstruction> for Mnemonic {
    fn from(value: PseudoInstruction) -> Self {
        Self::PseudoOp(value)
    }
}

impl From<Opcode> for Mnemonic {
    fn from(value: Opcode) -> Self {
        Self::Opcode(value)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Display)]
pub enum Parameter {
    None,
    A,
    X,
    Y,
    #[strum(to_string = "#{0}")]
    Immediate(u8),
    #[strum(to_string = "{0}")]
    Absolute(u16),
    #[strum(to_string = "{0}")]
    Label(String),
    #[strum(to_string = "({0})")]
    Indirect(Box<Parameter>),
    #[strum(to_string = "{0:?}")]
    List(Vec<Parameter>),
}

impl Parameter {
    pub fn label(label: impl Into<String>) -> Self {
        Self::Label(label.into())
    }

    pub fn list(parameters: impl Into<Vec<Parameter>>) -> Self {
        Self::List(parameters.into())
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Instruction {
    label: Option<String>,
    mnemonic: Mnemonic,
    parameter: Parameter,
}

impl Display for Instruction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(label) = &self.label {
            write!(f, "{label}: ")?;
        }
        write!(f, "{}", self.mnemonic)?;
        if self.parameter != Parameter::None {
            write!(f, " {}", self.parameter)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum AssembledParameter {
    Label(String),
    U8(u8),
    U16(u16),
}

#[derive(Debug, Clone)]
pub struct AssembledInstruction {
    instruction: &'static InstructionMeta,
    parameter_1: Option<AssembledParameter>,
    parameter_2: Option<AssembledParameter>,
}

impl AssembledInstruction {
    fn assemble(instruction: &Instruction) -> Result<Self, AssemblerError> {
        match instruction.mnemonic {
            Mnemonic::Opcode(opcode) => {
                let candidates = get_instructions(opcode);
                let ((mut mode_1, mut parameter_1), mode_param_2) =
                    Self::parse_parameters(instruction)?;
                let (mode_2, parameter_2) = if let Some((x, y)) = mode_param_2 {
                    (x, y)
                } else {
                    (AddressingMode::None, None)
                };

                // zeropage optimizations (only for parameter_1)
                if let Some(AssembledParameter::U16(number)) = parameter_1
                    && (0..=u8::MAX as u16).contains(&number)
                {
                    let mut zp_optimize = |abs_mode: AddressingMode, zp_mode: AddressingMode| {
                        if mode_1 == abs_mode && candidates.iter().any(|c| c.parameter_1 == zp_mode)
                        {
                            mode_1 = zp_mode;
                            parameter_1 = Some(AssembledParameter::U8(number as u8));
                        }
                    };

                    zp_optimize(AddressingMode::Absolute, AddressingMode::ZeroPage);
                    zp_optimize(
                        AddressingMode::AbsoluteIndexedX,
                        AddressingMode::ZeroPageIndexedX,
                    );
                    zp_optimize(
                        AddressingMode::AbsoluteIndexedY,
                        AddressingMode::ZeroPageIndexedY,
                    );
                    zp_optimize(
                        AddressingMode::AbsoluteIndirect,
                        AddressingMode::ZeroPageIndirect,
                    );
                    zp_optimize(
                        AddressingMode::AbsoluteIndexedIndirectX,
                        AddressingMode::ZeroPageIndexedIndirectX,
                    );
                }

                // special handling for labels
                if matches!(parameter_1, Some(AssembledParameter::Label(_)))
                    || matches!(parameter_2, Some(AssembledParameter::Label(_)))
                {
                    // only works when there is one possible opcode
                    // TODO: implement selection between relative and absolute
                    let candidate = candidates.iter().exactly_one().map_err(|_| {
                        AssemblerError::ParameterMismatch(format!(
                            "multiple candidates for labelled instruction {:?}",
                            instruction.mnemonic
                        ))
                    })?;
                    return Ok(AssembledInstruction {
                        instruction: candidate,
                        parameter_1,
                        parameter_2,
                    });
                }

                for candidate in candidates {
                    // special handling for BRK with no argument
                    if candidate.opcode == Opcode::BRK
                        && matches!(mode_1, AddressingMode::None)
                        && mode_2 == AddressingMode::None
                        && parameter_2.is_none()
                        && let Some(AssembledParameter::U8(_)) = parameter_1
                    {
                        return Ok(AssembledInstruction {
                            instruction: candidate,
                            parameter_1,
                            parameter_2,
                        });
                    }

                    // TODO: better matching for not exactly fitting addressing modes
                    match (candidate.parameter_1, mode_1, candidate.parameter_2, mode_2) {
                        (p1, m1, p2, m2) if p1 == m1 && p2 == m2 => {
                            return Ok(AssembledInstruction {
                                instruction: candidate,
                                parameter_1,
                                parameter_2,
                            });
                        }
                        _ => {}
                    }
                }

                Err(AssemblerError::ParameterMismatch(format!(
                    "could not find matching instruction for {:?}",
                    instruction.mnemonic
                )))
            }
            Mnemonic::PseudoOp(pseudo) => match pseudo {
                PseudoInstruction::BBR => todo!(),
                PseudoInstruction::BBS => todo!(),
                PseudoInstruction::RMB => todo!(),
                PseudoInstruction::SMB => todo!(),
            },
        }
    }

    #[allow(clippy::type_complexity)]
    fn parse_parameters(
        instruction: &Instruction,
    ) -> Result<
        (
            (AddressingMode, Option<AssembledParameter>),
            Option<(AddressingMode, Option<AssembledParameter>)>,
        ),
        AssemblerError,
    > {
        Ok(match &instruction.parameter {
            Parameter::None => ((AddressingMode::None, None), None),
            Parameter::A => ((AddressingMode::Accumulator, None), None),
            Parameter::Immediate(number) => (
                (
                    AddressingMode::Immediate,
                    Some(AssembledParameter::U8(*number)),
                ),
                None,
            ),
            Parameter::Absolute(number) => (
                (
                    AddressingMode::Absolute,
                    Some(AssembledParameter::U16(*number)),
                ),
                None,
            ),
            Parameter::Label(label) => (
                (
                    AddressingMode::None, // placeholder
                    Some(AssembledParameter::Label(label.to_string())),
                ),
                None,
            ),
            Parameter::Indirect(inner) => match inner.as_ref() {
                Parameter::Absolute(number) => (
                    (
                        AddressingMode::AbsoluteIndirect,
                        Some(AssembledParameter::U16(*number)),
                    ),
                    None,
                ),
                Parameter::List(inner) => match inner.as_slice() {
                    [Parameter::Absolute(number), Parameter::X] => (
                        (
                            AddressingMode::AbsoluteIndexedIndirectX,
                            Some(AssembledParameter::U16(*number)),
                        ),
                        None,
                    ),
                    [Parameter::Label(label), Parameter::X] => (
                        (
                            AddressingMode::AbsoluteIndexedIndirectX,
                            Some(AssembledParameter::Label(label.to_string())),
                        ),
                        None,
                    ),
                    _ => {
                        return Err(AssemblerError::ParameterMismatch(format!(
                            "could not match parameters with addressing mode: {:?}",
                            instruction.mnemonic
                        )));
                    }
                },
                _ => {
                    return Err(AssemblerError::ParameterMismatch(format!(
                        "could not match parameters with addressing mode: {:?}",
                        instruction.mnemonic
                    )));
                }
            },
            Parameter::List(inner) => match inner.as_slice() {
                [Parameter::Absolute(number), Parameter::X] => (
                    (
                        AddressingMode::AbsoluteIndexedX,
                        Some(AssembledParameter::U16(*number)),
                    ),
                    None,
                ),
                [Parameter::Label(label), Parameter::X] => (
                    (
                        AddressingMode::AbsoluteIndexedX,
                        Some(AssembledParameter::Label(label.to_string())),
                    ),
                    None,
                ),
                [Parameter::Absolute(number), Parameter::Y] => (
                    (
                        AddressingMode::AbsoluteIndexedY,
                        Some(AssembledParameter::U16(*number)),
                    ),
                    None,
                ),
                [Parameter::Label(label), Parameter::Y] => (
                    (
                        AddressingMode::AbsoluteIndexedY,
                        Some(AssembledParameter::Label(label.to_string())),
                    ),
                    None,
                ),
                [Parameter::Indirect(inner), Parameter::Y] => match inner.as_ref() {
                    Parameter::Absolute(number) if (0..=u8::MAX as u16).contains(number) => (
                        (
                            AddressingMode::ZeroPageIndirectIndexedY,
                            Some(AssembledParameter::U16(*number)),
                        ),
                        None,
                    ),
                    _ => {
                        return Err(AssemblerError::ParameterMismatch(format!(
                            "could not match parameters with addressing mode: {:?}",
                            instruction.mnemonic
                        )));
                    }
                },
                [Parameter::Absolute(number), Parameter::Label(label)] => (
                    (
                        AddressingMode::Absolute,
                        Some(AssembledParameter::U16(*number)),
                    ),
                    Some((
                        AddressingMode::ProgramCounterRelative,
                        Some(AssembledParameter::Label(label.to_string())),
                    )),
                ),
                _ => {
                    return Err(AssemblerError::ParameterMismatch(format!(
                        "could not match parameters with addressing mode: {:?}",
                        instruction.mnemonic
                    )));
                }
            },
            _ => {
                return Err(AssemblerError::ParameterMismatch(format!(
                    "could not match parameters with addressing mode: {:?}",
                    instruction.mnemonic
                )));
            }
        })
    }

    fn fill_label(
        parameter: &mut AssembledParameter,
        addressing_mode: AddressingMode,
        address: u16,
        labels: &HashMap<String, u16>,
    ) -> Result<(), AssemblerError> {
        if let AssembledParameter::Label(label) = parameter {
            let resolved = labels
                .get(label)
                .copied()
                .ok_or_else(|| AssemblerError::UnknownLabel((*label).to_string()))?;
            match addressing_mode {
                AddressingMode::ProgramCounterRelative => {
                    // pc + n = resolved <=> n = resolved - pc
                    let diff = if resolved < address {
                        let d = address - resolved;
                        if (0..=128).contains(&d) {
                            // 128u16 as i8 is -128
                            // (-128i8).wrapping_neg() is -128
                            Ok((d as i8).wrapping_neg())
                        } else {
                            Err(AssemblerError::JumpTooFar)
                        }
                    } else {
                        let d = resolved - address;
                        if (0..=127).contains(&d) {
                            Ok(d as i8)
                        } else {
                            Err(AssemblerError::JumpTooFar)
                        }
                    }?;
                    *parameter = AssembledParameter::U8(diff as u8);
                }
                AddressingMode::Absolute
                | AddressingMode::AbsoluteIndexedX
                | AddressingMode::AbsoluteIndexedY
                | AddressingMode::AbsoluteIndirect
                | AddressingMode::AbsoluteIndexedIndirectX => {
                    *parameter = AssembledParameter::U16(resolved);
                }
                _ => {
                    return Err(AssemblerError::ParameterMismatch(format!(
                        "could not replace label with actual address: {addressing_mode:?}"
                    )));
                }
            }
        }
        Ok(())
    }

    fn fill_labels(
        &mut self,
        address: u16,
        labels: &HashMap<String, u16>,
    ) -> Result<(), AssemblerError> {
        if let Some(parameter_1) = &mut self.parameter_1 {
            Self::fill_label(parameter_1, self.instruction.parameter_1, address, labels)?;
        }
        if let Some(parameter_2) = &mut self.parameter_2 {
            Self::fill_label(parameter_2, self.instruction.parameter_2, address, labels)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Assembly {
    instructions: Vec<Instruction>,
    labels: HashMap<String, u16>,
    assembled_instructions: Vec<AssembledInstruction>,
}

impl Assembly {
    fn from_instructions(instructions: impl Into<Vec<Instruction>>) -> Self {
        Self {
            instructions: instructions.into(),
            labels: HashMap::new(),
            assembled_instructions: vec![],
        }
    }

    fn assemble(&mut self) -> Result<(), AssemblerError> {
        // pass 1: find opcodes and offsets, collect params
        let mut address = 0u16;
        for instruction in &self.instructions {
            if let Some(label) = &instruction.label
                && self.labels.insert(label.to_string(), address).is_some()
            {
                return Err(AssemblerError::DoubleLabel(label.to_string()));
            }

            let assembled = AssembledInstruction::assemble(instruction)?;
            address = address
                .checked_add(assembled.instruction.width())
                .ok_or(AssemblerError::AddressOverflow)?;
            self.assembled_instructions.push(assembled);
        }

        // pass 2: labels
        let mut address = 0u16;
        for (_instruction, assembled) in
            std::iter::zip(&self.instructions, &mut self.assembled_instructions)
        {
            address += assembled.instruction.width();
            assembled.fill_labels(address, &self.labels)?;
        }

        Ok(())
    }

    fn write(&self, mut w: impl Write) -> std::io::Result<()> {
        for assembled in &self.assembled_instructions {
            w.write_all(&[assembled.instruction.byte])?;
            for p in [
                assembled.parameter_1.as_ref(),
                assembled.parameter_2.as_ref(),
            ]
            .iter()
            .flatten()
            {
                match p {
                    AssembledParameter::U8(number) => w.write_all(&[*number])?,
                    AssembledParameter::U16(number) => w.write_all(&number.to_le_bytes())?,
                    AssembledParameter::Label(_) => unreachable!(),
                }
            }
        }
        Ok(())
    }
}

pub fn assemble(instructions: &[Instruction], w: impl Write) -> Result<(), AssemblerError> {
    let mut assembly = Assembly::from_instructions(instructions);
    assembly.assemble()?;
    assembly.write(w)?;
    Ok(())
}

pub fn disassemble(_r: impl Read) -> Vec<Instruction> {
    let instructions = vec![];
    // TODO
    /*loop {
        let mut buf = [0];
        let result = r.read_exact(&mut buf);
        if let Err(e) = &result {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                break;
            }
        }
        result.unwrap();
        let opcode = buf[0];

        if let Some(opcode) = get_instruction(opcode) {
            if let Ok(instruction) = opcode.read_parameters(r.by_ref()) {
                instructions.push(instruction);
            } else {
                instructions.push(crate::opcode::Opcode::Invalid.iinsn());
            }
        } else {
            instructions.push(crate::opcode::Opcode::NOP.iinsn());
        }
    }*/
    instructions
}
