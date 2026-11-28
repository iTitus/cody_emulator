use std::{
    collections::HashSet,
    error::Error,
    net::{TcpListener, TcpStream},
    num::NonZeroUsize,
};

use log::info;

use crate::{
    cpu::{Cpu, Status},
    memory::Memory,
};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum NextCpuAction {
    Pause,
    Resume,
    Step,
    Stop,
}

#[derive(Debug)]
pub struct Debugger {
    tx: std::sync::mpsc::Sender<CpuToDebuggerMessage>,
    rx: std::sync::mpsc::Receiver<DebuggerToCpuMessage>,
    pub next_action: NextCpuAction,
    pub breakpoints: HashSet<u16>,
}

#[derive(Debug, Clone)]
pub enum DebuggerToCpuMessage {
    ReadRegisters,
    WriteRegisters(Registers),
    ReadMemory { start_address: u16, amount: usize },
    WriteMemory { start_address: u16, data: Box<[u8]> },
    SetNextAction(NextCpuAction),
    AddBreakpoint(u16),
    RemoveBreakpoint(u16),
}

#[derive(Debug, Clone)]
pub enum CpuToDebuggerMessage {
    Registers(Registers),
    Memory(Box<[u8]>),
    Stopped,
    Stepped,
    BreakpointHit,
}

pub fn start_server(port: u16) -> Result<Debugger, Box<dyn Error>> {
    let (tx1, rx1) = std::sync::mpsc::channel::<CpuToDebuggerMessage>();
    let (tx2, rx2) = std::sync::mpsc::channel::<DebuggerToCpuMessage>();
    let debugger = Debugger {
        tx: tx1,
        rx: rx2,
        next_action: NextCpuAction::Pause,
        breakpoints: HashSet::default(),
    };
    let mut target = Target { tx: tx2, rx: rx1 };
    std::thread::spawn(move || {
        while let Ok(conn) = wait_for_tcp(port) {
            let connection: Box<dyn gdbstub::conn::ConnectionExt<Error = std::io::Error>> =
                Box::new(conn);
            let gdb = gdbstub::stub::GdbStub::new(connection);
            let result = gdb.run_blocking::<GdbEventLoop>(&mut target);
            info!("debugger disconnected: {result:?}");
        }
    });

    Ok(debugger)
}

fn wait_for_tcp(port: u16) -> Result<TcpStream, Box<dyn Error>> {
    let sock = TcpListener::bind(("0.0.0.0", port))?;
    info!("Waiting for a GDB connection on {}...", sock.local_addr()?);

    let (stream, addr) = sock.accept()?;
    info!("Debugger connected from {addr}");

    Ok(stream)
}

pub fn handle_debugger<M: Memory>(cpu: &mut Cpu<M>) -> NextCpuAction {
    if !cpu.run {
        // TODO: what do to when cpu is stopped because of STP?
        return NextCpuAction::Stop;
    }

    if let Some(debugger) = &mut cpu.debugger {
        match debugger.next_action {
            NextCpuAction::Resume if debugger.breakpoints.contains(&cpu.pc) => {
                let _ = debugger.tx.send(CpuToDebuggerMessage::BreakpointHit);
                debugger.next_action = NextCpuAction::Pause;
            }
            NextCpuAction::Step => {
                let _ = debugger.tx.send(CpuToDebuggerMessage::Stepped);
                debugger.next_action = NextCpuAction::Pause;
            }
            _ => {}
        }

        while let Ok(m) = debugger.rx.try_recv() {
            match m {
                DebuggerToCpuMessage::ReadRegisters => {
                    let _ = debugger.tx.send(CpuToDebuggerMessage::Registers(Registers {
                        a: cpu.a,
                        x: cpu.x,
                        y: cpu.y,
                        s: cpu.s,
                        p: cpu.p.into_bits() | 0b0010_0000,
                        pc: cpu.pc,
                    }));
                }
                DebuggerToCpuMessage::WriteRegisters(registers) => {
                    cpu.a = registers.a;
                    cpu.x = registers.x;
                    cpu.y = registers.y;
                    cpu.s = registers.s;
                    cpu.p = Status::from_bits(registers.p | 0b0010_0000);
                    cpu.pc = registers.pc;
                }
                DebuggerToCpuMessage::ReadMemory {
                    start_address,
                    amount,
                } => {
                    let mut data = vec![];
                    for n in 0..amount {
                        let Some(target) =
                            n.try_into().ok().and_then(|n| start_address.checked_add(n))
                        else {
                            break;
                        };
                        data.push(cpu.memory.force_read_u8(target));
                    }
                    let _ = debugger
                        .tx
                        .send(CpuToDebuggerMessage::Memory(data.into_boxed_slice()));
                }
                DebuggerToCpuMessage::WriteMemory {
                    start_address,
                    data,
                } => {
                    for (n, &value) in data.iter().enumerate() {
                        let Some(target) =
                            n.try_into().ok().and_then(|n| start_address.checked_add(n))
                        else {
                            break;
                        };
                        cpu.memory.force_write_u8(target, value);
                    }
                }
                DebuggerToCpuMessage::SetNextAction(next_action) => {
                    debugger.next_action = next_action;
                    if next_action == NextCpuAction::Stop {
                        let _ = debugger.tx.send(CpuToDebuggerMessage::Stopped);
                    }
                }
                DebuggerToCpuMessage::AddBreakpoint(address) => {
                    debugger.breakpoints.insert(address);
                }
                DebuggerToCpuMessage::RemoveBreakpoint(address) => {
                    debugger.breakpoints.remove(&address);
                }
            }
        }

        debugger.next_action
    } else {
        NextCpuAction::Resume
    }
}

