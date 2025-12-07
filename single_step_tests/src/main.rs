use anyhow::{Context, anyhow};
use cody_emulator::cpu::{Cpu, Status};
use cody_emulator::memory::Memory;
use cody_emulator::memory::contiguous::Contiguous;
use cody_emulator::memory::logging::{LoggingMemory, MemoryAccess, MemoryAccessType};
use cody_emulator::opcode::OPCODES;
use single_step_tests::{CycleOp, TestCase};
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::panic::catch_unwind;
use std::path::Path;

const CHECK_MEMORY_ACCESSES: bool = false;

fn main() -> anyhow::Result<()> {
    // only documented opcodes
    let test_cases: Vec<TestCase> = {
        let mut v = vec![];
        for opc in OPCODES {
            let path = format!("65x02/wdc65c02/v1/{:02x}.json", opc.byte);
            let test_cases = collect_test_cases(path)?;
            v.extend(test_cases);
        }
        v
    };

    for test_case in test_cases {
        println!("Test Case: {}", test_case.name);
        let result = catch_unwind(|| execute_test_case(&test_case));
        if result.is_err() {
            println!("Test Case: {test_case:?} => FAIL");
        }
        if result.is_err() {
            return Err(anyhow!("test failed"));
        }
    }

    Ok(())
}

fn collect_test_cases(path: impl AsRef<Path>) -> anyhow::Result<Vec<TestCase>> {
    let path = path.as_ref();
    if path.is_dir() {
        collect_test_cases_from_dir(path)
    } else {
        collect_test_cases_from_file(path)
    }
}

fn collect_test_cases_from_dir(path: impl AsRef<Path>) -> anyhow::Result<Vec<TestCase>> {
    let path = path.as_ref();
    let ctx = path.display().to_string();

    let mut test_cases = vec![];
    for e in fs::read_dir(path).context(ctx.clone())? {
        let e = e.context(ctx.clone())?;
        let path = e.path();
        let metadata = fs::metadata(&path).context(ctx.clone())?;
        if metadata.is_file() && path.extension().is_some_and(|ext| ext == "json") {
            test_cases.extend(collect_test_cases_from_file(path)?);
        }
    }

    Ok(test_cases)
}

fn collect_test_cases_from_file(path: impl AsRef<Path>) -> anyhow::Result<Vec<TestCase>> {
    let path = path.as_ref();
    let ctx = path.display().to_string();

    if fs::metadata(path).context(ctx.clone())?.len() == 0 {
        return Ok(vec![]);
    }

    let file = File::open(path).context(ctx.clone())?;
    serde_json::from_reader(BufReader::new(file)).context(ctx)
}

fn execute_test_case(test_case: &TestCase) {
    let memory = LoggingMemory::new(Contiguous::new_ram(0x10000));
    let mut cpu = Cpu::new(memory);
    cpu.pc = test_case.initial.pc;
    cpu.s = test_case.initial.s;
    cpu.a = test_case.initial.a;
    cpu.x = test_case.initial.x;
    cpu.y = test_case.initial.y;
    cpu.p = Status::from_bits(test_case.initial.p);

    for ram_value in &test_case.initial.ram {
        cpu.memory.write_u8(ram_value.address(), ram_value.value());
    }

    cpu.memory.reset_log();
    let cycles = cpu.step_instruction();

    assert_eq!(
        cycles as usize,
        test_case.cycles.len(),
        "cycles: expected={}, actual={}",
        test_case.cycles.len(),
        cycles
    );
    if CHECK_MEMORY_ACCESSES {
        assert_eq!(
            cpu.memory.log().len(),
            test_case.cycles.len(),
            "memory accesses: expected={}, actual={}",
            test_case.cycles.len(),
            cpu.memory.log().len()
        );
        for (idx, (cycle, &memory_access)) in
            test_case.cycles.iter().zip(cpu.memory.log()).enumerate()
        {
            let expected = MemoryAccess {
                access_type: match cycle.op() {
                    CycleOp::Read => MemoryAccessType::Read,
                    CycleOp::Write => MemoryAccessType::Write,
                },
                address: cycle.address(),
                value: cycle.value(),
            };
            assert_eq!(
                memory_access,
                expected,
                "cycle[{}]: expected={:?}, actual={:?}",
                idx + 1,
                expected,
                memory_access
            );
        }
    }
    assert_eq!(
        cpu.pc, test_case.r#final.pc,
        "pc: expected={}, actual={}",
        test_case.r#final.pc, cpu.pc
    );
    assert_eq!(
        cpu.s, test_case.r#final.s,
        "s: expected={}, actual={}",
        test_case.r#final.s, cpu.s
    );
    assert_eq!(
        cpu.a, test_case.r#final.a,
        "a: expected={}, actual={}",
        test_case.r#final.a, cpu.a
    );
    assert_eq!(
        cpu.x, test_case.r#final.x,
        "x: expected={}, actual={}",
        test_case.r#final.x, cpu.x
    );
    assert_eq!(
        cpu.y, test_case.r#final.y,
        "y: expected={}, actual={}",
        test_case.r#final.y, cpu.y
    );
    assert_eq!(
        cpu.p,
        Status::from_bits(test_case.r#final.p),
        "p: expected={} ({:?}), actual={} ({:?})",
        test_case.r#final.p,
        Status::from_bits(test_case.r#final.p),
        cpu.p.into_bits(),
        cpu.p
    );
    for ram_value in &test_case.r#final.ram {
        assert_eq!(
            cpu.memory.read_u8(ram_value.address()),
            ram_value.value(),
            "mem[{}]: expected={}, actual={}",
            ram_value.address(),
            ram_value.value(),
            cpu.memory.read_u8(ram_value.address())
        );
    }
}
