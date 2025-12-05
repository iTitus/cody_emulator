use crate::memory::Memory;
use crate::opcode::{AddressingMode, Opcode, get_instruction};
use bitfields::bitfield;
use log::trace;

pub const INITIAL_STACK_POINTER: u8 = 0xFD;
pub const NMI_VECTOR: u16 = 0xFFFA;
pub const RESET_VECTOR: u16 = 0xFFFC;
pub const IRQ_VECTOR: u16 = 0xFFFE;

#[bitfield(u8)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct Status {
    carry: bool,
    zero: bool,
    #[bits(default = true)]
    irqb_disable: bool,
    decimal_mode: bool,
    #[bits(default = true)]
    _brk_command: bool,
    #[bits(default = true)]
    _unused: bool,
    overflow: bool,
    negative: bool,
}

#[derive(Debug, Default)]
pub struct Cpu<M> {
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
    /// memory/bus access
    pub memory: M,
    /// true if running
    run: bool,
    /// true if waiting for interrupt
    wai: bool,
    /// cycles elapsed since turning on
    cycle: usize,
}

impl<M: Memory> Cpu<M> {
    pub fn new(memory: M) -> Self {
        let mut cpu = Self {
            a: 0,
            x: 0,
            y: 0,
            s: 0,
            p: Status::default(),
            pc: 0,
            memory,
            run: false,
            wai: false,
            cycle: 0,
        };
        cpu.reset();
        cpu
    }

    pub fn reset(&mut self) {
        self.run = true;
        self.a = 0;
        self.x = 0;
        self.y = 0;
        self.s = INITIAL_STACK_POINTER;
        self.p = Status::default();
        self.pc = self.memory.read_u16(RESET_VECTOR);
        self.wai = false;
        self.cycle = 0;
    }

    pub fn run(&mut self) {
        while self.run {
            self.step_instruction();
        }
    }

