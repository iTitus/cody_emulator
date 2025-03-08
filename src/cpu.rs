use crate::memory::Memory;
use crate::opcode::{get_opcode, AddressingMode, Mnemonic};
use bitfields::bitfield;

#[bitfield(u8)]
#[derive(Copy, Clone)]
pub struct Status {
    carry: bool,
    zero: bool,
    #[bits(default = true)]
    irqb_disable: bool,
    decimal_mode: bool,
    #[bits(default = true)]
    brk_command: bool,
    #[bits(default = true)]
    _unused: bool,
    overflow: bool,
    negative: bool,
}

#[derive(Debug, Default)]
pub struct Cpu<M> {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub s: u8,
    pub p: Status,
    pub pc: u16,
    pub memory: M,
}

impl<M: Memory> Cpu<M> {
    pub fn new(memory: M) -> Self {
        let mut cpu = Self {
            a: 0,
            x: 0,
            y: 0,
            s: 0xFD,
            p: Status::default(),
            pc: 0,
            memory,
        };
        cpu.pc = cpu.memory.read_u16(0xFFFC);
        cpu
    }

    pub fn run(&mut self) {
        loop {
            let opcode = get_opcode(self.read_u8_inc_pc());
            if let Some(opcode) = opcode {
                match opcode.mnemonic {
                    Mnemonic::Invalid => unimplemented!(),
                    Mnemonic::ADC => {
                        let m = self.read_operand(opcode.parameter_1);
                        self.do_addition(m);
                    }
                    Mnemonic::AND => {
                        let m = self.read_operand(opcode.parameter_1);
                        self.set_a(self.a & m);
                    }
                    Mnemonic::ASL => {
                        if opcode.parameter_1 == AddressingMode::Accumulator {
                            let m = self.a;
                            self.set_a(m << 1);
                            self.p.set_carry((m & 0x80) != 0);
                        } else {
                            let addr = self.read_address(opcode.parameter_1);
                            let m = self.memory.read_u8(addr);
                            let value = m << 1;
                            self.memory.write_u8(addr, value);
                            self.update_nz_flags(value);
                            self.p.set_carry((m & 0x80) != 0);
                        }
                    }
                    Mnemonic::BBR0 => self.bbr(0),
                    Mnemonic::BBR1 => self.bbr(1),
                    Mnemonic::BBR2 => self.bbr(2),
                    Mnemonic::BBR3 => self.bbr(3),
                    Mnemonic::BBR4 => self.bbr(4),
                    Mnemonic::BBR6 => self.bbr(5),
                    Mnemonic::BBR5 => self.bbr(6),
                    Mnemonic::BBR7 => self.bbr(7),
                    Mnemonic::BBS0 => self.bbs(0),
                    Mnemonic::BBS1 => self.bbs(1),
                    Mnemonic::BBS2 => self.bbs(2),
                    Mnemonic::BBS3 => self.bbs(3),
                    Mnemonic::BBS4 => self.bbs(4),
                    Mnemonic::BBS6 => self.bbs(5),
                    Mnemonic::BBS5 => self.bbs(6),
                    Mnemonic::BBS7 => self.bbs(7),
                    Mnemonic::BCC => self.branch(!self.p.carry()),
                    Mnemonic::BCS => self.branch(self.p.carry()),
                    Mnemonic::BEQ => self.branch(self.p.zero()),
                    Mnemonic::BIT => {
                        let m = self.read_operand(opcode.parameter_1);
                        self.p.set_zero((self.a & m) == 0);
                        // TODO: some sources say this does not happen with immediate operand
                        self.p.set_negative((m & 0x80) != 0);
                        self.p.set_carry((m & 0x40) != 0);
                    }
                    Mnemonic::BMI => self.branch(self.p.negative()),
                    Mnemonic::BNE => self.branch(!self.p.zero()),
                    Mnemonic::BPL => self.branch(!self.p.negative()),
                    Mnemonic::BRA => self.branch(true),
                    Mnemonic::BRK => {
                        // TODO: check correctness
                        // what happens if irqb_disable = 1?
                        self.pc += 1;
                        self.p.set_brk_command(true);
                        self.push_pc();
                        self.push_flags();
                        self.push(self.p.into());
                        self.p.set_decimal_mode(false); // ?
                        self.pc = self.memory.read_u16(0xFFFE);
                    }
                    Mnemonic::BVC => self.branch(!self.p.overflow()),
                    Mnemonic::BVS => self.branch(self.p.overflow()),
                    Mnemonic::CLC => self.p.set_carry(false),
                    Mnemonic::CLD => self.p.set_decimal_mode(false),
                    Mnemonic::CLI => self.p.set_irqb_disable(false),
                    Mnemonic::CLV => self.p.set_overflow(false),
                    Mnemonic::CMP => self.cmp(self.a, opcode.parameter_1),
                    Mnemonic::CPX => self.cmp(self.x, opcode.parameter_1),
                    Mnemonic::CPY => self.cmp(self.y, opcode.parameter_1),
                    Mnemonic::DEC => {
                        if opcode.parameter_1 == AddressingMode::Accumulator {
                            self.set_a(self.a.wrapping_sub(1));
                        } else {
                            let addr = self.read_address(opcode.parameter_1);
                            let value = self.memory.read_u8(addr);
                            let new_value = value.wrapping_sub(1);
                            self.memory.write_u8(addr, new_value);
                            self.update_nz_flags(new_value);
                        }
                    }
                    Mnemonic::DEX => self.set_x(self.x.wrapping_sub(1)),
                    Mnemonic::DEY => self.set_y(self.y.wrapping_sub(1)),
                    Mnemonic::EOR => {
                        let m = self.read_operand(opcode.parameter_1);
                        self.set_a(self.a ^ m);
                    }
                    Mnemonic::INC => {
                        if opcode.parameter_1 == AddressingMode::Accumulator {
                            self.set_a(self.a.wrapping_add(1));
                        } else {
                            let addr = self.read_address(opcode.parameter_1);
                            let value = self.memory.read_u8(addr);
                            let new_value = value.wrapping_add(1);
                            self.memory.write_u8(addr, new_value);
                            self.update_nz_flags(new_value);
                        }
                    }
                    Mnemonic::INX => self.set_x(self.x.wrapping_add(1)),
                    Mnemonic::INY => self.set_y(self.y.wrapping_add(1)),
                    Mnemonic::JMP => {
                        let target = self.read_address(opcode.parameter_1);
                        self.pc = target;
                    }
                    Mnemonic::JSR => {
                        let target = self.read_address(opcode.parameter_1);
                        self.pc = self.pc.wrapping_sub(1); // why?
                        self.push_pc();
                        self.pc = target;
                    }
                    Mnemonic::LDA => {
                        let op = self.read_operand(opcode.parameter_1);
                        self.set_a(op);
                    }
                    Mnemonic::LDX => {
                        let op = self.read_operand(opcode.parameter_1);
                        self.set_x(op);
                    }
                    Mnemonic::LDY => {
                        let op = self.read_operand(opcode.parameter_1);
                        self.set_y(op);
                    }
                    Mnemonic::LSR => {
                        if opcode.parameter_1 == AddressingMode::Accumulator {
                            let m = self.a;
                            self.set_a(m >> 1);
                            self.p.set_carry((m & 0b1) != 0);
                        } else {
                            let addr = self.read_address(opcode.parameter_1);
                            let m = self.memory.read_u8(addr);
                            let value = m >> 1;
                            self.memory.write_u8(addr, value);
                            self.update_nz_flags(value);
                            self.p.set_carry((m & 0b1) != 0);
                        }
                    }
                    Mnemonic::NOP => {}
                    Mnemonic::ORA => {
                        let m = self.read_operand(opcode.parameter_1);
                        self.set_a(self.a | m);
                    }
                    Mnemonic::PHA => self.push(self.a),
                    Mnemonic::PHP => self.push_flags(),
                    Mnemonic::PHX => self.push(self.x),
                    Mnemonic::PHY => self.push(self.y),
                    Mnemonic::PLA => {
                        let value = self.pop();
                        self.set_a(value);
                    }
                    Mnemonic::PLP => self.pop_flags(),
                    Mnemonic::PLX => {
                        let value = self.pop();
                        self.set_x(value);
                    }
                    Mnemonic::PLY => {
                        let value = self.pop();
                        self.set_y(value);
                    }
                    Mnemonic::RMB0 => self.rmb(0),
                    Mnemonic::RMB1 => self.rmb(1),
                    Mnemonic::RMB2 => self.rmb(2),
                    Mnemonic::RMB3 => self.rmb(3),
                    Mnemonic::RMB4 => self.rmb(4),
                    Mnemonic::RMB5 => self.rmb(5),
                    Mnemonic::RMB6 => self.rmb(6),
                    Mnemonic::RMB7 => self.rmb(7),
                    Mnemonic::ROL => {
                        if opcode.parameter_1 == AddressingMode::Accumulator {
                            let m = self.a;
                            self.set_a((m << 1) | self.p.carry() as u8);
                            self.p.set_carry((m & 0x80) != 0);
                        } else {
                            let addr = self.read_address(opcode.parameter_1);
                            let m = self.memory.read_u8(addr);
                            let value = (m << 1) | self.p.carry() as u8;
                            self.memory.write_u8(addr, value);
                            self.update_nz_flags(value);
                            self.p.set_carry((m & 0x80) != 0);
                        }
                    }
                    Mnemonic::ROR => {
                        if opcode.parameter_1 == AddressingMode::Accumulator {
                            let m = self.a;
                            self.set_a((m >> 1) | ((self.p.carry() as u8) << 7));
                            self.p.set_carry((m & 0b1) != 0);
                        } else {
                            let addr = self.read_address(opcode.parameter_1);
                            let m = self.memory.read_u8(addr);
                            let value = (m >> 1) | ((self.p.carry() as u8) << 7);
                            self.memory.write_u8(addr, value);
                            self.update_nz_flags(value);
                            self.p.set_carry((m & 0b1) != 0);
                        }
                    }
                    Mnemonic::RTI => {
                        self.pop_flags();
                        self.pop_pc();
                    }
                    Mnemonic::RTS => {
                        self.pop_pc();
                        self.pc = self.pc.wrapping_add(1);
                    }
                    Mnemonic::SBC => {
                        let m = self.read_operand(opcode.parameter_1);
                        self.do_addition(!m);
                    }
                    Mnemonic::SEC => self.p.set_carry(true),
                    Mnemonic::SED => self.p.set_decimal_mode(true),
                    Mnemonic::SEI => self.p.set_irqb_disable(true),
                    Mnemonic::SMB0 => self.smb(0),
                    Mnemonic::SMB1 => self.smb(1),
                    Mnemonic::SMB2 => self.smb(2),
                    Mnemonic::SMB3 => self.smb(3),
                    Mnemonic::SMB4 => self.smb(4),
                    Mnemonic::SMB5 => self.smb(5),
                    Mnemonic::SMB6 => self.smb(6),
                    Mnemonic::SMB7 => self.smb(7),
                    Mnemonic::STA => {
                        let addr = self.read_address(opcode.parameter_1);
                        self.memory.write_u8(addr, self.a);
                    }
                    Mnemonic::STP => return,
                    Mnemonic::STX => {
                        let addr = self.read_address(opcode.parameter_1);
                        self.memory.write_u8(addr, self.x);
                    }
                    Mnemonic::STY => {
                        let addr = self.read_address(opcode.parameter_1);
                        self.memory.write_u8(addr, self.y);
                    }
                    Mnemonic::STZ => {
                        let addr = self.read_address(opcode.parameter_1);
                        self.memory.write_u8(addr, 0);
                    }
                    Mnemonic::TAX => self.set_x(self.a),
                    Mnemonic::TAY => self.set_y(self.a),
                    Mnemonic::TRB => {
                        let addr = self.read_address(opcode.parameter_1);
                        let a = self.a;
                        let m = self.memory.read_u8(addr);
                        self.memory.write_u8(addr, m & !a);
                        self.p.set_zero((m & a) != 0);
                    }
                    Mnemonic::TSB => {
                        let addr = self.read_address(opcode.parameter_1);
                        let a = self.a;
                        let m = self.memory.read_u8(addr);
                        self.memory.write_u8(addr, m | a);
                        self.p.set_zero((m & a) != 0);
                    }
                    Mnemonic::TSX => self.set_x(self.s),
                    Mnemonic::TXA => self.set_a(self.x),
                    Mnemonic::TXS => self.s = self.x,
                    Mnemonic::TYA => self.set_a(self.y),
                    Mnemonic::WAI => todo!("WAI"),
                }
            } else {
                // NOP
            }
        }
    }

