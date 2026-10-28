use crate::{
    artifact::Program,
    bus::TestBus,
    config::{Case, Config},
    fault,
};
use kagura::cpu::{Cpu, Fault};
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Status {
    Pass,
    Fail,
    StepLimit,
    Error,
}
impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::StepLimit => "STEP_LIMIT",
            Self::Error => "ERROR",
        })
    }
}
pub struct Outcome {
    pub status: Status,
    pub reason: String,
    pub steps: u64,
    pub pc: u32,
}
pub enum Stop {
    Normal,
    Fault(Fault),
    Limit,
    Mock(String),
}
pub fn execute(cpu: &mut Cpu, bus: &mut TestBus<'_>, exit: u32, limit: u64) -> (Stop, u64) {
    let mut steps = 0;
    loop {
        if cpu.pc() == exit {
            return (Stop::Normal, steps);
        }
        if steps == limit {
            return (Stop::Limit, steps);
        }
        steps += 1;
        let result = cpu.step(bus);
        if let Some(reason) = bus.mismatch.take() {
            return (Stop::Mock(reason), steps);
        }
        if let Err(fault) = result {
            return (Stop::Fault(fault), steps);
        }
    }
}
pub fn run(config: &Config, case: &Case, index: u32, program: &Program, exit: u32) -> Outcome {
    match run_inner(config, case, index, program, exit) {
        Ok(outcome) => outcome,
        Err(reason) => Outcome {
            status: Status::Error,
            reason,
            steps: 0,
            pc: program.image.entry_address,
        },
    }
}
fn run_inner(
    config: &Config,
    case: &Case,
    index: u32,
    program: &Program,
    exit: u32,
) -> Result<Outcome, String> {
    let mut bus = TestBus::new(config, case)?;
    bus.ram[..program.bytes.len()].copy_from_slice(&program.bytes);
    for s in &program.image.sections {
        let start = s.memory_address as usize + s.data.len();
        let end = s.memory_address as usize + s.memory_size as usize;
        bus.ram[start..end].fill(0);
    }
    for r in &case.ram_initial {
        bus.ram[r.address as usize..r.bounds().1 as usize].copy_from_slice(&r.bytes);
    }
    let args = bus.ram.len() - 8;
    bus.ram[args..args + 4].copy_from_slice(&index.to_le_bytes());
    bus.ram[args + 4..].fill(0);
    let mut cpu = Cpu::new();
    cpu.set_pc(program.image.entry_address);
    cpu.set_reg(14, program.image.initial_sp);
    let (stop, steps) = execute(&mut cpu, &mut bus, exit, case.max_steps);
    let pc = cpu.pc();
    let mut failures = bus.expectations(case);
    match stop {
        Stop::Limit => {
            return Ok(Outcome {
                status: Status::StepLimit,
                reason: format!(
                    "上限={} 実行ステップ数={steps} 停止PC=0x{pc:08x}",
                    case.max_steps
                ),
                steps,
                pc,
            });
        }
        Stop::Mock(reason) => {
            return Ok(Outcome {
                status: Status::Fail,
                reason,
                steps,
                pc,
            });
        }
        Stop::Normal => {
            if case.expect_fault.is_some() {
                failures.push("期待faultが発生せず正常終了しました".into());
            }
        }
        Stop::Fault(f) => match &case.expect_fault {
            Some(expected) => match fault::matches(expected, f, &program.debug) {
                Ok(mismatches) => {
                    if !mismatches.is_empty() {
                        failures.push(fault::describe(f, &program.debug));
                    }
                    failures.extend(mismatches);
                }
                Err(reason) => {
                    return Ok(Outcome {
                        status: Status::Error,
                        reason,
                        steps,
                        pc,
                    });
                }
            },
            None => failures.push(format!(
                "予期しないfault: {}",
                fault::describe(f, &program.debug)
            )),
        },
    }
    Ok(Outcome {
        status: if failures.is_empty() {
            Status::Pass
        } else {
            Status::Fail
        },
        reason: failures.join("; "),
        steps,
        pc,
    })
}