enum GdbEventLoop {}

impl gdbstub::stub::run_blocking::BlockingEventLoop for GdbEventLoop {
    type Target = Target;
    type Connection = Box<dyn gdbstub::conn::ConnectionExt<Error = std::io::Error>>;
    type StopReason = gdbstub::stub::SingleThreadStopReason<u16>;

    fn wait_for_stop_reason(
        target: &mut Self::Target,
        conn: &mut Self::Connection,
    ) -> Result<
        gdbstub::stub::run_blocking::Event<Self::StopReason>,
        gdbstub::stub::run_blocking::WaitForStopReasonError<
            <Self::Target as gdbstub::target::Target>::Error,
            <Self::Connection as gdbstub::conn::Connection>::Error,
        >,
    > {
        loop {
            match target.rx.try_recv() {
                Ok(message) => match message {
                    CpuToDebuggerMessage::Stopped => {
                        return Ok(gdbstub::stub::run_blocking::Event::TargetStopped(
                            gdbstub::stub::SingleThreadStopReason::Terminated(
                                gdbstub::common::Signal::SIGSTOP,
                            ),
                        ));
                    }
                    CpuToDebuggerMessage::Stepped => {
                        return Ok(gdbstub::stub::run_blocking::Event::TargetStopped(
                            gdbstub::stub::SingleThreadStopReason::DoneStep,
                        ));
                    }
                    CpuToDebuggerMessage::BreakpointHit => {
                        return Ok(gdbstub::stub::run_blocking::Event::TargetStopped(
                            gdbstub::stub::SingleThreadStopReason::SwBreak(()),
                        ));
                    }
                    _ => {
                        return Err(gdbstub::stub::run_blocking::WaitForStopReasonError::Target(
                            FatalError::UnexpectedMessage,
                        ));
                    }
                },
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Err(gdbstub::stub::run_blocking::WaitForStopReasonError::Target(
                        FatalError::ConnectionLost,
                    ));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
            if conn
                .peek()
                .map_err(gdbstub::stub::run_blocking::WaitForStopReasonError::Connection)?
                .is_some()
            {
                let byte = conn
                    .read()
                    .map_err(gdbstub::stub::run_blocking::WaitForStopReasonError::Connection)?;
                return Ok(gdbstub::stub::run_blocking::Event::IncomingData(byte));
            }
            // std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    fn on_interrupt(
        target: &mut Self::Target,
    ) -> Result<Option<Self::StopReason>, <Self::Target as gdbstub::target::Target>::Error> {
        target
            .tx
            .send(DebuggerToCpuMessage::SetNextAction(NextCpuAction::Stop))
            .map_err(|_| FatalError::ConnectionLost)?;
        let CpuToDebuggerMessage::Stopped =
            target.rx.recv().map_err(|_| FatalError::ConnectionLost)?
        else {
            return Err(FatalError::UnexpectedMessage);
        };
        Ok(Some(gdbstub::stub::BaseStopReason::Signal(
            gdbstub::common::Signal::SIGINT,
        )))
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum FatalError {
    ConnectionLost,
    UnexpectedMessage,
    UnsupportedOperation,
}

pub struct Target {
    tx: std::sync::mpsc::Sender<DebuggerToCpuMessage>,
    rx: std::sync::mpsc::Receiver<CpuToDebuggerMessage>,
}

impl gdbstub::target::Target for Target {
    type Arch = Arch65C02;
    type Error = FatalError;

    #[inline(always)]
    fn base_ops(&mut self) -> gdbstub::target::ext::base::BaseOps<'_, Self::Arch, Self::Error> {
        gdbstub::target::ext::base::BaseOps::SingleThread(self)
    }

    #[inline(always)]
    fn support_breakpoints(
        &mut self,
    ) -> Option<gdbstub::target::ext::breakpoints::BreakpointsOps<'_, Self>> {
        Some(self)
    }
}

impl gdbstub::target::ext::base::singlethread::SingleThreadBase for Target {
    fn read_registers(
        &mut self,
        regs: &mut <Self::Arch as gdbstub::arch::Arch>::Registers,
    ) -> gdbstub::target::TargetResult<(), Self> {
        self.tx
            .send(DebuggerToCpuMessage::ReadRegisters)
            .map_err(|_| gdbstub::target::TargetError::Fatal(FatalError::ConnectionLost))?;
        let CpuToDebuggerMessage::Registers(cpu_regs) = self
            .rx
            .recv()
            .map_err(|_| gdbstub::target::TargetError::Fatal(FatalError::ConnectionLost))?
        else {
            return Err(gdbstub::target::TargetError::Fatal(
                FatalError::UnexpectedMessage,
            ));
        };
        regs.clone_from(&cpu_regs);
        Ok(())
    }

    fn write_registers(
        &mut self,
        regs: &<Self::Arch as gdbstub::arch::Arch>::Registers,
    ) -> gdbstub::target::TargetResult<(), Self> {
        self.tx
            .send(DebuggerToCpuMessage::WriteRegisters(regs.clone()))
            .map_err(|_| gdbstub::target::TargetError::Fatal(FatalError::ConnectionLost))?;
        Ok(())
    }

    fn read_addrs(
        &mut self,
        start_addr: <Self::Arch as gdbstub::arch::Arch>::Usize,
        data: &mut [u8],
    ) -> gdbstub::target::TargetResult<usize, Self> {
        self.tx
            .send(DebuggerToCpuMessage::ReadMemory {
                start_address: start_addr,
                amount: data.len(),
            })
            .map_err(|_| gdbstub::target::TargetError::Fatal(FatalError::ConnectionLost))?;
        let CpuToDebuggerMessage::Memory(cpu_data) = self
            .rx
            .recv()
            .map_err(|_| gdbstub::target::TargetError::Fatal(FatalError::ConnectionLost))?
        else {
            return Err(gdbstub::target::TargetError::Fatal(
                FatalError::UnexpectedMessage,
            ));
        };
        data[..cpu_data.len()].copy_from_slice(&cpu_data);
        Ok(cpu_data.len())
    }

    fn write_addrs(
        &mut self,
        start_addr: <Self::Arch as gdbstub::arch::Arch>::Usize,
        data: &[u8],
    ) -> gdbstub::target::TargetResult<(), Self> {
        self.tx
            .send(DebuggerToCpuMessage::WriteMemory {
                start_address: start_addr,
                data: data.into(),
            })
            .map_err(|_| gdbstub::target::TargetError::Fatal(FatalError::ConnectionLost))?;
        Ok(())
    }

    #[inline(always)]
    fn support_resume(
        &mut self,
    ) -> Option<gdbstub::target::ext::base::singlethread::SingleThreadResumeOps<'_, Self>> {
        Some(self)
    }
}

impl gdbstub::target::ext::base::singlethread::SingleThreadResume for Target {
    fn resume(&mut self, signal: Option<gdbstub::common::Signal>) -> Result<(), Self::Error> {
        if signal.is_some() {
            return Err(FatalError::UnsupportedOperation);
        }
        self.tx
            .send(DebuggerToCpuMessage::SetNextAction(NextCpuAction::Resume))
            .map_err(|_| FatalError::ConnectionLost)?;
        Ok(())
    }

