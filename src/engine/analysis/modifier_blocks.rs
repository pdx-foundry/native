//! Modifier-block member chains at constructor-proven vtable points. Named leaves retain
//! unresolved readers; the default domain must join one try-member before entry forms or
//! inherited keys are added. No content name selects a path through this method.
pub(crate) mod reference;
pub(crate) mod triggered;

use std::collections::BTreeMap;

use super::{
    declarations::Function,
    discovery::Symbol,
    fields::{
        self, ConcreteReader, DataSection, DispatchInput, KeyReaders, PathOutcome, ReaderJoin,
        RootField, Token, TokenPath,
    },
    numeric::{self, ModifierInput, NumericFacts},
    readers,
    stop::Unresolved,
};
use crate::{GrammarProperty, ReferenceTarget};

pub(crate) struct ModifierBlockInput {
    pub points: BTreeMap<u64, ConcreteReader>,
    pub functions: BTreeMap<u64, Function>,
    pub symbols: Vec<Symbol>,
    pub pointers: BTreeMap<u64, u64>,
    pub data: Vec<DataSection>,
    pub tokens: BTreeMap<i64, Token>,
    pub key_readers: KeyReaders,
    pub serializer_constructors: BTreeMap<u64, i64>,
    pub reader_token_offset: u64,
    pub base_member: String,
    pub base: ModifierInput,
    pub references: BTreeMap<String, Result<ReferenceTarget, Unresolved>>,
}

#[derive(Debug, Clone)]
pub(crate) enum Entry {
    Numeric(String),
    Reference(ReferenceTarget),
}

#[derive(Debug, Clone)]
pub(crate) struct Variant {
    pub fields: Vec<RootField>,
    pub paths: Vec<TokenPath>,
    pub fixed_complete: bool,
    pub entries: GrammarProperty<Vec<Entry>>,
    pub stops: Vec<(Option<String>, Unresolved)>,
}

#[derive(Default)]
pub(crate) struct ModifierBlockFacts {
    pub points: BTreeMap<u64, Result<Variant, Unresolved>>,
}

pub(crate) fn analyze(input: &ModifierBlockInput, numeric: &NumericFacts) -> ModifierBlockFacts {
    let mut dispatch = DispatchInput::command(
        &input.functions,
        &input.symbols,
        &input.data,
        input.reader_token_offset,
        &input.key_readers,
    );
    dispatch.bitwise_updates = true;
    dispatch.serializer_constructors = Some(&input.serializer_constructors);
    ModifierBlockFacts {
        points: input
            .points
            .iter()
            .map(|(&point, reader)| {
                let dispatch = dispatch.with_owner_vtable(Some((point, &input.pointers)));
                (point, variant(input, numeric, &dispatch, &reader.member))
            })
            .collect(),
    }
}

fn variant(
    input: &ModifierBlockInput,
    numeric: &NumericFacts,
    dispatch: &DispatchInput<'_>,
    root: &str,
) -> Result<Variant, Unresolved> {
    let MemberLeaves {
        paths: leaves,
        mut stops,
    } = member_leaves(dispatch, root)?;
    let member_complete = stops.is_empty();
    let mut paths: Vec<_> = leaves
        .iter()
        .filter(|path| path.domain[0] == path.domain[1])
        .cloned()
        .collect();
    let (entries, base_complete) = match default_members(input, numeric, &leaves) {
        Ok(base) => {
            paths.extend(base.paths);
            stops.extend(base.stops.into_iter().map(|stop| (None, stop)));
            (base.entries, base.fixed_complete)
        }
        Err(stop) => {
            stops.push((None, stop));
            (GrammarProperty::Unresolved, false)
        }
    };
    let fixed_complete = base_complete && member_complete;
    let (fields, key_stops) = fixed_fields(&paths, &input.tokens);
    let fixed_complete = fixed_complete && key_stops.is_empty();
    stops.extend(key_stops);
    Ok(Variant {
        fields,
        paths,
        fixed_complete,
        entries,
        stops,
    })
}

struct MemberLeaves {
    paths: Vec<TokenPath>,
    stops: Vec<(Option<String>, Unresolved)>,
}

