mod artifact;
mod build;
mod bus;
mod config;
mod fault;
mod runner;
#[cfg(test)]
mod tests;
use std::{collections::HashSet, path::PathBuf};

struct Options {
    config: PathBuf,
    cases: HashSet<String>,
    fail_fast: bool,
}
fn options(args: impl Iterator<Item = std::ffi::OsString>) -> Result<Options, String> {
    let mut path = None;
    let mut cases = HashSet::new();
    let mut fail_fast = false;
    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--case") => {
                let value = args
                    .next()
                    .ok_or("--caseには名前が必要です")?
                    .into_string()
                    .map_err(|_| "case名はUTF-8が必要です")?;
                cases.insert(value);
            }
            Some("--fail-fast") => fail_fast = true,
            Some(s) if s.starts_with('-') => return Err(format!("未知option: {s}")),
            _ => {
                if path.replace(PathBuf::from(arg)).is_some() {
                    return Err("設定pathは一つだけ指定できます".into());
                }
            }
        }
    }
    Ok(Options {
        config: path.unwrap_or_else(|| PathBuf::from("ond-unit.toml")),
        cases,
        fail_fast,
    })
}
fn run() -> Result<u8, String> {
    let options = options(std::env::args_os().skip(1))?;
    let path = std::fs::canonicalize(&options.config)
        .map_err(|e| format!("設定 {}: {e}", options.config.display()))?;
    let config = config::Config::read(&path)?;
    for name in &options.cases {
        if !config.cases.iter().any(|c| &c.name == name) {
            return Err(format!("存在しないcase: {name}"));
        }
    }
    let image = build::build(&config, &path)?;
    let program = artifact::Program::read(&image, &config)?;
    let exit = config.exit_address()?;
    let mut counts = [0usize; 4];
    for (index, case) in config.cases.iter().enumerate() {
        if !options.cases.is_empty() && !options.cases.contains(&case.name) {
            continue;
        }
        let outcome = runner::run(&config, case, index as u32, &program, exit);
        counts[match outcome.status {
            runner::Status::Pass => 0,
            runner::Status::Fail => 1,
            runner::Status::StepLimit => 2,
            runner::Status::Error => 3,
        }] += 1;
        println!(
            "{}: {} (steps={}, PC=0x{:08x}){}{}",
            case.name,
            outcome.status,
            outcome.steps,
            outcome.pc,
            if outcome.reason.is_empty() { "" } else { ": " },
            outcome.reason
        );
        if outcome.status == runner::Status::Error
            || (options.fail_fast && outcome.status != runner::Status::Pass)
        {
            break;
        }
    }
    println!(
        "集計: PASS={} FAIL={} STEP_LIMIT={} ERROR={}",
        counts[0], counts[1], counts[2], counts[3]
    );
    Ok(if counts[3] > 0 {
        2
    } else if counts[1] + counts[2] > 0 {
        1
    } else {
        0
    })
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(code) => std::process::ExitCode::from(code),
        Err(reason) => {
            eprintln!("ERROR: {reason}\n集計: PASS=0 FAIL=0 STEP_LIMIT=0 ERROR=1");
            std::process::ExitCode::from(2)
        }
    }
}