    #[inline(always)]
    fn support_single_step(
        &mut self,
    ) -> Option<gdbstub::target::ext::base::singlethread::SingleThreadSingleStepOps<'_, Self>> {
        Some(self)
    }
}

impl gdbstub::target::ext::base::singlethread::SingleThreadSingleStep for Target {
    fn step(&mut self, signal: Option<gdbstub::common::Signal>) -> Result<(), Self::Error> {
        if signal.is_some() {
            return Err(FatalError::UnsupportedOperation);
        }
        self.tx
            .send(DebuggerToCpuMessage::SetNextAction(NextCpuAction::Step))
            .map_err(|_| FatalError::ConnectionLost)?;
        Ok(())
    }
}

impl gdbstub::target::ext::breakpoints::Breakpoints for Target {
    #[inline(always)]
    fn support_sw_breakpoint(
        &mut self,
    ) -> Option<gdbstub::target::ext::breakpoints::SwBreakpointOps<'_, Self>> {
        Some(self)
    }
}

impl gdbstub::target::ext::breakpoints::SwBreakpoint for Target {
    fn add_sw_breakpoint(
        &mut self,
        addr: <Self::Arch as gdbstub::arch::Arch>::Usize,
        _kind: <Self::Arch as gdbstub::arch::Arch>::BreakpointKind,
    ) -> gdbstub::target::TargetResult<bool, Self> {
        self.tx
            .send(DebuggerToCpuMessage::AddBreakpoint(addr))
            .map_err(|_| gdbstub::target::TargetError::Fatal(FatalError::ConnectionLost))?;
        Ok(true)
    }

