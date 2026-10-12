//! The template candidate method on small authored inputs.
use crate::engine::analysis::discovery::{Symbol, candidates};

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