    /// execute one instruction, returns the number of elapsed cycles
    pub fn step_instruction(&mut self) -> u8 {
        if !self.run {
            return 0;
        }

        let interrupt = self.memory.update(self.cycle);
        if interrupt.is_nmi() || interrupt.is_irq() {
            self.wai = false;
            if interrupt.is_nmi() || (interrupt.is_irq() && !self.p.irqb_disable()) {
                self.push_pc();
                self.push_flags_no_brk();
                self.p.set_irqb_disable(true);
                self.p.set_decimal_mode(false);
                self.pc = self.memory.read_u16(if interrupt.is_nmi() {
                    NMI_VECTOR
                } else {
                    IRQ_VECTOR
                });
            }
        }

        if !self.wai {
            let pc = self.pc;
            let opcode = get_instruction(self.read_u8_inc_pc());
            let cycles = if let Some(opcode) = opcode {
                trace!("Executing opcode 0x{pc:04X} {opcode:?}");
                let mut extra_cycles = 0;
                match opcode.opcode {
                    Opcode::ADC => {
                        let (m, page_cross) = self.read_value_operand(opcode.parameter_1);
                        if page_cross {
                            extra_cycles += 1;
                        }
                        if self.p.decimal_mode() {
                            extra_cycles += 1;
                            self.do_addition_decimal(m);
                        } else {
                            self.do_addition(m);
                        }
                    }
                    Opcode::AND => {
                        let (m, page_cross) = self.read_value_operand(opcode.parameter_1);
                        if page_cross {
                            extra_cycles += 1;
                        }
                        self.set_a(self.a & m);
                    }
                    Opcode::ASL => {
                        if opcode.parameter_1 == AddressingMode::Accumulator {
                            let m = self.a;
                            self.set_a(m << 1);
                            self.p.set_carry((m & 0x80) != 0);
                        } else {
                            let (addr, page_cross) = self.read_address_operand(opcode.parameter_1);
                            if page_cross {
                                extra_cycles += 1;
                            }
                            let m = self.memory.read_u8(addr);
                            let value = m << 1;
                            self.memory.write_u8(addr, value);
                            self.update_nz_flags(value);
                            self.p.set_carry((m & 0x80) != 0);
                        }
                    }
                    Opcode::BBR0 => extra_cycles += self.bbr(0),
                    Opcode::BBR1 => extra_cycles += self.bbr(1),
                    Opcode::BBR2 => extra_cycles += self.bbr(2),
                    Opcode::BBR3 => extra_cycles += self.bbr(3),
                    Opcode::BBR4 => extra_cycles += self.bbr(4),
                    Opcode::BBR5 => extra_cycles += self.bbr(5),
                    Opcode::BBR6 => extra_cycles += self.bbr(6),
                    Opcode::BBR7 => extra_cycles += self.bbr(7),
                    Opcode::BBS0 => extra_cycles += self.bbs(0),
                    Opcode::BBS1 => extra_cycles += self.bbs(1),
                    Opcode::BBS2 => extra_cycles += self.bbs(2),
                    Opcode::BBS3 => extra_cycles += self.bbs(3),
                    Opcode::BBS4 => extra_cycles += self.bbs(4),
                    Opcode::BBS5 => extra_cycles += self.bbs(5),
                    Opcode::BBS6 => extra_cycles += self.bbs(6),
                    Opcode::BBS7 => extra_cycles += self.bbs(7),
                    Opcode::BCC => extra_cycles += self.branch(!self.p.carry()),
                    Opcode::BCS => extra_cycles += self.branch(self.p.carry()),
                    Opcode::BEQ => extra_cycles += self.branch(self.p.zero()),
                    Opcode::BIT => {
                        let (m, page_cross) = self.read_value_operand(opcode.parameter_1);
                        if page_cross {
                            extra_cycles += 1;
                        }
                        self.p.set_zero((self.a & m) == 0);
                        if opcode.parameter_1 != AddressingMode::Immediate {
                            // N and V are only touched when not in immediate mode
                            self.p.set_negative((m & 0x80) != 0);
                            self.p.set_overflow((m & 0x40) != 0);
                        }
                    }
                    Opcode::BMI => extra_cycles += self.branch(self.p.negative()),
                    Opcode::BNE => extra_cycles += self.branch(!self.p.zero()),
                    Opcode::BPL => extra_cycles += self.branch(!self.p.negative()),
                    Opcode::BRA => extra_cycles += self.branch(true),
                    Opcode::BRK => {
                        self.pc = self.pc.wrapping_add(1); // skip unused 2nd instruction byte
                        // BRK logic
                        self.push_pc();
                        self.push_flags();
                        self.p.set_irqb_disable(true);
                        self.p.set_decimal_mode(false);
                        self.pc = self.memory.read_u16(IRQ_VECTOR);
                    }
                    Opcode::BVC => extra_cycles += self.branch(!self.p.overflow()),
                    Opcode::BVS => extra_cycles += self.branch(self.p.overflow()),
                    Opcode::CLC => self.p.set_carry(false),
                    Opcode::CLD => self.p.set_decimal_mode(false),
                    Opcode::CLI => self.p.set_irqb_disable(false),
                    Opcode::CLV => self.p.set_overflow(false),
                    Opcode::CMP => {
                        let page_cross = self.cmp(self.a, opcode.parameter_1);
                        if page_cross {
                            extra_cycles += 1;
                        }
                    }
                    Opcode::CPX => {
                        let page_cross = self.cmp(self.x, opcode.parameter_1);
                        if page_cross {
                            extra_cycles += 1;
                        }
                    }
                    Opcode::CPY => {
                        let page_cross = self.cmp(self.y, opcode.parameter_1);
                        if page_cross {
                            extra_cycles += 1;
                        }
                    }
                    Opcode::DEC => {
                        if opcode.parameter_1 == AddressingMode::Accumulator {
                            self.set_a(self.a.wrapping_sub(1));
                        } else {
                            let (addr, page_cross) = self.read_address_operand(opcode.parameter_1);
                            if page_cross && opcode.parameter_1 != AddressingMode::AbsoluteIndexedX
                            {
                                extra_cycles += 1;
                            }
                            let value = self.memory.read_u8(addr);
                            let new_value = value.wrapping_sub(1);
                            self.memory.write_u8(addr, new_value);
                            self.update_nz_flags(new_value);
                        }
                    }
                    Opcode::DEX => self.set_x(self.x.wrapping_sub(1)),
                    Opcode::DEY => self.set_y(self.y.wrapping_sub(1)),
                    Opcode::EOR => {
                        let (m, page_cross) = self.read_value_operand(opcode.parameter_1);
                        if page_cross {
                            extra_cycles += 1;
                        }
                        self.set_a(self.a ^ m);
                    }
                    Opcode::INC => {
                        if opcode.parameter_1 == AddressingMode::Accumulator {
                            self.set_a(self.a.wrapping_add(1));
                        } else {
                            let (addr, page_cross) = self.read_address_operand(opcode.parameter_1);
                            if page_cross && opcode.parameter_1 != AddressingMode::AbsoluteIndexedX
                            {
                                extra_cycles += 1;
                            }
                            let value = self.memory.read_u8(addr);
                            let new_value = value.wrapping_add(1);
                            self.memory.write_u8(addr, new_value);
                            self.update_nz_flags(new_value);
                        }
                    }
                    Opcode::INX => self.set_x(self.x.wrapping_add(1)),
                    Opcode::INY => self.set_y(self.y.wrapping_add(1)),
                    Opcode::JMP => {
                        let (target, page_cross) = self.read_address_operand(opcode.parameter_1);
                        if page_cross {
                            extra_cycles += 1;
                        }
                        self.pc = target;
                    }
                    Opcode::JSR => {
                        let (target, page_cross) = self.read_address_operand(opcode.parameter_1);
                        if page_cross {
                            extra_cycles += 1;
                        }
                        self.pc = self.pc.wrapping_sub(1);
                        self.push_pc();
                        self.pc = target;
                    }
                    Opcode::LDA => {
                        let (op, page_cross) = self.read_value_operand(opcode.parameter_1);
                        if page_cross {
                            extra_cycles += 1;
                        }
                        self.set_a(op);
                    }
                    Opcode::LDX => {
                        let (op, page_cross) = self.read_value_operand(opcode.parameter_1);
                        if page_cross {
                            extra_cycles += 1;
                        }
                        self.set_x(op);
                    }
                    Opcode::LDY => {
                        let (op, page_cross) = self.read_value_operand(opcode.parameter_1);
                        if page_cross {
                            extra_cycles += 1;
                        }
                        self.set_y(op);
                    }
                    Opcode::LSR => {
                        if opcode.parameter_1 == AddressingMode::Accumulator {
                            let m = self.a;
                            self.set_a(m >> 1);
                            self.p.set_carry((m & 0b1) != 0);
                        } else {
                            let (addr, page_cross) = self.read_address_operand(opcode.parameter_1);
                            if page_cross {
                                extra_cycles += 1;
                            }
                            let m = self.memory.read_u8(addr);
                            let value = m >> 1;
                            self.memory.write_u8(addr, value);
                            self.update_nz_flags(value);
                            self.p.set_carry((m & 0b1) != 0);
                        }
                    }
                    Opcode::NOP => {}
                    Opcode::ORA => {
                        let (m, page_cross) = self.read_value_operand(opcode.parameter_1);
                        if page_cross {
                            extra_cycles += 1;
                        }
                        self.set_a(self.a | m);
                    }
                    Opcode::PHA => self.push(self.a),
                    Opcode::PHP => {
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
                            let (addr, page_cross) = self.read_address_operand(opcode.parameter_1);
                            if page_cross {
                                extra_cycles += 1;
                            }
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
                            let (addr, page_cross) = self.read_address_operand(opcode.parameter_1);
                            if page_cross {
                                extra_cycles += 1;
                            }
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
                        let (m, page_cross) = self.read_value_operand(opcode.parameter_1);
                        if page_cross {
                            extra_cycles += 1;
                        }
                        if self.p.decimal_mode() {
                            extra_cycles += 1;
                            self.do_subtraction_decimal(m);
                        } else {
                            self.do_subtraction(m);
                        }
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
                        let (addr, _) = self.read_address_operand(opcode.parameter_1);
                        self.memory.write_u8(addr, self.a);
                    }
                    Opcode::STP => self.run = false,
                    Opcode::STX => {
                        let (addr, _) = self.read_address_operand(opcode.parameter_1);
                        self.memory.write_u8(addr, self.x);
                    }
                    Opcode::STY => {
                        let (addr, _) = self.read_address_operand(opcode.parameter_1);
                        self.memory.write_u8(addr, self.y);
                    }
                    Opcode::STZ => {
                        let (addr, _) = self.read_address_operand(opcode.parameter_1);
                        self.memory.write_u8(addr, 0);
                    }
                    Opcode::TAX => self.set_x(self.a),
                    Opcode::TAY => self.set_y(self.a),
                    Opcode::TRB => {
                        let (addr, page_cross) = self.read_address_operand(opcode.parameter_1);
                        if page_cross {
                            extra_cycles += 1;
                        }
                        let a = self.a;
                        let m = self.memory.read_u8(addr);
                        self.memory.write_u8(addr, m & !a);
                        self.p.set_zero((m & a) == 0);
                    }
                    Opcode::TSB => {
                        let (addr, page_cross) = self.read_address_operand(opcode.parameter_1);
                        if page_cross {
                            extra_cycles += 1;
                        }
                        let a = self.a;
                        let m = self.memory.read_u8(addr);
                        self.memory.write_u8(addr, m | a);
                        self.p.set_zero((m & a) == 0);
                    }
                    Opcode::TSX => self.set_x(self.s),
                    Opcode::TXA => self.set_a(self.x),
                    Opcode::TXS => self.s = self.x,
                    Opcode::TYA => self.set_a(self.y),
                    Opcode::WAI => self.wai = true,
                }

                opcode.cycles + extra_cycles
            } else {
                // TODO: implement undocumented opcodes with correct cycle count
                1
            };

            self.cycle = self.cycle.wrapping_add(cycles as usize);
            return cycles;
        }

        // cycles for WAI check
        // TODO: find exact value
        1
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

    /// return value and if a page boundary was crossed
    fn read_value_operand(&mut self, addressing_mode: AddressingMode) -> (u8, bool) {
        match addressing_mode {
            AddressingMode::Accumulator => (self.a, false),
            AddressingMode::Immediate => (self.read_u8_inc_pc(), false),
            _ => {
                let (address, page_cross) = self.read_address_operand(addressing_mode);
                (self.memory.read_u8(address), page_cross)
            }
        }
    }

    /// return address and if a page boundary was crossed
    fn read_address_operand(&mut self, addressing_mode: AddressingMode) -> (u16, bool) {
        match addressing_mode {
            AddressingMode::Absolute => (self.read_u16_inc_pc(), false),
            AddressingMode::AbsoluteIndexedX => {
                let base = self.read_u16_inc_pc();
                let address = base.wrapping_add(self.x as u16);
                if base >> 8 != address >> 8 {
                    (address, true)
                } else {
                    (address, false)
                }
            }
            AddressingMode::AbsoluteIndexedY => {
                let base = self.read_u16_inc_pc();
                let address = base.wrapping_add(self.y as u16);
                if base >> 8 != address >> 8 {
                    (address, true)
                } else {
                    (address, false)
                }
            }
            AddressingMode::AbsoluteIndirect => {
                let address = self.read_u16_inc_pc();
                (self.memory.read_u16(address), false)
            }
            AddressingMode::AbsoluteIndexedIndirectX => {
                let address = self.read_u16_inc_pc().wrapping_add(self.x as u16);
                (self.memory.read_u16(address), false)
            }
            AddressingMode::ProgramCounterRelative => {
                let offset = self.read_u8_inc_pc() as i8;
                (self.pc.wrapping_add_signed(offset as i16), false)
            }
            AddressingMode::ZeroPage => (self.read_u8_inc_pc() as u16, false),
            AddressingMode::ZeroPageIndexedX => {
                (self.read_u8_inc_pc().wrapping_add(self.x) as u16, false)
            }
            AddressingMode::ZeroPageIndexedY => {
                (self.read_u8_inc_pc().wrapping_add(self.y) as u16, false)
            }
            AddressingMode::ZeroPageIndirect => {
                let address = self.read_u8_inc_pc();
                (self.memory.read_u16_zp(address), false)
            }
            AddressingMode::ZeroPageIndexedIndirectX => {
                let address = self.read_u8_inc_pc().wrapping_add(self.x);
                (self.memory.read_u16_zp(address), false)
            }
            AddressingMode::ZeroPageIndirectIndexedY => {
                let address = self.read_u8_inc_pc();
                let base = self.memory.read_u16_zp(address);
                let address = base.wrapping_add(self.y as u16);
                if base >> 8 != address >> 8 {
                    (address, true)
                } else {
                    (address, false)
                }
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
        // always set unused flag and brk flag
        self.push(self.p.into_bits() | 0b0011_0000);
    }

    fn push_flags_no_brk(&mut self) {
        // always set unused flag
        self.push(self.p.into_bits() | 0b0010_0000);
    }

    fn do_addition(&mut self, m: u8) {
        let a = self.a;
        let cf = self.p.carry();
        let c = cf as u8;
        let r = a.wrapping_add(m).wrapping_add(c);
        self.set_a(r);
        self.p.set_carry(if cf { r <= a } else { r < a });
        self.p.set_overflow(((a ^ r) & (m ^ r) & 0x80) != 0);
    }

    fn do_addition_decimal(&mut self, m: u8) {
        let a = self.a;
        let cf = self.p.carry();

        let mut c = cf as u8;
        let mut lo = (a & 0xf).wrapping_add(m & 0xf).wrapping_add(c);
        c = (lo > 9) as u8;
        if c != 0 {
            lo = (lo - 10) & 0xf;
        }

        let mut hi = (a >> 4).wrapping_add(m >> 4).wrapping_add(c);
        let overflow_flag_pre = (hi & 0x8) << 4;
        c = (hi > 9) as u8;
        if c != 0 {
            hi = (hi - 10) & 0xf;
        }

        let r = lo | (hi << 4);
        self.set_a(r);
        self.p.set_carry(c != 0);
        self.p
            .set_overflow(((a ^ overflow_flag_pre) & (m ^ overflow_flag_pre) & 0x80) != 0);
    }

    fn do_subtraction(&mut self, m: u8) {
        self.do_addition(!m);
    }

    fn do_subtraction_decimal(&mut self, m: u8) {
        let a = self.a;
        let cf = self.p.carry();

        let mut b = !cf as u8;
        let mut lo = (a & 0xf).wrapping_sub(m & 0xf).wrapping_sub(b);
        b = ((lo & 0x80) != 0) as u8;
        if b != 0 {
            lo = lo.wrapping_add(10);
        }
        let negative_lo = (lo & 0x80) != 0;
        lo &= 0xf;

        let mut hi = (a >> 4).wrapping_sub(m >> 4).wrapping_sub(b);
        let overflow_flag_pre = (hi & 0x8) << 4;
        b = ((hi & 0x80) != 0) as u8;
        if b != 0 {
            hi = hi.wrapping_add(10);
        }
        hi = hi.wrapping_sub(negative_lo as u8);
        hi &= 0xf;

        let r = lo | (hi << 4);
        self.set_a(r);
        self.p.set_carry(b == 0);
        self.p
            .set_overflow(((a ^ overflow_flag_pre) & (!m ^ overflow_flag_pre) & 0x80) != 0);
    }

    fn bbr(&mut self, bit: u8) -> u8 {
        let (m, _) = self.read_value_operand(AddressingMode::ZeroPage);
        let (target, _) = self.read_address_operand(AddressingMode::ProgramCounterRelative);
        if ((m >> bit) & 0b1) == 0 {
            let pc = self.pc;
            self.pc = target;
            if pc >> 8 != target >> 8 { 2 } else { 1 }
        } else {
            0
        }
    }

    fn bbs(&mut self, bit: u8) -> u8 {
        let (m, _) = self.read_value_operand(AddressingMode::ZeroPage);
        let (target, _) = self.read_address_operand(AddressingMode::ProgramCounterRelative);
        if ((m >> bit) & 0b1) != 0 {
            let pc = self.pc;
            self.pc = target;
            if pc >> 8 != target >> 8 { 2 } else { 1 }
        } else {
            0
        }
    }

    fn branch(&mut self, condition: bool) -> u8 {
        let (target, _) = self.read_address_operand(AddressingMode::ProgramCounterRelative);
        if condition {
            let pc = self.pc;
            self.pc = target;
            if pc >> 8 != target >> 8 { 2 } else { 1 }
        } else {
            0
        }
    }

    fn cmp(&mut self, a: u8, addressing_mode: AddressingMode) -> bool {
        let (m, page_cross) = self.read_value_operand(addressing_mode);
        self.update_nz_flags(a.wrapping_sub(m));
        self.p.set_carry(a >= m);
        page_cross
    }

    fn rmb(&mut self, bit: u8) {
        let (addr, _) = self.read_address_operand(AddressingMode::ZeroPage);
        let m = self.memory.read_u8(addr);
        self.memory.write_u8(addr, m & !(1 << bit));
    }

    fn smb(&mut self, bit: u8) {
        let (addr, _) = self.read_address_operand(AddressingMode::ZeroPage);
        let m = self.memory.read_u8(addr);
        self.memory.write_u8(addr, m | (1 << bit));
    }
}
