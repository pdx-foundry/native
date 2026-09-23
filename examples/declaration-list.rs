//! Print every effect and trigger declaration with its declared scopes, one per line, then the
//! answer's gaps. Give a name part to print only the declarations whose name contains it.
//!
//! usage: declaration-list <installation> [name-part]
use pdx_native::{Declaration, DeclarationKind, DeclaredScopes, Native};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (installation, filter) = match args.as_slice() {
        [installation] => (installation, ""),
        [installation, filter] => (installation, filter.as_str()),
        _ => return Err("usage: declaration-list <installation> [name-part]".into()),
    };
    let native = Native::open(installation)?;
    for kind in [DeclarationKind::Effect, DeclarationKind::Trigger] {
        let answer = native.declarations(kind)?;
        let matching: Vec<&Declaration> = answer
            .value
            .iter()
            .filter(|declaration| declaration.name.contains(filter))
            .collect();
        println!(
            "== {kind:?}: {} of {} declarations, {:?} ==",
            matching.len(),
            answer.value.len(),
            answer.completeness
        );
        for declaration in matching {
            println!(
                "{}  scopes: {}",
                declaration.name,
                scopes(&declaration.scopes)
            );
        }
        for gap in &answer.gaps {
            println!("gap {:?} {:?}: {}", gap.kind, gap.subject, gap.detail);
        }
    }
    Ok(())
}

fn scopes(scopes: &DeclaredScopes) -> String {
    match scopes {
        DeclaredScopes::Any => "any".into(),
        DeclaredScopes::Listed(listed) => {
            let names: Vec<_> = listed.iter().map(|scope| scope.name.as_str()).collect();
            format!("[{}]", names.join(", "))
        }
        DeclaredScopes::Unresolved => "unresolved".into(),
    }
}
