use crate::interrupt::InterruptProvider;
use crate::memory::Memory;
use crate::opcode::{AddressingMode, Opcode, get_instruction};
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
pub struct Cpu<M, I> {
    /// A register
    pub a: u8,
    /// X register
    pub x: u8,
    /// Y register
    pub y: u8,
    /// stack pointer
    pub s: u8,
    /// processor status/flags
    pub p: Status,
    /// program counter
    pub pc: u16,
    /// memory
    pub memory: M,
    /// interrupt provider
    pub interrupt_provider: I,
    /// software interrupt requested
    brk: bool,
    /// interrupt requested
    irq: bool,
    /// non-maskable interrupt requested
    nmi: bool,
    /// waiting for interrupt
    wai: bool,
}

impl<M: Memory, I: InterruptProvider> Cpu<M, I> {
    pub fn new(memory: M, interrupt_provider: I) -> Self {
        let mut cpu = Self {
            a: 0,
            x: 0,
            y: 0,
            s: 0xFD,
            p: Status::default(),
            pc: 0,
            memory,
            interrupt_provider,
            brk: false,
            irq: false,
            nmi: false,
            wai: false,
        };
        cpu.pc = cpu.memory.read_u16(0xFFFC);
        cpu
    }

    pub fn run(&mut self) {
        loop {
            if self.interrupt_provider.consume_irq() {
                self.irq = true;
            }
            if self.interrupt_provider.consume_nmi() {
                self.nmi = true;
            }

            if self.brk {
                self.brk = false;
                assert!(!self.wai);
                self.push_pc();
                self.p.set_brk_command(true);
                self.push_flags();
                self.p.set_irqb_disable(true);
                self.p.set_decimal_mode(false);
                self.pc = self.memory.read_u16(0xFFFE);
            }
            if self.irq {
                self.irq = false;
                self.wai = false;
                if !self.p.irqb_disable() {
                    self.push_pc();
                    self.p.set_brk_command(false);
                    self.push_flags();
                    self.p.set_irqb_disable(true);
                    self.p.set_decimal_mode(false);
                    self.pc = self.memory.read_u16(0xFFFE);
                }
            }
            if self.nmi {
                self.nmi = false;
                self.wai = false;
                self.push_pc();
                self.p.set_brk_command(false);
                self.push_flags();
                self.p.set_irqb_disable(true);
                self.p.set_decimal_mode(false);
                self.pc = self.memory.read_u16(0xFFFA);
            }
            if self.wai {
                continue; // busy looping
            }

            let opcode = get_instruction(self.read_u8_inc_pc());
            if let Some(opcode) = opcode {
                match opcode.opcode {
                    Opcode::ADC => {
                        let m = self.read_operand(opcode.parameter_1);
                        self.do_addition(m);
                    }
                    Opcode::AND => {
                        let m = self.read_operand(opcode.parameter_1);
                        self.set_a(self.a & m);
                    }
                    Opcode::ASL => {
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
                    Opcode::BBR0 => self.bbr(0),
                    Opcode::BBR1 => self.bbr(1),
                    Opcode::BBR2 => self.bbr(2),
                    Opcode::BBR3 => self.bbr(3),
                    Opcode::BBR4 => self.bbr(4),
                    Opcode::BBR6 => self.bbr(5),
                    Opcode::BBR5 => self.bbr(6),
                    Opcode::BBR7 => self.bbr(7),
                    Opcode::BBS0 => self.bbs(0),
                    Opcode::BBS1 => self.bbs(1),
                    Opcode::BBS2 => self.bbs(2),
                    Opcode::BBS3 => self.bbs(3),
                    Opcode::BBS4 => self.bbs(4),
                    Opcode::BBS6 => self.bbs(5),
                    Opcode::BBS5 => self.bbs(6),
                    Opcode::BBS7 => self.bbs(7),
                    Opcode::BCC => self.branch(!self.p.carry()),
                    Opcode::BCS => self.branch(self.p.carry()),
                    Opcode::BEQ => self.branch(self.p.zero()),
                    Opcode::BIT => {
                        let m = self.read_operand(opcode.parameter_1);
                        self.p.set_zero((self.a & m) == 0);
                        // TODO: some sources say this does not happen with immediate operand
                        self.p.set_negative((m & 0x80) != 0);
                        self.p.set_carry((m & 0x40) != 0);
                    }
                    Opcode::BMI => self.branch(self.p.negative()),
                    Opcode::BNE => self.branch(!self.p.zero()),
                    Opcode::BPL => self.branch(!self.p.negative()),
                    Opcode::BRA => self.branch(true),
                    Opcode::BRK => {
                        self.pc = self.pc.wrapping_add(1); // skip unused 2nd instruction byte
                        self.brk = true;
                    }
                    Opcode::BVC => self.branch(!self.p.overflow()),
                    Opcode::BVS => self.branch(self.p.overflow()),
                    Opcode::CLC => self.p.set_carry(false),
                    Opcode::CLD => self.p.set_decimal_mode(false),
                    Opcode::CLI => self.p.set_irqb_disable(false),
                    Opcode::CLV => self.p.set_overflow(false),
                    Opcode::CMP => self.cmp(self.a, opcode.parameter_1),
                    Opcode::CPX => self.cmp(self.x, opcode.parameter_1),
                    Opcode::CPY => self.cmp(self.y, opcode.parameter_1),
                    Opcode::DEC => {
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
                    Opcode::DEX => self.set_x(self.x.wrapping_sub(1)),
                    Opcode::DEY => self.set_y(self.y.wrapping_sub(1)),
                    Opcode::EOR => {
                        let m = self.read_operand(opcode.parameter_1);
                        self.set_a(self.a ^ m);
                    }
                    Opcode::INC => {
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
                    Opcode::INX => self.set_x(self.x.wrapping_add(1)),
                    Opcode::INY => self.set_y(self.y.wrapping_add(1)),
                    Opcode::JMP => {
                        let target = self.read_address(opcode.parameter_1);
                        self.pc = target;
                    }
                    Opcode::JSR => {
                        let target = self.read_address(opcode.parameter_1);
                        self.pc = self.pc.wrapping_sub(1);
                        self.push_pc();
                        self.pc = target;
                    }
                    Opcode::LDA => {
                        let op = self.read_operand(opcode.parameter_1);
                        self.set_a(op);
                    }
                    Opcode::LDX => {
                        let op = self.read_operand(opcode.parameter_1);
                        self.set_x(op);
                    }
                    Opcode::LDY => {
                        let op = self.read_operand(opcode.parameter_1);
                        self.set_y(op);
                    }
                    Opcode::LSR => {
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
                    Opcode::NOP => {}
                    Opcode::ORA => {
                        let m = self.read_operand(opcode.parameter_1);
                        self.set_a(self.a | m);
                    }
                    Opcode::PHA => self.push(self.a),
                    Opcode::PHP => {
                        self.p.set_brk_command(true);
                        self.push_flags();
                    }
                    Opcode::PHX => self.push(self.x),
                    Opcode::PHY => self.push(self.y),
                    Opcode::PLA => {
                        let value = self.pop();
                        self.set_a(value);
                    }
                    Opcode::PLP => self.pop_flags(),
                    Opcode::PLX => {
                        let value = self.pop();
                        self.set_x(value);
                    }
                    Opcode::PLY => {
                        let value = self.pop();
                        self.set_y(value);
                    }
                    Opcode::RMB0 => self.rmb(0),
                    Opcode::RMB1 => self.rmb(1),
                    Opcode::RMB2 => self.rmb(2),
                    Opcode::RMB3 => self.rmb(3),
                    Opcode::RMB4 => self.rmb(4),
                    Opcode::RMB5 => self.rmb(5),
                    Opcode::RMB6 => self.rmb(6),
                    Opcode::RMB7 => self.rmb(7),
                    Opcode::ROL => {
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
                    Opcode::ROR => {
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
                    Opcode::RTI => {
                        self.pop_flags();
                        self.pop_pc();
                    }
                    Opcode::RTS => {
                        self.pop_pc();
                        self.pc = self.pc.wrapping_add(1);
                    }
                    Opcode::SBC => {
                        let m = self.read_operand(opcode.parameter_1);
                        self.do_addition(!m);
                    }
                    Opcode::SEC => self.p.set_carry(true),
                    Opcode::SED => self.p.set_decimal_mode(true),
                    Opcode::SEI => self.p.set_irqb_disable(true),
                    Opcode::SMB0 => self.smb(0),
                    Opcode::SMB1 => self.smb(1),
                    Opcode::SMB2 => self.smb(2),
                    Opcode::SMB3 => self.smb(3),
                    Opcode::SMB4 => self.smb(4),
                    Opcode::SMB5 => self.smb(5),
                    Opcode::SMB6 => self.smb(6),
                    Opcode::SMB7 => self.smb(7),
                    Opcode::STA => {
                        let addr = self.read_address(opcode.parameter_1);
                        self.memory.write_u8(addr, self.a);
                    }
                    Opcode::STP => return,
                    Opcode::STX => {
                        let addr = self.read_address(opcode.parameter_1);
                        self.memory.write_u8(addr, self.x);
                    }
                    Opcode::STY => {
                        let addr = self.read_address(opcode.parameter_1);
                        self.memory.write_u8(addr, self.y);
                    }
                    Opcode::STZ => {
                        let addr = self.read_address(opcode.parameter_1);
                        self.memory.write_u8(addr, 0);
                    }
                    Opcode::TAX => self.set_x(self.a),
                    Opcode::TAY => self.set_y(self.a),
                    Opcode::TRB => {
                        let addr = self.read_address(opcode.parameter_1);
                        let a = self.a;
                        let m = self.memory.read_u8(addr);
                        self.memory.write_u8(addr, m & !a);
                        self.p.set_zero((m & a) != 0);
                    }
                    Opcode::TSB => {
                        let addr = self.read_address(opcode.parameter_1);
                        let a = self.a;
                        let m = self.memory.read_u8(addr);
                        self.memory.write_u8(addr, m | a);
                        self.p.set_zero((m & a) != 0);
                    }
                    Opcode::TSX => self.set_x(self.s),
                    Opcode::TXA => self.set_a(self.x),
                    Opcode::TXS => self.s = self.x,
                    Opcode::TYA => self.set_a(self.y),
                    Opcode::WAI => self.wai = true,
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
        self.memory.write_u8(0x0100 + self.s as u16, value);
        self.s = self.s.wrapping_sub(1);
    }

    fn pop(&mut self) -> u8 {
        self.s = self.s.wrapping_add(1);
        self.memory.read_u8(0x0100 + self.s as u16)
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
        // always set brk and unused flags
        self.p = Status::from_bits(self.pop() | 0b0011_0000);
    }

    fn push_flags(&mut self) {
        // always set unused flag
        self.push(self.p.into_bits() | 0b0010_0000);
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