fn member_leaves(dispatch: &DispatchInput<'_>, root: &str) -> Result<MemberLeaves, Unresolved> {
    let (paths, gaps) = fields::explore_member(dispatch, root);
    let mut stops: Vec<_> = gaps
        .into_iter()
        .map(|_| (None, Unresolved::new("modifier-member-path")))
        .collect();
    let mut pending: Vec<_> = paths
        .into_iter()
        .map(|path| (path, vec![root.to_owned()]))
        .collect();
    let mut leaves = Vec::new();
    let mut visited = 0;
    while let Some((mut path, chain)) = pending.pop() {
        visited += 1;
        if visited > 4096 {
            return Err(Unresolved::new("modifier-block-path-limit"));
        }
        if let PathOutcome::Reader(ReaderJoin::Joined { callee, .. }) = &path.outcome
            && readers::is_member(callee)
        {
            let mut child_chain = chain.clone();
            child_chain.push(callee.clone());
            match fields::follow_member(dispatch, &path, &chain) {
                Ok((children, gaps)) => {
                    stops.extend(
                        gaps.into_iter()
                            .map(|_| (None, Unresolved::new("modifier-member-path"))),
                    );
                    pending.extend(
                        children
                            .into_iter()
                            .map(|child| (child, child_chain.clone())),
                    );
                    continue;
                }
                Err(stop) => path.outcome = PathOutcome::Reader(ReaderJoin::Missing(stop)),
            }
        }
        // A singleton dispatch establishes the key even when its value path is obstructed.
        if path.domain[0] == path.domain[1]
            && let PathOutcome::Gap(stop) = &path.outcome
        {
            path.outcome = PathOutcome::Reader(ReaderJoin::Missing(stop.clone()));
        }
        leaves.push(path);
    }
    leaves.sort_by_key(|path| path.domain);
    Ok(MemberLeaves {
        paths: leaves,
        stops,
    })
}

struct DefaultMembers {
    paths: Vec<TokenPath>,
    fixed_complete: bool,
    entries: GrammarProperty<Vec<Entry>>,
    stops: Vec<Unresolved>,
}

/// Prove the default route before exposing its reference form or inherited base keys.
fn default_members(
    input: &ModifierBlockInput,
    numeric: &NumericFacts,
    leaves: &[TokenPath],
) -> Result<DefaultMembers, Unresolved> {
    let default: Vec<_> = leaves
        .iter()
        .filter(|path| path.domain[0] != path.domain[1])
        .collect();
    let callee = default_callee(&default)?;
    let mut entries = Vec::new();
    let mut stops = Vec::new();
    if callee != input.base_member {
        let target = input
            .references
            .get(callee)
            .ok_or_else(|| Unresolved::new("modifier-block-default"))?
            .clone()?;
        if target == ReferenceTarget::Unresolved {
            stops.push(Unresolved::new("modifier-reference-target"));
        }
        entries.push(Entry::Reference(target));
    }
    let tokens = match numeric::modifier::member_tokens(&input.base) {
        Ok(tokens) => tokens,
        Err(stop) => {
            stops.push(stop);
            return Ok(DefaultMembers {
                paths: vec![],
                fixed_complete: false,
                entries: incomplete_entries(entries),
                stops,
            });
        }
    };
    let paths = inherited_paths(tokens, &default);
    match &numeric.modifier_entry {
        Ok(entry) => entries.push(Entry::Numeric(entry.shared_callee.clone())),
        Err(stop) => stops.push(stop.clone()),
    }
    let entries = if stops.is_empty() {
        GrammarProperty::Known(entries)
    } else {
        incomplete_entries(entries)
    };
    Ok(DefaultMembers {
        paths,
        fixed_complete: true,
        entries,
        stops,
    })
}

fn default_callee<'a>(paths: &[&'a TokenPath]) -> Result<&'a str, Unresolved> {
    let mut callee = None;
    for path in paths {
        let PathOutcome::Reader(ReaderJoin::Joined { callee: name, .. }) = &path.outcome else {
            return Err(Unresolved::new("modifier-block-default"));
        };
        if !path.conditions.is_empty() || callee.is_some_and(|previous| previous != name) {
            return Err(Unresolved::new("modifier-block-default"));
        }
        callee = Some(name.as_str());
    }
    callee.ok_or_else(|| Unresolved::new("modifier-block-default"))
}

fn inherited_paths(
    tokens: numeric::modifier::MemberTokens,
    default: &[&TokenPath],
) -> Vec<TokenPath> {
    let name = ReaderJoin::Missing(Unresolved::new("modifier-name-reader"));
    let data = ReaderJoin::Joined {
        callee: "CReader::Read(int&)".into(),
        arguments: BTreeMap::new(),
        tail: false,
    };
    [(tokens.name, name), (tokens.data, data)]
        .into_iter()
        .filter(|(token, _)| {
            default
                .iter()
                .any(|path| (path.domain[0]..=path.domain[1]).contains(token))
        })
        .map(|(token, join)| TokenPath {
            domain: [token, token],
            conditions: vec![],
            instructions: vec![],
            terminal: 0,
            outcome: PathOutcome::Reader(join),
        })
        .collect()
}

