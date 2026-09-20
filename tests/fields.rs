use evidence::CaptureOrigin;
use pdx_native::internals::{
    decode::{AnalysisOrigin, AnalysisProvenance, DECODER},
    discovery::{Symbol, candidates},
    fields::{self, FieldDescriptor, FieldInput, Function, PathOutcome, ReaderJoin},
};
use pdx_native::{Engine, ReplayRequest};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs};
fn code(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}
fn branch(from: u64, to: u64, link: bool) -> u32 {
    (if link { 0x94000000 } else { 0x14000000 })
        | (((to as i64 - from as i64) / 4) as u32 & 0x3ffffff)
}
fn reference(path: &str, bytes: &[u8]) -> pdx_native::ArtifactReference {
    pdx_native::ArtifactReference {
        path: path.into(),
        bytes: bytes.len() as u64,
        sha256: format!("{:x}", Sha256::digest(bytes)),
    }
}
fn fixture() -> FieldInput {
    let root = "CExample::ReadMember(CReader&, int)";
    let symbols=vec![
        Symbol{name:"TSingleObjectGameDatabase<CExampleDatabase, CExample, false>::LoadFile(char const*, bool)".into(),address:0x7000},
        Symbol{name:root.into(),address:0x1000},Symbol{name:"GetTokenArray()".into(),address:0x2000},
        Symbol{name:"CToken::CToken(int, char const*)".into(),address:0x3000},Symbol{name:"CReader::Read(int&)".into(),address:0x4000},
        Symbol{name:"CPersistent::ReadMember(CReader&, int)".into(),address:0x5000},Symbol{name:"CReader::ReportUnexpected()".into(),address:0x6000},
    ];
    FieldInput {
        selection: candidates(&symbols).remove(0),
        symbols,
        strings: BTreeMap::from([(0x8000, "new_engine_field".into())]),
        gaps: vec![],
        functions: vec![
            Function {
                name: root.into(),
                address: 0x1000,
                code: code(&[
                    0x71001c5f,
                    0x54000060,
                    0x9100e000,
                    branch(0x100c, 0x5000, false),
                    0x91010008,
                    0xaa0103e0,
                    0xaa0803e1,
                    branch(0x101c, 0x4000, false),
                ]),
            },
            Function {
                name: "GetTokenArray()".into(),
                address: 0x2000,
                code: code(&[
                    0x528000e1,
                    0xd0000022,
                    0x91000042,
                    branch(0x200c, 0x3000, true),
                    0xd65f03c0,
                ]),
            },
            Function {
                name: "CPersistent::ReadMember(CReader&, int)".into(),
                address: 0x5000,
                code: code(&[0xaa0103e0, branch(0x5004, 0x6000, false)]),
            },
        ],
    }
}
fn descriptor(bytes: &[u8]) -> FieldDescriptor {
    FieldDescriptor {
        format: fields::FORMAT.into(),
        capture_origin: CaptureOrigin::Synthetic,
        input: reference("input.json", bytes),
        provenance: AnalysisProvenance {
            executable: "a".repeat(64),
            slice: "b".repeat(64),
            composition: "c".repeat(64),
            implementation: "d".repeat(64),
            method: fields::METHOD.into(),
            decoder: DECODER.into(),
            qualification_records: vec![],
            evidence: vec![],
        },
    }
}
fn derive(input: FieldInput) -> pdx_native::RegistryFieldResult {
    let bytes = serde_json::to_vec(&input).unwrap();
    fields::derive(descriptor(&bytes), &bytes, AnalysisOrigin::Replay).unwrap()
}
fn replace(input: &mut FieldInput, index: usize, word: u32) {
    input.functions[0].code[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
}
#[test]
fn discovers_unknown_name_and_excludes_pivot_and_rejection_tokens() {
    let result = derive(fixture());
    assert_eq!(result.fields.len(), 1, "{:?}", result.gaps);
    assert_eq!(result.fields[0].name, "new_engine_field");
    assert_eq!(result.fields[0].token, 7);
    assert_eq!(result.paths.len(), 3);
    assert!(matches!(
        result.fields[0].readers[0],
        ReaderJoin::Joined { .. }
    ));
    assert_eq!(
        result
            .paths
            .iter()
            .filter(|p| matches!(p.outcome, PathOutcome::Rejected))
            .count(),
        2
    );
    assert!(result.partition_accounted);
    assert!(!result.complete_registry);
}
#[test]
fn unsupported_instruction_preserves_obligation_and_does_not_invent_fields() {
    let mut input = fixture();
    replace(&mut input, 0, 0xd65f03c0);
    let result = derive(input);
    assert!(result.fields.is_empty());
    assert!(result.partition_accounted);
    assert!(matches!(result.paths[0].outcome, PathOutcome::Gap(_)));
}
#[test]
fn clobbered_and_truncated_receivers_cannot_join() {
    for word in [0xaa1f03e0, 0x2a0103e0, 0xf9400020] {
        let mut input = fixture();
        replace(&mut input, 5, word);
        let result = derive(input);
        assert_eq!(result.fields.len(), 1);
        assert!(matches!(
            result.fields[0].readers[0],
            ReaderJoin::Missing { .. }
        ));
    }
}
#[test]
fn unresolved_callee_and_missing_token_name_remain_gaps() {
    let mut input = fixture();
    input.symbols.retain(|s| s.address != 0x4000);
    let result = derive(input);
    assert!(matches!(
        result.fields[0].readers[0],
        ReaderJoin::Missing { .. }
    ));
    let mut input = fixture();
    input.strings.clear();
    let result = derive(input);
    assert!(result.fields.is_empty());
    assert!(result.gaps.iter().any(|g| g.kind == "token-table"));
}
#[test]
fn clobbered_token_constructor_arguments_do_not_reuse_stale_values() {
    let mut input = fixture();
    input.functions[1].code[8..12].copy_from_slice(&0xaa1f03e2u32.to_le_bytes());
    let result = derive(input);
    assert!(result.fields.is_empty());
    assert!(result.gaps.iter().any(|g| g.kind == "token-table"));
}
#[test]
fn unsupported_rejection_body_is_not_a_successful_negative_result() {
    let mut input = fixture();
    input.functions[2].code[0..4].copy_from_slice(&0xd503201fu32.to_le_bytes());
    let result = derive(input);
    assert!(
        !result
            .paths
            .iter()
            .any(|p| matches!(p.outcome, PathOutcome::Rejected))
    );
    assert!(result.gaps.iter().any(|g| g.kind == "reader-join"));
}
#[test]
fn altered_flags_and_external_branch_do_not_silently_drop_token_intervals() {
    for (index, word) in [(0, 0x71001c3f), (1, 0x54000100)] {
        let mut input = fixture();
        replace(&mut input, index, word);
        let result = derive(input);
        assert!(result.fields.is_empty());
        assert!(result.partition_accounted);
        assert!(
            result
                .paths
                .iter()
                .any(|p| matches!(p.outcome, PathOutcome::Gap(_)))
        );
    }
}
#[test]
fn replay_is_exact_and_rejects_damage_missing_inputs_and_unknown_revisions() {
    let root = tempfile::tempdir().unwrap();
    let bytes = serde_json::to_vec(&fixture()).unwrap();
    let desc = descriptor(&bytes);
    fs::write(root.path().join("input.json"), &bytes).unwrap();
    let replay = |desc: &FieldDescriptor| {
        let raw = serde_json::to_vec(desc).unwrap();
        fs::write(root.path().join("descriptor.json"), &raw).unwrap();
        Engine.replay_registry_fields(ReplayRequest {
            artifact_root: root.path().into(),
            descriptor: reference("descriptor.json", &raw),
        })
    };
    let result = replay(&desc).unwrap();
    assert_eq!(result.fields, derive(fixture()).fields);
    assert_eq!(result.paths, derive(fixture()).paths);
    let mut changed = desc.clone();
    changed.provenance.method = "future".into();
    assert!(replay(&changed).is_err());
    changed = desc.clone();
    changed.input.bytes = u64::MAX;
    assert!(replay(&changed).is_err());
    changed = desc.clone();
    changed.input.path = "../input.json".into();
    assert!(replay(&changed).is_err());
    let mut damaged = bytes.clone();
    damaged[0] = b'[';
    fs::write(root.path().join("input.json"), damaged).unwrap();
    assert!(replay(&desc).is_err());
    fs::remove_file(root.path().join("input.json")).unwrap();
    assert!(replay(&desc).is_err());
}
#[test]
fn completeness_cannot_be_supplied_in_recorded_inputs() {
    let mut input = serde_json::to_value(fixture()).unwrap();
    input["complete_registry"] = true.into();
    let bytes = serde_json::to_vec(&input).unwrap();
    assert!(fields::derive(descriptor(&bytes), &bytes, AnalysisOrigin::Replay).is_err());
}

#[test]
fn conflicting_names_for_one_token_never_select_an_arbitrary_name() {
    let mut input = fixture();
    input.strings.insert(0x8010, "conflicting_field".into());
    input.functions[1].code = code(&[
        0x528000e1,
        0xd0000022,
        0x91000042,
        branch(0x200c, 0x3000, true),
        0x528000e1,
        0xd0000022,
        0x91004042,
        branch(0x201c, 0x3000, true),
        0xd65f03c0,
    ]);
    let result = derive(input);
    assert!(result.fields.is_empty());
    assert!(
        result
            .gaps
            .iter()
            .any(|g| g.reason.contains("conflicting names"))
    );
}

#[test]
fn writeback_cannot_preserve_a_stale_token_name_pointer() {
    let mut input = fixture();
    // The post-index store changes x2, even though its first operand is x3.
    input.functions[1].code = code(&[
        0x528000e1,
        0xd0000022,
        0xf8008443,
        branch(0x200c, 0x3000, true),
        0xd65f03c0,
    ]);
    let result = derive(input);
    assert!(result.fields.is_empty());
    assert!(result.gaps.iter().any(|g| g.kind == "token-table"));
}

#[test]
fn cyclic_dispatch_is_bounded_and_visible() {
    let mut input = fixture();
    replace(&mut input, 4, branch(0x1010, 0x1010, false));
    let result = derive(input);
    assert!(result.partition_accounted);
    assert!(
        result
            .paths
            .iter()
            .any(|p| matches!(&p.outcome,PathOutcome::Gap(reason) if reason.contains("cycle")))
    );
    assert!(result.fields.is_empty());
}

#[test]
fn an_unresolved_state_alternative_stays_attached_to_the_field() {
    let mut input = fixture();
    input.functions[0].code = code(&[
        0x71001c5f,
        0x54000060,
        0x9100e000,
        branch(0x100c, 0x5000, false),
        0x340000a3, // cbz w3, 0x1024
        0x91010008,
        0xaa0103e0,
        0xaa0803e1,
        branch(0x1020, 0x4000, false),
        0xd65f03c0,
    ]);
    let result = derive(input);
    assert_eq!(result.fields.len(), 1);
    assert_eq!(result.fields[0].readers.len(), 2);
    assert!(
        result.fields[0]
            .readers
            .iter()
            .any(|j| matches!(j, ReaderJoin::Missing { .. }))
    );
    assert!(
        result.fields[0]
            .readers
            .iter()
            .any(|j| matches!(j, ReaderJoin::Joined { .. }))
    );
}

#[test]
fn a_branch_into_the_constructor_cannot_bypass_argument_provenance() {
    let mut input = fixture();
    // This branch is outside the eight-instruction constructor window.
    let mut words = vec![branch(0x2000, 0x2030, false)];
    words.extend([0xd503201f; 8]);
    words.extend([
        0x528000e1,
        0xd0000022,
        0x91000042,
        branch(0x2030, 0x3000, true),
        0xd65f03c0,
    ]);
    input.functions[1].code = code(&words);
    let result = derive(input);
    assert!(result.fields.is_empty());
    assert!(result.gaps.iter().any(|g| g.kind == "token-table"));
}

#[test]
fn unreachable_token_constructors_are_not_discovered() {
    for skip in [branch(0x2000, 0x2014, false), 0xd65f03c0] {
        let mut input = fixture();
        input.functions[1].code = code(&[
            skip,
            0x528000e1,
            0xd0000022,
            0x91000042,
            branch(0x2010, 0x3000, true),
            0xd65f03c0,
        ]);
        let result = derive(input);
        assert!(result.fields.is_empty());
        assert!(result.gaps.iter().any(|g| g.kind == "token-table"));
    }
}

#[test]
fn known_zero_tests_keep_only_feasible_reader_paths() {
    for (constant, branch_op, has_field) in [
        (0x52800003, 0x340000a3, false),
        (0x52800023, 0x340000a3, true),
        (0x52800003, 0x350000a3, true),
        (0x52800023, 0x350000a3, false),
    ] {
        let mut input = fixture();
        input.functions[0].code = code(&[
            0x71001c5f,
            0x54000060,
            0x9100e000,
            branch(0x100c, 0x5000, false),
            constant,
            branch_op,
            0x91010008,
            0xaa0103e0,
            0xaa0803e1,
            branch(0x1024, 0x4000, false),
            0xd65f03c0,
        ]);
        let result = derive(input);
        assert_eq!(!result.fields.is_empty(), has_field);
        assert_eq!(result.paths.len(), 3, "constant alternatives must not fork");
    }
}

#[test]
fn constant_token_construction_branches_do_not_emit_dead_literals() {
    let mut input = fixture();
    input.functions[1].code = code(&[
        0x52800003, // mov w3,#0
        0x340000a3, // cbz w3,0x2018
        0x528000e1,
        0xd0000022,
        0x91000042,
        branch(0x2014, 0x3000, true),
        0xd65f03c0,
    ]);
    assert!(derive(input).fields.is_empty());
}

#[test]
fn duplicate_constructor_symbols_use_one_address_and_conflicts_remain_unknown() {
    let mut input = fixture();
    let constructor = input
        .symbols
        .iter()
        .find(|s| s.address == 0x3000)
        .unwrap()
        .clone();
    input
        .symbols
        .extend(std::iter::repeat_n(constructor, 10000));
    assert_eq!(derive(input.clone()).fields.len(), 1);
    input.symbols.push(Symbol {
        name: "ambiguous_alias".into(),
        address: 0x3000,
    });
    assert!(derive(input).fields.is_empty());
}

#[test]
fn token_constructor_reachability_stops_at_external_tail_calls() {
    let mut input = fixture();
    input.functions[1].code = code(&[
        0x91440268, // add x8,x19,#0x100,lsl #12
        0x528000e1,
        0xd0000022,
        0x91000042,
        branch(0x2010, 0x3000, true),
        branch(0x2014, 0x9000, false),
        0x52800101, // unreachable constructor for another token
        0xd0000022,
        0x91000042,
        branch(0x2024, 0x3000, true),
        0xd65f03c0,
    ]);
    let result = derive(input);
    assert_eq!(result.fields.len(), 1);
    assert_eq!(result.fields[0].token, 7);
}