    fn read_u8_inc_pc(&mut self) -> u8 {
        let result = self.memory.read_u8(self.pc);
        self.pc += 1;
        result
    }

    fn read_u16_inc_pc(&mut self) -> u16 {
        let result = self.memory.read_u16(self.pc);
        self.pc += 2;
        result
    }

    fn read_operand(&mut self, addressing_mode: AddressingMode) -> u8 {
        match addressing_mode {
            AddressingMode::Accumulator => self.a,
            AddressingMode::Immediate => self.read_u8_inc_pc(),
            _ => {
                let address = self.read_address(addressing_mode);
                self.memory.read_u8(address)
            }
        }
    }

    fn read_address(&mut self, addressing_mode: AddressingMode) -> u16 {
        match addressing_mode {
            AddressingMode::Absolute => self.read_u16_inc_pc(),
            AddressingMode::AbsoluteIndexedX => self.read_u16_inc_pc().wrapping_add(self.x as u16),
            AddressingMode::AbsoluteIndexedY => self.read_u16_inc_pc().wrapping_add(self.y as u16),
            AddressingMode::AbsoluteIndirect => {
                let address = self.read_u16_inc_pc();
                self.memory.read_u16(address)
            }
            AddressingMode::AbsoluteIndexedIndirectX => {
                let address = self.read_u16_inc_pc().wrapping_add(self.x as u16);
                self.memory.read_u16(address)
            }
            AddressingMode::ProgramCounterRelative => {
                let offset = self.read_u8_inc_pc() as i8;
                self.pc.wrapping_add_signed(offset as i16)
            }
            AddressingMode::ZeroPage => self.read_u8_inc_pc() as u16,
            AddressingMode::ZeroPageIndexedX => {
                (self.read_u8_inc_pc() as u16).wrapping_add(self.x as u16)
            }
            AddressingMode::ZeroPageIndexedY => {
                (self.read_u8_inc_pc() as u16).wrapping_add(self.y as u16)
            }
            AddressingMode::ZeroPageIndirect => {
                let address = self.read_u8_inc_pc() as u16;
                self.memory.read_u16(address)
            }
            AddressingMode::ZeroPageIndexedIndirectX => {
                let address = (self.read_u8_inc_pc() as u16).wrapping_add(self.x as u16);
                self.memory.read_u16(address)
            }
            AddressingMode::ZeroPageIndirectIndexedY => {
                let address = self.read_u8_inc_pc() as u16;
                self.memory.read_u16(address).wrapping_add(self.y as u16)
            }
            _ => unimplemented!(),
        }
    }

