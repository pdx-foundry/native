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
    assert_eq!(found[0].address, "0x3000");
    assert_eq!(found[0].initial_loader.as_deref(), Some("0x4000"));
    assert!(!found[0].has_named_member_reader);
    assert!(candidates(&[]).is_empty());
}

#[test]
fn template_matching_does_not_require_a_named_reader_or_accept_near_matches() {
    let mut symbols = symbols();
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
    symbols.push(Symbol {
        name: "TSingleObjectGameDatabase<CExampleDatabase, CExample, false>::Init()".into(),
        address: 0x5008,
    });
    assert_eq!(candidates(&symbols)[0].initial_loader, None);
}
