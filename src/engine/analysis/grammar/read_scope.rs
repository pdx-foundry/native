//! Scope arguments at command member boundaries.
//!
//! Assumption: outer readers pass the incoming scope unchanged to their member reader.
//! The exact-build samples and config checks are recorded in docs/native/discovery.md.
//! Member instructions still determine whether children inherit or replace that scope.

use super::GrammarInput;
use crate::BlockFamily;
use crate::engine::analysis::{declarations::CommandReader, stop::Unresolved};
use crate::engine::analysis::{
    fields::{self, DispatchInput, PathOutcome, ReaderJoin, TokenPath, Value},
    readers,
};

/// Scope arguments and remaining paths from the member walk.
#[derive(Debug, Default)]
pub struct ReadScopes {
    /// Scope names indexed by mask bit.
    pub names: Option<Vec<String>>,
    /// Whether every member path reached a supported boundary.
    pub complete: bool,
    /// Each child-family boundary and its scope argument, including unknown arguments.
    pub children: Vec<(BlockFamily, Option<Value>)>,
    /// Named-key alternatives, including rejected and unresolved paths.
    pub paths: Vec<TokenPath>,
}

pub(super) fn analyze(input: &GrammarInput, reader: CommandReader, root: &str) -> ReadScopes {
    let mut result = ReadScopes {
        names: input.declarations.scope_names.clone(),
        complete: !input.forms.cut_bodies.contains(&reader.member),
        ..Default::default()
    };
    if let Some(&family) = input.families.get(root) {
        result.children.push((family, Some(Value::EnclosingScope)));
        return result;
    }
    let mut dispatch = DispatchInput::command(
        &input.declarations.functions,
        &input.symbols,
        &input.data,
        input.reader_token_offset,
        &input.key_readers,
    );
    dispatch.scope = Some(Value::EnclosingScope);
    let (paths, gaps) = fields::explore_member(&dispatch, root);
    result.complete &= gaps.is_empty();
    let mut pending: Vec<_> = paths
        .into_iter()
        .map(|path| (path, vec![root.to_string()]))
        .collect();
    let mut visited = 0;
    while let Some((mut path, chain)) = pending.pop() {
        visited += 1;
        if visited > super::PATH_LIMIT {
            stop_pending_paths(
                &mut result,
                std::iter::once(path).chain(pending.drain(..).map(|(path, _)| path)),
            );
            break;
        }
        let PathOutcome::Reader(ReaderJoin::Joined {
            callee, arguments, ..
        }) = &path.outcome
        else {
            result.complete &= matches!(
                path.outcome,
                PathOutcome::Rejected | PathOutcome::Reader(ReaderJoin::Stored { .. })
            );
            result.paths.push(path);
            continue;
        };
        let member = readers::is_member(callee);
        let family = input.families.get(callee).copied().or_else(|| {
            let (_, family) = readers::entry(callee);
            (path.domain[0] != path.domain[1]
                && matches!(family, BlockFamily::Trigger | BlockFamily::Effect))
            .then_some(family)
        });
        if let Some(family) = family {
            result
                .children
                .push((family, readers::scope_argument(callee, arguments).cloned()));
            result.paths.push(path);
            continue;
        }
        if !member {
            result.paths.push(path);
            continue;
        }
        match fields::follow_member(&dispatch, &path, &chain) {
            Ok((children, gaps)) => {
                result.complete &= gaps.is_empty()
                    && !input.symbols.iter().any(|symbol| {
                        symbol.name == *callee && input.forms.cut_bodies.contains(&symbol.address)
                    });
                let mut chain = chain;
                chain.push(callee.clone());
                pending.extend(children.into_iter().map(|child| (child, chain.clone())));
            }
            Err(stop) => {
                result.complete = false;
                path.outcome = PathOutcome::Gap(stop);
                result.paths.push(path);
            }
        }
    }
    result
}

fn stop_pending_paths(result: &mut ReadScopes, paths: impl Iterator<Item = TokenPath>) {
    result.complete = false;
    for mut path in paths {
        path.outcome = PathOutcome::Gap(Unresolved::new("read-scope-path-limit"));
        result.paths.push(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::assembler::{Arm64, arm64};

    fn input(root: Arm64, delegate: Arm64) -> GrammarInput {
        let mut input = super::super::tests::input(root, delegate);
        for symbol in &mut input.symbols {
            symbol.name = symbol.name.replace(
                "::ReadMember(CReader&, int)",
                "::ReadMember(CReader&, int, EScopeType)",
            );
        }
        input.families = input
            .families
            .into_iter()
            .map(|(name, family)| {
                (
                    name.replace(
                        "::ReadMember(CReader&, int)",
                        "::ReadMember(CReader&, int, EScopeType)",
                    ),
                    family,
                )
            })
            .collect();
        input
    }

    #[test]
    fn member_scope_word_copy_does_not_preserve_enclosing_scope() {
        let mut member = Arm64::at(0x3000);
        arm64!(member; mov w3, w3; b extern 0x5000);
        let input = input(member, Arm64::at(0x4000));
        let result = super::super::analyze(&input, 0x10000).unwrap();
        assert_eq!(result.read_scopes.children[0].1, None);
    }

    #[test]
    fn read_scope_path_limit_retains_every_unvisited_alternative() {
        let path = |domain| TokenPath {
            domain,
            conditions: vec![],
            instructions: vec![],
            terminal: 0,
            outcome: PathOutcome::Rejected,
        };
        let mut result = ReadScopes {
            complete: true,
            paths: vec![path([7, 7])],
            ..Default::default()
        };
        stop_pending_paths(&mut result, [path([6, 8]), path([7, 9])].into_iter());
        assert!(!result.complete);
        assert_eq!(result.paths.len(), 3);
        assert!(result.paths[1..].iter().all(|path| matches!(&path.outcome, PathOutcome::Gap(stop) if stop.reason == "read-scope-path-limit")));
        assert_eq!(result.paths[1].domain, [6, 8]);
        assert_eq!(result.paths[2].domain, [7, 9]);
    }

    #[test]
    fn constructor_scope_requires_proof_of_memory_at_member_entry() {
        for overwrite in [false, true] {
            let mut member = Arm64::at(0x3000);
            arm64!(member; ldr x3, [x0, #0x40]; b extern 0x5000);
            let mut input = input(member, Arm64::at(0x4000));
            input.declarations.functions.get_mut(&0x1000).unwrap().code = arm64!(at 0x1000;
                mov w0, #128; bl extern 0x9000;
                mov x19, x0;
                movz x8, #1, lsl #16; movk x8, #0x1000; str x8, [x19];
                mov w8, #0x200; str x8, [x19, #0x40]; ret
            );
            let mut read = Arm64::at(0x2000);
            if overwrite {
                arm64!(read; str xzr, [x0, #0x40]);
            }
            arm64!(read; mov x3, x2; b extern 0x3000);
            input.declarations.functions.get_mut(&0x2000).unwrap().code = read.bytes();
            let result = super::super::analyze(&input, 0x10000).unwrap();
            assert!(matches!(
                result.read_scopes.children[0].1,
                Some(Value::Load(..))
            ));
        }
    }
}