    fn update_nz_flags(&mut self, value: u8) {
        self.p.set_zero(value == 0);
        self.p.set_negative((value & 0x80) != 0);
    }

    fn set_a(&mut self, value: u8) {
        self.a = value;
        self.update_nz_flags(value);
    }

    fn set_x(&mut self, value: u8) {
        self.x = value;
        self.update_nz_flags(value);
    }

    fn set_y(&mut self, value: u8) {
        self.y = value;
        self.update_nz_flags(value);
    }

    fn push(&mut self, value: u8) {
        let addr = 0x0100 + self.s as u16;
        self.s = self.s.wrapping_sub(1);
        self.memory.write_u8(addr, value);
    }

    fn pop(&mut self) -> u8 {
        self.s = self.s.wrapping_add(1);
        let addr = 0x0100 + self.s as u16;
        self.memory.read_u8(addr)
    }

    fn push_pc(&mut self) {
        let [l, h] = self.pc.to_le_bytes();
        self.push(h);
        self.push(l);
    }

    fn pop_pc(&mut self) {
        let l = self.pop();
        let h = self.pop();
        self.pc = u16::from_le_bytes([l, h]);
    }

    fn pop_flags(&mut self) {
        // TODO: find out if I and B are set
        self.p = (self.pop() | 0x20).into();
    }