    fn remove_sw_breakpoint(
        &mut self,
        addr: <Self::Arch as gdbstub::arch::Arch>::Usize,
        _kind: <Self::Arch as gdbstub::arch::Arch>::BreakpointKind,
    ) -> gdbstub::target::TargetResult<bool, Self> {
        self.tx
            .send(DebuggerToCpuMessage::RemoveBreakpoint(addr))
            .map_err(|_| gdbstub::target::TargetError::Fatal(FatalError::ConnectionLost))?;
        Ok(true)
    }
}

pub enum Arch65C02 {}

impl gdbstub::arch::Arch for Arch65C02 {
    type Usize = u16;
    type Registers = Registers;
    type BreakpointKind = ();
    type RegId = RegId;

    fn target_description_xml() -> Option<&'static str> {
        Some(
            r#"<?xml version="1.0"?>
<!DOCTYPE target SYSTEM "gdb-target.dtd">
<target version="1.0">
    <architecture>65c02</architecture>
    <feature name="org.gnu.gdb.65c02.cpu">
        <flags id="status_type" size="1">
            <field name="C" start="0" end="0" />
            <field name="Z" start="1" end="1" />
            <field name="I" start="2" end="2" />
            <field name="D" start="3" end="3" />
            <field name="B" start="4" end="4" />
            <field name="V" start="6" end="6" />
            <field name="N" start="7" end="7" />
        </flags>
        <reg name="A" bitsize="8" group="general" />
        <reg name="X" bitsize="8" group="index" />
        <reg name="Y" bitsize="8" group="index" />
        <reg name="S" bitsize="8" group="control" type="data_ptr" />
        <reg name="P" bitsize="8" group="control" type="status_type" />
        <reg name="PC" bitsize="16" group="control" type="code_ptr" />
    </feature>
</target>"#,
        )
    }

    fn lldb_register_info(reg_id: usize) -> Option<gdbstub::arch::lldb::RegisterInfo<'static>> {
        match reg_id {
            0 => Some(gdbstub::arch::lldb::RegisterInfo::Register(
                gdbstub::arch::lldb::Register {
                    name: "A",
                    alt_name: None,
                    bitsize: 8,
                    offset: 0,
                    encoding: gdbstub::arch::lldb::Encoding::Uint,
                    format: gdbstub::arch::lldb::Format::Hex,
                    set: "general",
                    gcc: None,
                    dwarf: None,
                    generic: Some(gdbstub::arch::lldb::Generic::Arg1),
                    container_regs: None,
                    invalidate_regs: None,
                },
            )),
            1 => Some(gdbstub::arch::lldb::RegisterInfo::Register(
                gdbstub::arch::lldb::Register {
                    name: "X",
                    alt_name: None,
                    bitsize: 8,
                    offset: 1,
                    encoding: gdbstub::arch::lldb::Encoding::Uint,
                    format: gdbstub::arch::lldb::Format::Hex,
                    set: "general",
                    gcc: None,
                    dwarf: None,
                    generic: Some(gdbstub::arch::lldb::Generic::Arg2),
                    container_regs: None,
                    invalidate_regs: None,
                },
            )),
            2 => Some(gdbstub::arch::lldb::RegisterInfo::Register(
                gdbstub::arch::lldb::Register {
                    name: "Y",
                    alt_name: None,
                    bitsize: 8,
                    offset: 2,
                    encoding: gdbstub::arch::lldb::Encoding::Uint,
                    format: gdbstub::arch::lldb::Format::Hex,
                    set: "general",
                    gcc: None,
                    dwarf: None,
                    generic: Some(gdbstub::arch::lldb::Generic::Arg3),
                    container_regs: None,
                    invalidate_regs: None,
                },
            )),
            3 => Some(gdbstub::arch::lldb::RegisterInfo::Register(
                gdbstub::arch::lldb::Register {
                    name: "S",
                    alt_name: None,
                    bitsize: 8,
                    offset: 3,
                    encoding: gdbstub::arch::lldb::Encoding::Uint,
                    format: gdbstub::arch::lldb::Format::Hex,
                    set: "general",
                    gcc: None,
                    dwarf: None,
                    generic: Some(gdbstub::arch::lldb::Generic::Sp),
                    container_regs: None,
                    invalidate_regs: None,
                },
            )),
            4 => Some(gdbstub::arch::lldb::RegisterInfo::Register(
                gdbstub::arch::lldb::Register {
                    name: "P",
                    alt_name: None,
                    bitsize: 8,
                    offset: 4,
                    encoding: gdbstub::arch::lldb::Encoding::Uint,
                    format: gdbstub::arch::lldb::Format::Binary,
                    set: "general",
                    gcc: None,
                    dwarf: None,
                    generic: Some(gdbstub::arch::lldb::Generic::Flags),
                    container_regs: None,
                    invalidate_regs: None,
                },
            )),
            5 => Some(gdbstub::arch::lldb::RegisterInfo::Register(
                gdbstub::arch::lldb::Register {
                    name: "PC",
                    alt_name: None,
                    bitsize: 16,
                    offset: 5,
                    encoding: gdbstub::arch::lldb::Encoding::Uint,
                    format: gdbstub::arch::lldb::Format::Hex,
                    set: "general",
                    gcc: None,
                    dwarf: None,
                    generic: Some(gdbstub::arch::lldb::Generic::Pc),
                    container_regs: None,
                    invalidate_regs: None,
                },
            )),
            _ => Some(gdbstub::arch::lldb::RegisterInfo::Done),
        }
    }
}

