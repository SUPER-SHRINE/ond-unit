use serde::Deserialize;
use std::{collections::HashSet, path::Path};

pub const SPACE: u64 = 1 << 32;
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub project: Project,
    pub vm: Vm,
    #[serde(default)]
    pub mmio: Vec<Device>,
    pub cases: Vec<Case>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub path: String,
    pub role: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vm {
    pub ram_size: u64,
    pub stack_size: u64,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Device {
    pub name: String,
    pub base: u32,
    pub size: u64,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub name: String,
    pub package: String,
    pub function: String,
    #[serde(default = "default_steps")]
    pub max_steps: u64,
    pub expect_fault: Option<ExpectedFault>,
    #[serde(default)]
    pub ram_initial: Vec<RamRegion>,
    #[serde(default)]
    pub ram_expected: Vec<RamRegion>,
    #[serde(default)]
    pub mmio: Vec<CaseDevice>,
}
fn default_steps() -> u64 {
    1_000_000
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedFault {
    pub cpu_kind: Option<CpuKind>,
    pub ond_kind: Option<OndKind>,
    pub function: Option<String>,
    pub source: Option<Source>,
    pub reason: Option<String>,
    pub address: Option<u32>,
    pub width: Option<u8>,
    pub operation: Option<FaultOperation>,
}
#[derive(Debug, Deserialize, PartialEq, Eq, Clone, Copy)]
pub enum CpuKind {
    InvalidInstruction,
    UnalignedAccess,
    UnalignedPc,
    BusFault,
}
#[derive(Debug, Deserialize, PartialEq, Eq, Clone, Copy)]
pub enum OndKind {
    BoundsCheck,
    DivisionByZero,
    SignedDivisionOverflow,
    InvalidConversion,
    StackOverflow,
    ExplicitTrap,
}
#[derive(Debug, Deserialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum FaultOperation {
    Fetch,
    Load,
    Store,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub path: String,
    pub line: u32,
    pub column: Option<u32>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RamRegion {
    pub address: u32,
    pub bytes: Vec<u8>,
}
impl RamRegion {
    pub fn bounds(&self) -> (u64, u64) {
        (
            u64::from(self.address),
            u64::from(self.address) + self.bytes.len() as u64,
        )
    }
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseDevice {
    pub device: String,
    #[serde(default)]
    pub read_responses: Vec<Response>,
    pub expected_accesses: Option<Vec<Access>>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub width: u8,
    pub value: u32,
}
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Access {
    pub operation: Operation,
    pub address: u32,
    pub width: u8,
    pub value: u32,
}
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Read,
    Write,
}

pub fn overlap(a: (u64, u64), b: (u64, u64)) -> bool {
    a.0 < a.1 && b.0 < b.1 && a.0 < b.1 && b.0 < a.1
}
pub fn nonempty(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty() {
        Err(format!("{label} は非空文字列が必要です"))
    } else {
        Ok(())
    }
}
fn width(value: u8) -> Result<(), String> {
    if matches!(value, 1 | 2 | 4) {
        Ok(())
    } else {
        Err(format!(
            "access width {value} は1、2、4のいずれかが必要です"
        ))
    }
}
fn response(w: u8, v: u32) -> Result<(), String> {
    width(w)?;
    if u64::from(v) < (1u64 << (w * 8)) {
        Ok(())
    } else {
        Err("access valueが幅に収まりません".into())
    }
}
impl Config {
    pub fn read(path: &Path) -> Result<Self, String> {
        let text =
            std::fs::read_to_string(path).map_err(|e| format!("設定 {}: {e}", path.display()))?;
        let config: Self = toml::from_str(&text).map_err(|e| format!("設定: {e}"))?;
        config.validate()?;
        Ok(config)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("version は1が必要です".into());
        }
        nonempty(&self.project.path, "project.path")?;
        if let Some(role) = &self.project.role {
            nonempty(role, "project.role")?;
        }
        let n = self.vm.ram_size;
        if n <= 8 || n > SPACE || !n.is_multiple_of(8) {
            return Err("ram_size は8より大きい2^32以下の8の倍数が必要です".into());
        }
        let stack = self.vm.stack_size;
        if stack == 0 || stack >= n - 8 || !stack.is_multiple_of(4) {
            return Err("stack_size は通常link RAM容量未満の正の4の倍数が必要です".into());
        }
        if self.cases.len() as u64 > SPACE {
            return Err("case indexがu32を超えます".into());
        }
        let mut names = HashSet::new();
        let mut occupied = vec![(0, n)];
        for dev in &self.mmio {
            nonempty(&dev.name, "mmio.name")?;
            if !names.insert(&dev.name) {
                return Err(format!("MMIO名重複: {}", dev.name));
            }
            let start = u64::from(dev.base);
            let end = start.checked_add(dev.size).ok_or("MMIO範囲overflow")?;
            if dev.size == 0 || end > SPACE || occupied.iter().any(|r| overlap(*r, (start, end))) {
                return Err(format!("MMIO範囲不正または重複: {}", dev.name));
            }
            occupied.push((start, end));
        }
        let mut case_names = HashSet::new();
        for case in &self.cases {
            for (value, label) in [
                (&case.name, "cases.name"),
                (&case.package, "cases.package"),
                (&case.function, "cases.function"),
            ] {
                nonempty(value, label)?;
            }
            // ASCIIの構文文字を生成式へ挿入させず、identifierの成立は通常compileへ委ねる。
            if !case.function.chars().all(|c| {
                c == '_'
                    || c.is_alphanumeric()
                    || (!c.is_ascii() && !c.is_control() && !c.is_whitespace())
            }) {
                return Err(format!("関数identifier不正: {}", case.function));
            }
            if !case_names.insert(&case.name) {
                return Err(format!("case名重複: {}", case.name));
            }
            if case.max_steps == 0 {
                return Err("max_stepsは正が必要です".into());
            }
            if let Some(f) = &case.expect_fault {
                if f.cpu_kind.is_none() && f.ond_kind.is_none() {
                    return Err("expect_faultはcpu_kindまたはond_kindが必要です".into());
                }
                if f.function.is_none() && f.source.is_none() {
                    return Err("expect_faultはfunctionまたはsourceが必要です".into());
                }
                if let Some(s) = &f.function {
                    nonempty(s, "expect_fault.function")?;
                }
                if let Some(s) = &f.source {
                    nonempty(&s.path, "expect_fault.source.path")?;
                    if s.line == 0 || s.column == Some(0) {
                        return Err("source行列は1以上が必要です".into());
                    }
                }
                if let Some(w) = f.width {
                    width(w)?;
                }
            }
            for r in case.ram_initial.iter().chain(&case.ram_expected) {
                if r.bounds().1 > n - 8 {
                    return Err(format!("RAM領域が通常link RAM外: {}", case.name));
                }
            }
            for (i, a) in case.ram_initial.iter().enumerate() {
                if case.ram_initial[..i]
                    .iter()
                    .any(|b| overlap(a.bounds(), b.bounds()))
                {
                    return Err(format!("RAM初期値重複: {}", case.name));
                }
            }
            let mut devices = HashSet::new();
            for d in &case.mmio {
                if !devices.insert(&d.device) {
                    return Err(format!("case内device重複: {}", d.device));
                }
                let common = self
                    .mmio
                    .iter()
                    .find(|v| v.name == d.device)
                    .ok_or_else(|| format!("未登録device: {}", d.device))?;
                for r in &d.read_responses {
                    response(r.width, r.value)?;
                }
                if let Some(accesses) = &d.expected_accesses {
                    for a in accesses {
                        response(a.width, a.value)?;
                        let start = u64::from(a.address);
                        if start < u64::from(common.base)
                            || start + u64::from(a.width) > u64::from(common.base) + common.size
                        {
                            return Err(format!("期待accessがdevice範囲外: {}", d.device));
                        }
                    }
                }
            }
        }
        self.exit_address()?;
        Ok(())
    }
    pub fn exit_address(&self) -> Result<u32, String> {
        let mut occupied = self
            .mmio
            .iter()
            .map(|d| (u64::from(d.base), u64::from(d.base) + d.size))
            .collect::<Vec<_>>();
        occupied.push((0, self.vm.ram_size));
        let mut candidate = SPACE - 4;
        loop {
            match occupied
                .iter()
                .find(|r| overlap(**r, (candidate, candidate + 4)))
            {
                None => return Ok(candidate as u32),
                Some((start, _)) => {
                    if *start < 4 {
                        return Err("正常終了先を確保できません".into());
                    }
                    candidate = (start - 4) & !3;
                }
            }
        }
    }
}
