use crate::{
    artifact::{Program, validate_debug},
    bus::TestBus,
    config::{Access, Config, Operation},
    fault,
    runner::{self, Status, Stop},
};
use kagura::{
    bus::Bus,
    cpu::{Cpu, Fault},
};
use ond_protocol::{
    debug::{DebugInfo, DebugOperation, DebugRange, FunctionDebug, SourceLocation},
    executable::{Executable, ExecutableSection},
    object::SectionKind,
};
use sha2::{Digest, Sha256};

const BASIC: &str = r#"
version = 1
[project]
path = "."
[vm]
ram_size = 1024
stack_size = 128
[[cases]]
name = "first"
package = "checks/cases"
function = "Run"
"#;
fn config(text: &str) -> Config {
    let c: Config = toml::from_str(text).unwrap();
    c.validate().unwrap();
    c
}
fn rejects(text: &str) {
    assert!(
        toml::from_str::<Config>(text)
            .and_then(|c| c.validate().map_err(serde::de::Error::custom))
            .is_err(),
        "accepted {text}"
    );
}
fn mock_config(extra: &str) -> Config {
    config(&format!(
        "{}\n[[mmio]]\nname='port'\nbase=4096\nsize=8\n{extra}",
        BASIC
    ))
}
fn source() -> SourceLocation {
    SourceLocation {
        path: "opaque/../case.ond".into(),
        line: 7,
        column: 3,
    }
}
fn program(words: &[u32]) -> Program {
    let image = Executable {
        entry_address: 56,
        initial_sp: 1016,
        sections: vec![ExecutableSection {
            kind: SectionKind::Text,
            file_offset: 56,
            memory_address: 56,
            memory_size: words.len() as u32 * 4,
            alignment: 4,
            data: words.iter().flat_map(|w| w.to_le_bytes()).collect(),
        }],
    };
    let bytes = image.write().unwrap();
    let debug = DebugInfo {
        build_id: format!("{:x}", Sha256::digest(&bytes)),
        stack_bottom: 888,
        stack_top: 1016,
        functions: vec![FunctionDebug {
            symbol: "checks/cases.Run".into(),
            address: 56,
            size: words.len() as u32 * 4,
            frame_size: 0,
            return_address_offset: 0,
            source: None,
            unwind: vec![],
        }],
        ranges: vec![DebugRange {
            start: 56,
            end: 56 + words.len() as u32 * 4,
            operation: DebugOperation::ExplicitTrap,
            message: Some("reason".into()),
            source: Some(source()),
            function: "checks/cases.Run".into(),
        }],
    };
    Program {
        bytes,
        image,
        debug,
    }
}
fn jump(pc: u32, target: u32) -> u32 {
    0xc0000000 | (((target.wrapping_sub(pc + 4) as i32 / 4) as u16) as u32)
}
#[test]
fn config_closed_schema_and_required_fields() {
    rejects(&BASIC.replace("version = 1", "version = 2"));
    rejects(&BASIC.replace("version = 1", "version = 1\nunknown=1"));
    rejects(&BASIC.replace("path = \".\"", "path = \".\"\nunknown=1"));
    rejects(&BASIC.replace("stack_size = 128", "stack_size = 128\nunknown=1"));
    rejects(&BASIC.replace("function = \"Run\"", "function = \"Run\"\nunknown=1"));
    rejects(&BASIC.replace("version = 1", ""));
    rejects(&BASIC.replace("name = \"first\"", "name = \"\""));
    rejects(&BASIC.replace("ram_size = 1024", "ram_size = '1024'"));
    rejects(&BASIC.replace("function = \"Run\"", "function = 'Run; trap \"injected\"'"));
    rejects(&format!(
        "{BASIC}\n[[cases]]\nname='first'\npackage='p'\nfunction='F'"
    ));
    rejects(&BASIC.replace("version = 1", "version = 1\nversion=1"));
}
#[test]
fn project_role_is_optional_and_requires_a_nonempty_string() {
    assert!(config(BASIC).project.role.is_none());
    let selected = BASIC.replace("path = \".\"", "path = \".\"\nrole = 'Test'");
    assert_eq!(config(&selected).project.role.as_deref(), Some("Test"));
    for value in ["''", "7", "true", "['Test']"] {
        rejects(&BASIC.replace("path = \".\"", &format!("path = \".\"\nrole = {value}")));
    }
}
#[test]
fn numeric_and_overlap_boundaries() {
    for size in ["8", "9", "4294967304", "-8"] {
        rejects(&BASIC.replace("ram_size = 1024", &format!("ram_size = {size}")));
    }
    for size in ["0", "3", "1016", "-4"] {
        rejects(&BASIC.replace("stack_size = 128", &format!("stack_size = {size}")));
    }
    rejects(&format!("{BASIC}\nmax_steps=0"));
    rejects(&format!(
        "{BASIC}\nram_initial=[{{address=1015,bytes=[1,2]}}]"
    ));
    rejects(&format!(
        "{BASIC}\nram_initial=[{{address=100,bytes=[256]}}]"
    ));
    rejects(&format!(
        "{BASIC}\nram_initial=[{{address=500,bytes=[1,2]}},{{address=501,bytes=[3]}}]"
    ));
    rejects(&format!("{BASIC}\n[[mmio]]\nname='x'\nbase=1023\nsize=1"));
    rejects(&format!(
        "{BASIC}\n[[mmio]]\nname='x'\nbase=4294967295\nsize=2"
    ));
    rejects(&format!(
        "{BASIC}\n[[mmio]]\nname='x'\nbase=4096\nsize=4\n[[mmio]]\nname='y'\nbase=4099\nsize=2"
    ));
    let c = config(&BASIC.replace("ram_size = 1024", "ram_size = 4294967288"));
    assert_eq!(c.exit_address().unwrap(), 0xfffffffc);
    rejects(&BASIC.replace("ram_size = 1024", "ram_size = 4294967296"));
}
#[test]
fn highest_exit_uses_mathematical_interval_end() {
    let c = config(&format!(
        "{BASIC}\n[[mmio]]\nname='top'\nbase=4294967292\nsize=4"
    ));
    assert_eq!(c.exit_address().unwrap(), 0xfffffff8);
    let c = config(&format!(
        "{BASIC}\n[[mmio]]\nname='top'\nbase=4294967291\nsize=5"
    ));
    assert_eq!(c.exit_address().unwrap(), 0xfffffff4);
    rejects(&format!(
        "{BASIC}\n[[mmio]]\nname='all'\nbase=1024\nsize=4294966272"
    ));
}
#[test]
fn fault_config_requires_kind_and_location() {
    for expected in [
        "{}",
        "{cpu_kind='InvalidInstruction'}",
        "{function='f'}",
        "{cpu_kind='Unknown',function='f'}",
        "{cpu_kind='BusFault',function='f',width=3}",
        "{cpu_kind='BusFault',source={path='p',line=0}}",
        "{cpu_kind='BusFault',function='f',operation='read'}",
    ] {
        rejects(&format!("{BASIC}\nexpect_fault={expected}"));
    }
    let c = config(&format!(
        "{BASIC}\nmax_steps=1\nexpect_fault={{ond_kind='ExplicitTrap',source={{path='p',line=1}}}}"
    ));
    assert_eq!(c.cases[0].max_steps, 1);
}
#[test]
fn mmio_config_references_width_and_range() {
    let base = format!("{BASIC}\n[[mmio]]\nname='port'\nbase=4096\nsize=8\n");
    // cases.mmioはcasesの子なのでroot mmio宣言の前に配置する。
    for setting in [
        "device='missing'",
        "device='port'\nread_responses=[{width=3,value=1}]",
        "device='port'\nread_responses=[{width=1,value=256}]",
        "device='port'\nexpected_accesses=[{operation='read',address=4103,width=2,value=0}]",
        "device='port'\nextra=0",
    ] {
        let text = base.replace(
            "\n[[mmio]]",
            &format!("\n[[cases.mmio]]\n{setting}\n[[mmio]]"),
        );
        rejects(&text);
    }
}
#[test]
fn mmio_atomic_accesses_and_independent_responses() {
    let c = config(&format!(
        "{BASIC}\n[[cases.mmio]]\ndevice='port'\nread_responses=[{{width=4,value=305419896}},{{width=1,value=7}}]\nexpected_accesses=[{{operation='read',address=4096,width=4,value=305419896}},{{operation='write',address=4100,width=2,value=43981}}]\n[[mmio]]\nname='port'\nbase=4096\nsize=8"
    ));
    let case = &c.cases[0];
    let mut b = TestBus::new(&c, case).unwrap();
    assert_eq!(b.read32(4096).unwrap(), 0x12345678);
    b.write16(4100, 0xabcd).unwrap();
    assert!(b.expectations(case).is_empty());
    assert_eq!(b.devices[0].consumed, 1); // 残応答は不一致にしない。
    assert!(b.write32(4102, 1).is_err());
    assert_eq!(b.devices[0].trace.len(), 2);
    let mut reset = TestBus::new(&c, case).unwrap();
    assert_eq!(reset.devices[0].consumed, 0);
    assert_eq!(reset.read32(4096).unwrap(), 0x12345678);
}
#[test]
fn response_exhaustion_and_width_mismatch_have_no_effect() {
    let c = mock_config("");
    let mut b = TestBus::new(&c, &c.cases[0]).unwrap();
    assert!(b.read32(4096).is_err());
    assert!(b.mismatch.as_ref().unwrap().contains("response_position=0"));
    assert!(b.devices[0].trace.is_empty());
    let c = config(&format!(
        "{BASIC}\n[[cases.mmio]]\ndevice='port'\nread_responses=[{{width=2,value=9}}]\n[[mmio]]\nname='port'\nbase=4096\nsize=8"
    ));
    let mut b = TestBus::new(&c, &c.cases[0]).unwrap();
    assert!(b.read32(4096).is_err());
    assert_eq!(b.devices[0].consumed, 0);
    assert!(b.devices[0].trace.is_empty());
    assert_eq!(b.read16(4096).unwrap(), 9);
}
#[test]
fn ram_byte_order_full_range_atomicity_and_reset() {
    let c = config(BASIC);
    let mut b = TestBus::new(&c, &c.cases[0]).unwrap();
    b.ram[100..104].copy_from_slice(&[0x78, 0x56, 0x34, 0x12]);
    assert_eq!(b.read32(100).unwrap(), 0x12345678);
    b.write32(100, 0xaabbccdd).unwrap();
    assert_eq!(&b.ram[100..104], &[0xdd, 0xcc, 0xbb, 0xaa]);
    b.ram[1023] = 5;
    assert!(b.write16(1023, 0).is_err());
    assert_eq!(b.ram[1023], 5);
    let reset = TestBus::new(&c, &c.cases[0]).unwrap();
    assert!(reset.ram.iter().all(|b| *b == 0));
}
#[test]
fn last_allowed_instruction_prioritizes_normal_and_fault() {
    let c = config(BASIC);
    for (word, kind) in [(0xc0000000u32, 0), (0xe0000000, 1), (0xc000ffff, 2)] {
        let mut b = TestBus::new(&c, &c.cases[0]).unwrap();
        b.ram[..4].copy_from_slice(&word.to_le_bytes());
        let mut cpu = Cpu::new();
        let (stop, steps) = runner::execute(&mut cpu, &mut b, 4, 1);
        assert_eq!(steps, 1);
        assert!(matches!(
            (stop, kind),
            (Stop::Normal, 0) | (Stop::Fault(_), 1) | (Stop::Limit, 2)
        ));
    }
}
#[test]
fn case_index_arguments_and_initial_bytes_are_little_endian() {
    let c = config(&format!(
        "{BASIC}\nram_initial=[{{address=704,bytes=[120,86,52,18]}}]\nram_expected=[{{address=700,bytes=[120,86,52,18,120,86,52,18]}}]"
    ));
    let p = program(&[0x710003f8, 0xb10002bc, jump(64, 0xfffffffc)]);
    let result = runner::run(&c, &c.cases[0], 0x12345678, &p, 0xfffffffc);
    assert_eq!(result.status, Status::Pass, "{}", result.reason);
    assert_eq!(result.steps, 3);
}
#[test]
fn mock_failure_overrides_matching_fault_expectation() {
    let c = config(&format!(
        "{BASIC}\nexpect_fault={{cpu_kind='BusFault',function='checks/cases.Run'}}\n[[mmio]]\nname='port'\nbase=4096\nsize=4"
    ));
    let p = program(&[0x71001000]);
    let result = runner::run(&c, &c.cases[0], 0, &p, 0xfffffffc);
    assert_eq!(result.status, Status::Fail);
    assert!(result.reason.contains("応答枯渇"));
}
#[test]
fn fault_conditions_all_match_and_absent_access_fields_fail() {
    let c = config(&format!(
        "{BASIC}\nexpect_fault={{cpu_kind='InvalidInstruction',ond_kind='ExplicitTrap',function='checks/cases.Run',source={{path='opaque/../case.ond',line=7,column=3}},reason='reason'}}"
    ));
    let p = program(&[0xe0000000]);
    let f = Fault::InvalidInstruction { faulting_pc: 56 };
    assert!(
        fault::matches(c.cases[0].expect_fault.as_ref().unwrap(), f, &p.debug)
            .unwrap()
            .is_empty()
    );
    let c = config(&format!(
        "{BASIC}\nexpect_fault={{cpu_kind='InvalidInstruction',function='checks/cases.Run',address=56,width=4,operation='fetch'}}"
    ));
    assert_eq!(
        fault::matches(c.cases[0].expect_fault.as_ref().unwrap(), f, &p.debug)
            .unwrap()
            .len(),
        3
    );
}
#[test]
fn fault_lookup_missing_information_is_error_without_fallback() {
    let mut p = program(&[0xe0000000]);
    let f = Fault::InvalidInstruction { faulting_pc: 56 };
    let c = config(&format!(
        "{BASIC}\nexpect_fault={{cpu_kind='InvalidInstruction',source={{path='opaque/../case.ond',line=7}}}}"
    ));
    p.debug.functions[0].source = Some(source());
    p.debug.ranges[0].source = None;
    assert!(fault::matches(c.cases[0].expect_fault.as_ref().unwrap(), f, &p.debug).is_err());
    let c = config(&format!(
        "{BASIC}\nexpect_fault={{cpu_kind='InvalidInstruction',function='checks/cases.Run'}}"
    ));
    assert!(
        fault::matches(c.cases[0].expect_fault.as_ref().unwrap(), f, &p.debug)
            .unwrap()
            .is_empty()
    );
    p.debug.functions.clear();
    assert!(fault::matches(c.cases[0].expect_fault.as_ref().unwrap(), f, &p.debug).is_err());
}
#[test]
fn debug_ranges_use_narrowest_and_wire_order_ties() {
    let mut p = program(&[0xe0000000, 0xe0000000]);
    let mut narrow = p.debug.ranges[0].clone();
    narrow.end = 60;
    narrow.operation = DebugOperation::Source;
    narrow.message = None;
    p.debug.ranges.insert(0, narrow.clone());
    let mut later = narrow;
    later.operation = DebugOperation::BoundsCheck;
    p.debug.ranges.insert(1, later);
    assert_eq!(
        fault::range_at(&p.debug, 56).unwrap().operation,
        DebugOperation::Source
    );
    assert_eq!(
        fault::range_at(&p.debug, 60).unwrap().operation,
        DebugOperation::ExplicitTrap
    );
}
#[test]
fn debug_rejects_invalid_build_source_ranges_and_frames() {
    let p = program(&[0xe0000000]);
    assert!(validate_debug(&p.image, &p.debug, &p.bytes).is_ok());
    let mut d = p.debug.clone();
    d.build_id = "0".repeat(64);
    assert!(validate_debug(&p.image, &d, &p.bytes).is_err());
    let mut d = p.debug.clone();
    d.functions[0].frame_size = 4;
    assert!(validate_debug(&p.image, &d, &p.bytes).is_err());
    let mut d = p.debug.clone();
    d.ranges[0].source.as_mut().unwrap().line = 0;
    assert!(validate_debug(&p.image, &d, &p.bytes).is_err());
    let mut d = p.debug.clone();
    d.ranges[0].message = None;
    assert!(validate_debug(&p.image, &d, &p.bytes).is_err());
    let mut d = p.debug.clone();
    d.ranges[0].function = "no-function-record-required".into();
    assert!(validate_debug(&p.image, &d, &p.bytes).is_ok());
    let mut d = p.debug.clone();
    d.functions.push(d.functions[0].clone());
    assert!(validate_debug(&p.image, &d, &p.bytes).is_err());
    let mut d = p.debug.clone();
    d.functions[0].unwind.push(ond_protocol::debug::UnwindRow {
        address: 56,
        cfa_sp_offset: 8,
        return_address: ond_protocol::debug::ReturnAddressLocation::LinkRegister,
    });
    assert!(validate_debug(&p.image, &d, &p.bytes).is_err());
}
#[test]
fn sidecar_closed_schema_required_nullable_and_duplicate_keys() {
    let p = program(&[0xe0000000]);
    let json = serde_json::to_string(&p.debug).unwrap();
    assert!(
        serde_json::from_str::<DebugInfo>(
            &json.replace("\"stack_top\":1016", "\"stack_top\":1016,\"unknown\":0")
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<DebugInfo>(&json.replace(
            "\"stack_top\":1016",
            "\"stack_top\":1016,\"stack_top\":1016"
        ))
        .is_err()
    );
    assert!(serde_json::from_str::<DebugInfo>(&json.replace("\"source\":null,", "")).is_err());
    assert!(
        serde_json::from_str::<DebugInfo>(
            &json.replace("\"stack_top\":1016", "\"stack_top\":1.016e3")
        )
        .is_err()
    );
}
#[test]
fn empty_mock_expectation_differs_from_omitted_stub() {
    let c = mock_config("");
    let mut b = TestBus::new(&c, &c.cases[0]).unwrap();
    b.write8(4096, 1).unwrap();
    assert!(b.expectations(&c.cases[0]).is_empty());
    b.devices[0].expected = Some(&[]);
    assert_eq!(b.expectations(&c.cases[0]).len(), 1);
    assert_eq!(
        b.devices[0].trace,
        vec![Access {
            operation: Operation::Write,
            address: 4096,
            width: 1,
            value: 1
        }]
    );
}

#[test]
fn raw_fault_never_infers_ond_kind_and_null_reason_is_mismatch() {
    let mut p = program(&[0xe0000000]);
    let c = config(&format!(
        "{BASIC}\nexpect_fault={{ond_kind='ExplicitTrap',function='checks/cases.Run'}}"
    ));
    let e = c.cases[0].expect_fault.as_ref().unwrap();
    let bus = kagura::cpu::Fault::BusFault {
        faulting_pc: 56,
        addr: 4096,
        width: kagura::bus::AccessWidth::Word,
        operation: kagura::cpu::AccessOperation::Load,
    };
    assert_eq!(fault::matches(e, bus, &p.debug).unwrap().len(), 1);
    p.debug.ranges[0].operation = DebugOperation::Source;
    p.debug.ranges[0].message = None;
    let invalid = Fault::InvalidInstruction { faulting_pc: 56 };
    assert_eq!(fault::matches(e, invalid, &p.debug).unwrap().len(), 1);
    let c = config(&format!(
        "{BASIC}\nexpect_fault={{cpu_kind='InvalidInstruction',function='checks/cases.Run',reason=''}}"
    ));
    assert_eq!(
        fault::matches(c.cases[0].expect_fault.as_ref().unwrap(), invalid, &p.debug)
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn access_fault_matches_cpu_result_and_debug_information_errors_keep_pc() {
    let c = config(&format!(
        "{BASIC}\nexpect_fault={{cpu_kind='BusFault',function='checks/cases.Run',address=4096,width=4,operation='load'}}"
    ));
    let mut p = program(&[0x71001000]);
    assert_eq!(
        runner::run(&c, &c.cases[0], 0, &p, 0xfffffffc).status,
        Status::Pass
    );
    p.debug.functions.clear();
    let result = runner::run(&c, &c.cases[0], 0, &p, 0xfffffffc);
    assert_eq!(result.status, Status::Error);
    assert_eq!(result.steps, 1);
    assert_eq!(result.pc, 56);
}
#[test]
fn artifact_sidecar_absence_mismatch_and_load_regions_are_errors() {
    let c = config(BASIC);
    let p = program(&[0xe0000000]);
    let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("artifact-tests-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("program.bin");
    std::fs::write(&path, &p.bytes).unwrap();
    // 固有名を使い、前回検証のsidecarを欠落ケースへ流用しない。
    let missing = directory.join(format!(
        "missing-{}.bin",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&missing, &p.bytes).unwrap();
    assert!(Program::read(&missing, &c).is_err());
    std::fs::write(
        path.with_extension("onddebug"),
        serde_json::to_vec(&p.debug).unwrap(),
    )
    .unwrap();
    assert!(Program::read(&path, &c).is_ok());
    let mut d = p.debug.clone();
    d.build_id = "0".repeat(64);
    std::fs::write(
        path.with_extension("onddebug"),
        serde_json::to_vec(&d).unwrap(),
    )
    .unwrap();
    assert!(Program::read(&path, &c).is_err());
    std::fs::write(
        path.with_extension("onddebug"),
        serde_json::to_vec(&p.debug).unwrap(),
    )
    .unwrap();
    let c = config(&format!("{BASIC}\nram_initial=[{{address=0,bytes=[0]}}]"));
    assert!(Program::read(&path, &c).is_err());
}
#[test]
fn generated_imports_are_unique_and_include_all_registered_functions() {
    let c = config(&format!(
        "{BASIC}\n[[cases]]\nname='second'\npackage='checks/cases'\nfunction='Other'"
    ));
    let text = crate::build::source(&c);
    assert_eq!(text.matches("import ").count(), 1);
    assert!(text.contains("p0.Run,p0.Other,"));
    assert!(text.contains("var selected: u32 = *(1016 as *u32)"));
}