#[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
pub struct Registers {
    a: u8,
    x: u8,
    y: u8,
    s: u8,
    p: u8,
    pc: u16,
}

impl gdbstub::arch::Registers for Registers {
    type ProgramCounter = u16;

    fn pc(&self) -> Self::ProgramCounter {
        self.pc
    }

    fn gdb_serialize(&self, mut write_byte: impl FnMut(Option<u8>)) {
        write_byte(Some(self.a));
        write_byte(Some(self.x));
        write_byte(Some(self.y));
        write_byte(Some(self.s));
        write_byte(Some(self.p));
        let [lo, hi] = self.pc.to_le_bytes();
        write_byte(Some(lo));
        write_byte(Some(hi));
    }

    fn gdb_deserialize(&mut self, bytes: &[u8]) -> Result<(), ()> {
        self.a = bytes[0];
        self.x = bytes[1];
        self.y = bytes[2];
        self.s = bytes[3];
        self.p = bytes[4];
        self.pc = u16::from_le_bytes([bytes[5], bytes[6]]);
        Ok(())
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum RegId {
    A,
    X,
    Y,
    S,
    P,
    Pc,
}

impl gdbstub::arch::RegId for RegId {
    fn from_raw_id(id: usize) -> Option<(Self, Option<std::num::NonZeroUsize>)> {
        match id {
            0 => Some((Self::A, Some(NonZeroUsize::new(1).expect("non-zero")))),
            1 => Some((Self::X, Some(NonZeroUsize::new(1).expect("non-zero")))),
            2 => Some((Self::Y, Some(NonZeroUsize::new(1).expect("non-zero")))),
            3 => Some((Self::S, Some(NonZeroUsize::new(1).expect("non-zero")))),
            4 => Some((Self::P, Some(NonZeroUsize::new(1).expect("non-zero")))),
            5 => Some((Self::Pc, Some(NonZeroUsize::new(2).expect("non-zero")))),
            _ => None,
        }
    }

    fn to_raw_id(&self) -> Option<usize> {
        Some(match self {
            RegId::A => 0,
            RegId::X => 1,
            RegId::Y => 2,
            RegId::S => 3,
            RegId::P => 4,
            RegId::Pc => 5,
        })
    }
}
