use crate::config::{CpuKind, ExpectedFault, FaultOperation, OndKind};
use kagura::{
    bus::AccessWidth,
    cpu::{AccessOperation, Fault, FaultCode},
};
use ond_protocol::debug::{DebugInfo, DebugOperation, DebugRange};

pub fn range_at(debug: &DebugInfo, pc: u32) -> Option<&DebugRange> {
    // min_by_keyは同値の最初を保持し、wire順をlookup優先順位にする。
    debug
        .ranges
        .iter()
        .filter(|r| r.start <= pc && pc < r.end)
        .min_by_key(|r| r.end - r.start)
}
fn cpu_kind(f: Fault) -> CpuKind {
    match f.code() {
        FaultCode::InvalidInstruction => CpuKind::InvalidInstruction,
        FaultCode::UnalignedAccess => CpuKind::UnalignedAccess,
        FaultCode::UnalignedPc => CpuKind::UnalignedPc,
        FaultCode::BusFault => CpuKind::BusFault,
    }
}
fn ond_kind(f: Fault, range: &DebugRange) -> Option<OndKind> {
    if f.code() != FaultCode::InvalidInstruction {
        return None;
    }
    match range.operation {
        DebugOperation::BoundsCheck => Some(OndKind::BoundsCheck),
        DebugOperation::DivisionByZero => Some(OndKind::DivisionByZero),
        DebugOperation::SignedDivisionOverflow => Some(OndKind::SignedDivisionOverflow),
        DebugOperation::InvalidConversion => Some(OndKind::InvalidConversion),
        DebugOperation::StackOverflow => Some(OndKind::StackOverflow),
        DebugOperation::ExplicitTrap => Some(OndKind::ExplicitTrap),
        _ => None,
    }
}
fn fault_access(f: Fault) -> (Option<u32>, Option<u8>, Option<FaultOperation>) {
    match f {
        Fault::BusFault {
            addr,
            width,
            operation,
            ..
        }
        | Fault::UnalignedAccess {
            addr,
            width,
            operation,
            ..
        } => (
            Some(addr),
            Some(match width {
                AccessWidth::Byte => 1,
                AccessWidth::Half => 2,
                AccessWidth::Word => 4,
            }),
            Some(match operation {
                AccessOperation::Fetch => FaultOperation::Fetch,
                AccessOperation::Load => FaultOperation::Load,
                AccessOperation::Store => FaultOperation::Store,
            }),
        ),
        Fault::UnalignedPc { addr, .. } => (Some(addr), None, None),
        Fault::InvalidInstruction { .. } => (None, None, None),
    }
}
pub fn matches(
    expected: &ExpectedFault,
    fault: Fault,
    debug: &DebugInfo,
) -> Result<Vec<String>, String> {
    let mut failures = Vec::new();
    let pc = fault.faulting_pc();
    let range =
        if expected.ond_kind.is_some() || expected.reason.is_some() || expected.source.is_some() {
            Some(
                range_at(debug, pc)
                    .ok_or_else(|| format!("fault PC 0x{pc:08x} に照合用DebugRangeがありません"))?,
            )
        } else {
            None
        };
    let function = if expected.function.is_some() {
        Some(
            debug
                .functions
                .iter()
                .find(|f| {
                    f.address <= pc && u64::from(pc) < u64::from(f.address) + u64::from(f.size)
                })
                .ok_or_else(|| format!("fault PC 0x{pc:08x} にFunctionDebugがありません"))?,
        )
    } else {
        None
    };
    if let Some(source) = &expected.source {
        let actual = range
            .and_then(|r| r.source.as_ref())
            .ok_or_else(|| format!("fault PC 0x{pc:08x} にDebugRange.sourceがありません"))?;
        if source.path != actual.path
            || source.line != actual.line
            || source.column.is_some_and(|c| c != actual.column)
        {
            failures.push(format!(
                "source不一致: expected={source:?} actual={actual:?}"
            ));
        }
    }
    if expected.cpu_kind.is_some_and(|k| k != cpu_kind(fault)) {
        failures.push(format!(
            "cpu_kind不一致: expected={:?} actual={:?}",
            expected.cpu_kind,
            cpu_kind(fault)
        ));
    }
    if expected
        .ond_kind
        .is_some_and(|k| range.and_then(|r| ond_kind(fault, r)) != Some(k))
    {
        failures.push(format!(
            "ond_kind不一致: expected={:?} actual={:?}",
            expected.ond_kind,
            range.and_then(|r| ond_kind(fault, r))
        ));
    }
    if let Some(name) = &expected.function {
        let actual = &function.unwrap().symbol;
        if name != actual {
            failures.push(format!(
                "function不一致: expected={name:?} actual={actual:?}"
            ));
        }
    }
    if let Some(reason) = &expected.reason
        && range.and_then(|r| r.message.as_ref()) != Some(reason)
    {
        failures.push(format!(
            "reason不一致: expected={reason:?} actual={:?}",
            range.and_then(|r| r.message.as_ref())
        ));
    }
    let (address, width, operation) = fault_access(fault);
    if expected.address.is_some_and(|v| Some(v) != address) {
        failures.push(format!(
            "address不一致: expected={:?} actual={address:?}",
            expected.address
        ));
    }
    if expected.width.is_some_and(|v| Some(v) != width) {
        failures.push(format!(
            "width不一致: expected={:?} actual={width:?}",
            expected.width
        ));
    }
    if expected.operation.is_some_and(|v| Some(v) != operation) {
        failures.push(format!(
            "operation不一致: expected={:?} actual={operation:?}",
            expected.operation
        ));
    }
    Ok(failures)
}

pub fn describe(fault: Fault, debug: &DebugInfo) -> String {
    let pc = fault.faulting_pc();
    let mut detail = format!("{fault:?}");
    if let Some(function) = debug
        .functions
        .iter()
        .find(|f| f.address <= pc && u64::from(pc) < u64::from(f.address) + u64::from(f.size))
    {
        detail.push_str(&format!(" function={:?}", function.symbol));
    }
    if let Some(range) = range_at(debug, pc) {
        if let Some(kind) = ond_kind(fault, range) {
            detail.push_str(&format!(" ond_kind={kind:?}"));
        }
        if let Some(source) = &range.source {
            detail.push_str(&format!(
                " source={:?}:{}:{}",
                source.path, source.line, source.column
            ));
        }
        if let Some(message) = &range.message {
            detail.push_str(&format!(" reason={message:?}"));
        }
    }
    detail
}
