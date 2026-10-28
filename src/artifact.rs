use crate::config::{Config, overlap};
use ond_protocol::{
    debug::{DebugInfo, DebugOperation, ReturnAddressLocation, SourceLocation},
    executable::Executable,
    object::SectionKind,
};
use sha2::{Digest, Sha256};
use std::path::Path;

pub struct Program {
    pub bytes: Vec<u8>,
    pub image: Executable,
    pub debug: DebugInfo,
}
impl Program {
    pub fn read(path: &Path, config: &Config) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("ONDX読込: {e}"))?;
        let image = Executable::read(&bytes)?;
        let debug_bytes = std::fs::read(path.with_extension("onddebug"))
            .map_err(|e| format!("debug sidecar読込: {e}"))?;
        // serdeのstruct visitorが未知fieldと重複fieldを拒否する。
        let debug: DebugInfo =
            serde_json::from_slice(&debug_bytes).map_err(|e| format!("debug sidecar: {e}"))?;
        validate_debug(&image, &debug, &bytes)?;
        let ram_end = config.vm.ram_size - 8;
        let stack_bottom = ram_end - config.vm.stack_size;
        if u64::from(image.initial_sp) != ram_end || u64::from(debug.stack_bottom) != stack_bottom {
            return Err("ONDX/debug stackとlink設定が一致しません".into());
        }
        if bytes.len() as u64 > stack_bottom {
            return Err("ONDXコピー範囲がstackと重複します".into());
        }
        let mut protected = vec![(0, bytes.len() as u64)];
        for section in &image.sections {
            let start = u64::from(section.memory_address);
            let end = start + u64::from(section.memory_size);
            if end > stack_bottom {
                return Err("ONDX sectionがstackと重複します".into());
            }
            if !section.data.is_empty() && section.memory_address != section.file_offset {
                return Err("ONDXはbase 0のRAMコピー配置が必要です".into());
            }
            let zero_start = start + section.data.len() as u64;
            if zero_start < end && zero_start < bytes.len() as u64 {
                return Err("ONDX zero範囲がコピー範囲と重複します".into());
            }
            protected.push((start, end));
        }
        for case in &config.cases {
            for region in &case.ram_initial {
                if protected.iter().any(|p| overlap(*p, region.bounds())) {
                    return Err(format!(
                        "case {}: RAM初期値がロード契約の範囲と重複します",
                        case.name
                    ));
                }
            }
        }
        Ok(Self {
            bytes,
            image,
            debug,
        })
    }
}
fn valid_source(source: Option<&SourceLocation>) -> bool {
    source.is_none_or(|s| !s.path.is_empty() && s.line > 0 && s.column > 0)
}
pub fn validate_debug(image: &Executable, debug: &DebugInfo, bytes: &[u8]) -> Result<(), String> {
    let fail = || "debug metadataの静的成立条件に違反しています".to_owned();
    if debug.build_id != format!("{:x}", Sha256::digest(bytes))
        || debug.stack_top != image.initial_sp
        || debug.stack_bottom >= debug.stack_top
        || !debug.stack_bottom.is_multiple_of(4)
    {
        return Err(fail());
    }
    let text = image
        .sections
        .iter()
        .find(|s| s.kind == SectionKind::Text)
        .ok_or_else(fail)?;
    let text_start = u64::from(text.memory_address);
    let text_end = text_start + u64::from(text.memory_size);
    let mut previous_end = text_start;
    for f in &debug.functions {
        let end = u64::from(f.address) + u64::from(f.size);
        if f.symbol.is_empty()
            || f.size == 0
            || !f.address.is_multiple_of(4)
            || !f.size.is_multiple_of(4)
            || u64::from(f.address) < previous_end
            || end > text_end
            || end > u64::from(u32::MAX)
            || !valid_source(f.source.as_ref())
            || !f.frame_size.is_multiple_of(8)
        {
            return Err(fail());
        }
        previous_end = end;
        if f.return_address_offset != 0
            && (!f.return_address_offset.is_multiple_of(4)
                || u64::from(f.return_address_offset) + 4 > u64::from(f.frame_size))
        {
            return Err(fail());
        }
        let mut previous_row = None;
        for (i, r) in f.unwind.iter().enumerate() {
            if r.address < f.address
                || u64::from(r.address) > end
                || !r.address.is_multiple_of(4)
                || previous_row.is_some_and(|a| a >= r.address)
                || !r.cfa_sp_offset.is_multiple_of(8)
                || r.cfa_sp_offset > f.frame_size
            {
                return Err(fail());
            }
            if i == 0
                && (r.address != f.address
                    || r.cfa_sp_offset != 0
                    || r.return_address != ReturnAddressLocation::LinkRegister)
            {
                return Err(fail());
            }
            if let ReturnAddressLocation::StackOffset(n) = r.return_address
                && (!n.is_multiple_of(4) || u64::from(n) + 4 > u64::from(r.cfa_sp_offset))
            {
                return Err(fail());
            }
            previous_row = Some(r.address);
        }
    }
    let mut previous_range = None;
    for r in &debug.ranges {
        let key = (r.start, r.end);
        if r.function.is_empty()
            || !r.start.is_multiple_of(4)
            || !r.end.is_multiple_of(4)
            || r.start >= r.end
            || u64::from(r.start) < text_start
            || u64::from(r.end) > text_end
            || previous_range.is_some_and(|k| k > key)
            || !valid_source(r.source.as_ref())
            || (r.operation == DebugOperation::ExplicitTrap) != r.message.is_some()
        {
            return Err(fail());
        }
        previous_range = Some(key);
    }
    Ok(())
}