    fn push_flags(&mut self) {
        self.push(self.p.into());
    }

    fn do_addition(&mut self, m: u8) {
        let a = self.a;
        let c = self.p.carry();
        let r = a.wrapping_add(m).wrapping_add(c as u8);
        self.set_a(r);
        self.p.set_carry(if c { r <= a } else { r < a });
        self.p.set_overflow(((a ^ r) & (m ^ r) & 0x80) != 0);
    }

    fn bbr(&mut self, bit: u8) {
        let m = self.read_operand(AddressingMode::ZeroPage);
        let target = self.read_address(AddressingMode::ProgramCounterRelative);
        if ((m >> bit) & 0b1) == 0 {
            self.pc = target;
        }
    }

    fn bbs(&mut self, bit: u8) {
        let m = self.read_operand(AddressingMode::ZeroPage);
        let target = self.read_address(AddressingMode::ProgramCounterRelative);
        if ((m >> bit) & 0b1) != 0 {
            self.pc = target;
        }
    }

    fn branch(&mut self, condition: bool) {
        let target = self.read_address(AddressingMode::ProgramCounterRelative);
        if condition {
            self.pc = target;
        }
    }

    fn cmp(&mut self, a: u8, addressing_mode: AddressingMode) {
        let m = self.read_operand(addressing_mode);
        self.p.set_zero(a == m);
        self.p.set_negative(a < m);
        self.p.set_carry(a >= m);
    }

    fn rmb(&mut self, bit: u8) {
        let addr = self.read_address(AddressingMode::ZeroPage);
        let m = self.memory.read_u8(addr);
        self.memory.write_u8(addr, m & !(1 << bit));
    }

    fn smb(&mut self, bit: u8) {
        let addr = self.read_address(AddressingMode::ZeroPage);
        let m = self.memory.read_u8(addr);
        self.memory.write_u8(addr, m | (1 << bit));
    }
}
