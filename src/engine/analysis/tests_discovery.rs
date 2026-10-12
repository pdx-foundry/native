//! The template candidate method and the database schedule on small authored inputs.
use crate::engine::analysis::assembler::{Arm64, arm64};
use crate::engine::analysis::decode::decode_arm64;
use crate::engine::analysis::discovery::schedule::{
    ScheduleFacts, ScheduleGap, ScheduleInventory, inventory,
};
use crate::engine::analysis::discovery::{Symbol, candidates};
use std::collections::{BTreeMap, BTreeSet};

fn symbols() -> Vec<Symbol> {
    vec![
        Symbol {
            name: "TSingleObjectGameDatabase<CExampleDatabase, CExample, false>::LoadFile(char const*, bool)".into(),
            address: 0x3000,
        },
        Symbol {
            name: "TSingleObjectGameDatabase<CExampleDatabase, CExample, false>::Init()".into(),
            address: 0x4000,
        },
    ]
}

#[test]
fn a_template_loader_is_a_candidate_with_its_initial_loader() {
    let found = candidates(&symbols());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].database, "CExampleDatabase");
    assert_eq!(found[0].owner_candidate, "CExample");
    assert_eq!(
        found[0].loader_class,
        "TSingleObjectGameDatabase<CExampleDatabase, CExample, false>"
    );
    assert_eq!(found[0].address, 0x3000);
    assert_eq!(found[0].initial_loader, Some(0x4000));
    assert!(candidates(&[]).is_empty());
}

#[test]
fn template_matching_rejects_near_matches_and_a_repeated_initial_loader() {
    let mut symbols = symbols();
    symbols.push(Symbol {
        name: "TSingleObjectGameDatabase<CWrong, CWrong, false>::LoadFile(char*, bool)".into(),
        address: 0x4000,
    });
    assert_eq!(candidates(&symbols).len(), 1);
    symbols.push(Symbol {
        name: "TSingleObjectGameDatabase<CExampleDatabase, CExample, false>::Init()".into(),
        address: 0x5008,
    });
    assert_eq!(candidates(&symbols)[0].initial_loader, None);
}

/// One schedule input: a template row and a custom-loader row, after a stack probe. `edit`
/// changes the code between the table owner's move and the fill.
fn schedule(owner: bool, edit: impl FnOnce(&mut Arm64)) -> ScheduleInventoryResult {
    let mut code = Arm64::at(0x1000);
    code.prologue().load(16, 0x9100);
    arm64!(code; blr x16); // stack probe
    if owner {
        arm64!(code; mov x19, sp);
    }
    edit(&mut code);
    code.address(8, 0x8000).load(9, 0x9000);
    arm64!(code; stp x8, x9, [x19, #0x60]); // "CExampleDatabase", create
    code.load(8, 0x9008);
    arm64!(code; stp x8, xzr, [x19, #0x70]);
    code.load(8, 0x9010).load(9, 0x9018);
    arm64!(code; stp x8, x9, [x19, #0x80]);
    code.address(8, 0x8020).address(9, 0x6000);
    arm64!(code; stp x8, x9, [x19, #0x90]); // "CCustomDatabase", create
    code.address(8, 0x6008);
    arm64!(code; stp x8, xzr, [x19, #0xa0]);
    code.address(8, 0x6010).address(9, 0x6018);
    arm64!(code;
        stp x8, x9, [x19, #0xb0];
        ldrsw x8, [x0, #0x14]; // the fill ends
        str x8, [x19, #0xc0]
    );
    let instructions = decode_arm64(&code.bytes(), 0x1000).unwrap();

    let symbols = [
        ("TGameDatabase<CExampleDatabase>", 0x5000),
        ("CCustomDatabase", 0x6000),
    ]
    .into_iter()
    .flat_map(|(class, start)| {
        [
            "CreateInstance",
            "DestroyInstance",
            "InitInstance",
            "PostReadInitInstance",
        ]
        .into_iter()
        .enumerate()
        .map(move |(index, function)| Symbol {
            name: format!("{class}::{function}()"),
            address: start + 8 * index as u64,
        })
    })
    .collect::<Vec<_>>();
    let facts = ScheduleFacts {
        symbols: &symbols,
        pointers: &(0..4)
            .map(|slot| (0x9000 + 8 * slot, 0x5000 + 8 * slot))
            .collect(),
        strings: &BTreeMap::from([
            (0x8000, "CExampleDatabase".into()),
            (0x8020, "CCustomDatabase".into()),
        ]),
        templates: BTreeSet::from(["CExampleDatabase"]),
    };

    inventory(&instructions, &facts)
}

type ScheduleInventoryResult = Result<ScheduleInventory, ScheduleGap>;

#[test]
fn a_schedule_row_with_no_template_candidate_is_counted_as_unjoined() {
    assert_eq!(
        schedule(true, |_| {}),
        Ok(ScheduleInventory {
            rows: 2,
            unjoined: vec!["CCustomDatabase".into()],
            unresolved: 0,
        })
    );
}

#[test]
fn a_schedule_without_its_table_owner_is_a_gap_not_an_empty_table() {
    assert_eq!(schedule(false, |_| {}), Err(ScheduleGap::NoTableOwner));
    let moved = |code: &mut Arm64| arm64!(code; add x19, x19, #8);
    assert_eq!(schedule(true, moved), Err(ScheduleGap::OwnerChanged));
}
