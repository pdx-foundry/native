//! The registry discovery method on small authored inputs.
use pdx_native::internals::discovery::{
    DiscoveryGapKind, SchedulerLayout, StaticInput, Symbol, candidates, discover, scheduler,
};
use std::collections::BTreeMap;

fn input() -> StaticInput {
    StaticInput {
        symbols: vec![Symbol {name:"TSingleObjectGameDatabase<CExampleDatabase, CExample, false>::LoadFile(char const*, bool)".into(),address:0x3000}],
        code: [0x910003f3u32, 0xb0000008,0xf9003268,0xd0000009,0xf9003669,0xa9077e7f,0xa9087e7f].into_iter().flat_map(u32::to_le_bytes).collect(),
        layout:SchedulerLayout{start:0x1000,end:0x101c,offset:96,stride:48,count:1},
        pointers:BTreeMap::new(),strings:BTreeMap::from([(0x2000,"example".into())]),vtables:BTreeMap::new(),
    }
}
#[test]
fn static_discovery_is_bounded_and_joins_the_schedule_to_its_candidate() {
    let result = discover(&input()).unwrap();
    assert_eq!(result.candidates.len(), 1);
    assert!(!result.candidates[0].has_named_member_reader);
    assert!(result.scheduling[0].recovered);
    assert_eq!(result.scheduling[0].candidates, [0]);
    assert!(
        result
            .gaps
            .iter()
            .any(|g| g.kind == DiscoveryGapKind::UnobservedCandidate && g.candidate == Some(0))
    );
    // The method never claims that it found every registry.
    assert!(
        result
            .gaps
            .iter()
            .any(|g| g.kind == DiscoveryGapKind::UnresolvedHelper)
    );
}
#[test]
fn omissions_and_clobbers_preserve_obligations() {
    let mut input = input();
    assert_eq!(scheduler(&input).unwrap().0[0].status, "recovered");
    input.strings.clear();
    assert_eq!(scheduler(&input).unwrap().0[0].status, "gap");
    input = input_fixture_with_clobber(0x52800008); // mov w8,#0 before the name store
    let (rows, _) = scheduler(&input).unwrap();
    assert!(rows[0].values[0].is_none());
    input = input_fixture_with_clobber(0x94000000); // unknown call clobbers x8
    let (rows, gaps) = scheduler(&input).unwrap();
    assert!(rows[0].values[0].is_none() && !gaps.is_empty());
    input = input_fixture_with_clobber(0x91002273); // add x19,x19,#8 changes table owner
    let result = discover(&input).unwrap();
    assert!(
        result
            .gaps
            .iter()
            .any(|g| g.kind == DiscoveryGapKind::Scheduler)
    );
    let mut input = self::input();
    input.symbols.clear();
    let result = discover(&input).unwrap();
    assert!(result.candidates.is_empty());
    assert_eq!(result.scheduling.len(), 1);
    assert!(
        result
            .gaps
            .iter()
            .any(|g| g.kind == DiscoveryGapKind::OutsideTemplate)
    );
}
fn input_fixture_with_clobber(code: u32) -> StaticInput {
    let mut input = input();
    input.code.splice(8..8, code.to_le_bytes());
    input.layout.end += 4;
    input
}
#[test]
fn template_matching_does_not_require_a_named_reader_or_accept_near_matches() {
    let mut symbols = input().symbols;
    symbols.push(Symbol {
        name: "TSingleObjectGameDatabase<CWrong, CWrong, false>::LoadFile(char*, bool)".into(),
        address: 0x4000,
    });
    assert_eq!(candidates(&symbols).len(), 1);
    symbols.push(Symbol {
        name: "CExample::ReadMember(CReader&, int)".into(),
        address: 0x5000,
    });
    assert!(candidates(&symbols)[0].has_named_member_reader);
}

#[test]
fn removed_scheduler_receiver_is_not_an_empty_success() {
    let mut input = input();
    input.code.drain(..4);
    input.layout.start += 4;
    assert!(
        scheduler(&input)
            .unwrap()
            .0
            .iter()
            .all(|r| r.status == "gap")
    );
}