fn incomplete_entries(entries: Vec<Entry>) -> GrammarProperty<Vec<Entry>> {
    if entries.is_empty() {
        GrammarProperty::Unresolved
    } else {
        GrammarProperty::Partial(entries)
    }
}

fn fixed_fields(
    paths: &[TokenPath],
    tokens: &BTreeMap<i64, Token>,
) -> (Vec<RootField>, Vec<(Option<String>, Unresolved)>) {
    let mut stops = Vec::new();
    let (fields, gaps) = fields::fields_and_gaps(paths, tokens);
    for gap in gaps {
        let key = gap
            .path
            .and_then(|index| fields.iter().find(|field| field.paths.contains(&index)))
            .map(|field| field.name.clone());
        let unresolved = gap
            .path
            .and_then(|index| match &paths[index].outcome {
                PathOutcome::Reader(ReaderJoin::Missing(stop)) | PathOutcome::Gap(stop) => {
                    Some(stop.clone())
                }
                _ => None,
            })
            .unwrap_or_else(|| Unresolved::new("modifier-token-name"));
        stops.push((key, unresolved));
    }
    for field in &fields {
        if readers::classify(&field.readers).kind == crate::ReaderKind::Unknown
            && !stops
                .iter()
                .any(|(key, _)| key.as_ref() == Some(&field.name))
        {
            stops.push((
                Some(field.name.clone()),
                Unresolved::new("modifier-reader-kind"),
            ));
        }
    }
    (fields, stops)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::assembler::arm64;

    fn input() -> ModifierBlockInput {
        let root = "Example::ReadMember(CReader&, int)";
        let base = "Base::TryReadMember(CReader&, int)";
        ModifierBlockInput {
            points: BTreeMap::from([(
                0x8000,
                ConcreteReader {
                    read: "read".into(),
                    member: root.into(),
                    family: crate::BlockFamily::Modifier,
                    delegate: None,
                },
            )]),
            functions: BTreeMap::from([(
                0x1000,
                Function {
                    address: 0x1000,
                    code: arm64!(at 0x1000;
                        cmp w2, #7;
                        b.ne extern 0x1018;
                        add x8, x0, #8;
                        mov x0, x1;
                        mov x1, x8;
                        b extern 0x2000;
                        b extern 0x3000
                    ),
                },
            )]),
            symbols: vec![
                Symbol {
                    name: root.into(),
                    address: 0x1000,
                },
                Symbol {
                    name: "CReader::Read(CString&, bool)".into(),
                    address: 0x2000,
                },
                Symbol {
                    name: base.into(),
                    address: 0x3000,
                },
            ],
            pointers: BTreeMap::new(),
            data: vec![],
            tokens: BTreeMap::from([(
                7,
                Token {
                    name: "authored_key".into(),
                    constructor: 0,
                    ambiguous: false,
                },
            )]),
            key_readers: KeyReaders::default(),
            serializer_constructors: BTreeMap::new(),
            reader_token_offset: 0x38,
            base_member: base.into(),
            base: ModifierInput {
                member: vec![],
                insert: vec![],
                names: BTreeMap::new(),
                shared_callee: "CReader::Read(CFixedPoint&)".into(),
            },
            references: BTreeMap::new(),
        }
    }

    #[test]
    fn a_base_shape_mismatch_retains_fixed_keys_but_proves_no_numeric_form() {
        let facts = analyze(&input(), &NumericFacts::default());
        let variant = facts.points[&0x8000].as_ref().unwrap();
        assert_eq!(variant.fields[0].name, "authored_key");
        assert_eq!(
            readers::classify(&variant.fields[0].readers).kind,
            crate::ReaderKind::String
        );
        assert!(matches!(variant.entries, GrammarProperty::Unresolved));
        assert!(
            variant
                .stops
                .iter()
                .any(|(_, stop)| stop.reason == "modifier-numeric-member-flow")
        );
    }

    #[test]
    fn ambiguous_tokens_and_missing_default_routes_remain_gaps() {
        let mut input = input();
        input.tokens.get_mut(&7).unwrap().ambiguous = true;
        input.symbols.retain(|symbol| symbol.address != 0x3000);
        let facts = analyze(&input, &NumericFacts::default());
        let variant = facts.points[&0x8000].as_ref().unwrap();
        assert!(variant.fields.is_empty());
        assert!(!variant.fixed_complete);
        assert!(matches!(variant.entries, GrammarProperty::Unresolved));
        assert!(
            variant
                .stops
                .iter()
                .any(|(_, stop)| stop.reason == "modifier-block-default")
        );
        assert!(
            variant
                .stops
                .iter()
                .any(|(_, stop)| stop.reason == "modifier-token-name")
        );
    }
}
