//! Define names and value forms from the engine's compiled read helpers.
use std::collections::{BTreeMap, BTreeSet};

use super::decode::Instruction;
use super::evaluate::{Call, Code, Exit, Machine, ReadOnlyData};
use crate::{Define, DefineValueType};

/// Name and revision of this static method.
pub const METHOD: &str = "defines/v1";

/// Which argument positions a reader uses for its key.
#[derive(Debug, Clone, Copy)]
pub enum ReaderSource {
    /// A container reader takes namespace and name in x1 and x2.
    Container,
    /// A table reader takes the name in x1; the helper also holds its namespace literal.
    Table,
}

/// A reader function identified from its engine symbol.
#[derive(Debug, Clone, Copy)]
pub struct ReaderSpec {
    pub value_type: Option<DefineValueType>,
    pub source: ReaderSource,
}

/// One helper's code. A failed decode is kept as one unresolved site.
pub struct ReadSite {
    pub symbol: String,
    pub address: u64,
    pub rows: Result<Vec<Instruction>, &'static str>,
}

/// Executable-derived input for the define inventory.
pub struct DefineInput {
    pub sites: Vec<ReadSite>,
    pub readers: BTreeMap<u64, ReaderSpec>,
    pub strings: BTreeMap<u64, String>,
    pub data: ReadOnlyData,
}

/// One helper's resolved define or the reason it did not resolve.
pub enum SiteOutcome {
    Resolved(Define),
    Unresolved {
        subject: Option<String>,
        reason: &'static str,
    },
}

#[derive(Default)]
struct SiteEvidence {
    found: BTreeSet<(String, String, DefineValueType)>,
    subject_hint: Option<String>,
    failure: Option<&'static str>,
    reader_seen: bool,
}

/// Run every helper independently; one difficult shape does not erase other defines.
pub fn analyze(input: &DefineInput) -> Vec<SiteOutcome> {
    input
        .sites
        .iter()
        .map(|site| analyze_site(input, site))
        .collect()
}

fn analyze_site(input: &DefineInput, site: &ReadSite) -> SiteOutcome {
    let rows = match &site.rows {
        Ok(rows) => rows,
        Err(reason) => {
            return SiteOutcome::Unresolved {
                subject: None,
                reason,
            };
        }
    };
    let evidence = collect_read_evidence(input, site, rows);
    if !evidence.reader_seen {
        return SiteOutcome::Unresolved {
            subject: None,
            reason: "define helper has no recognized read call",
        };
    }
    let subject = evidence
        .found
        .iter()
        .next()
        .map(|(namespace, name, _)| format!("{namespace}.{name}"))
        .or(evidence.subject_hint);
    if evidence.found.len() > 1 {
        return SiteOutcome::Unresolved {
            subject,
            reason: "reader paths yield conflicting names or value types",
        };
    }
    if let Some(reason) = evidence.failure {
        return SiteOutcome::Unresolved { subject, reason };
    }
    let Some((namespace, name, value_type)) = evidence.found.into_iter().next() else {
        return SiteOutcome::Unresolved {
            subject: None,
            reason: "reader call yielded no named value type",
        };
    };
    SiteOutcome::Resolved(Define {
        namespace,
        name,
        value_type,
    })
}

fn collect_read_evidence(
    input: &DefineInput,
    site: &ReadSite,
    rows: &[Instruction],
) -> SiteEvidence {
    let code = Code::from_rows(rows.to_vec());
    let mut evidence = SiteEvidence::default();
    for row in rows {
        if !matches!(row.operation.as_str(), "bl" | "b") {
            continue;
        }
        let Some(target) = address(&row.operands) else {
            continue;
        };
        let Some(spec) = input.readers.get(&target) else {
            continue;
        };
        evidence.reader_seen = true;
        let paths = Machine::new(&code, &input.data).run_paths_to(
            site.address,
            row.address,
            &mut |_, _| Ok(Call::Return(None)),
        );
        if paths.is_empty() {
            evidence.failure = Some("reader call cannot be reached through decoded code");
        }
        for path in paths {
            match path.end {
                Ok(Exit::Reached) => match read_key(input, site, spec, &path.machine) {
                    Some((namespace, name, Some(value_type))) => {
                        evidence.found.insert((namespace, name, value_type));
                    }
                    Some((namespace, name, None)) => {
                        evidence.subject_hint = Some(format!("{namespace}.{name}"));
                        evidence.failure = Some("reader value type is not classified");
                    }
                    _ => evidence.failure = Some("reader key arguments could not be resolved"),
                },
                Err(super::stop::Unresolved {
                    reason: "loop-limit" | "path-limit",
                    ..
                }) => evidence.failure = Some("reader path exceeds the table-search limit"),
                _ => evidence.failure = Some("reader call path could not be followed"),
            }
        }
    }
    evidence
}

fn read_key(
    input: &DefineInput,
    site: &ReadSite,
    spec: &ReaderSpec,
    machine: &Machine<'_>,
) -> Option<(String, String, Option<DefineValueType>)> {
    let string = |register| {
        let pointer = machine.register(register)?;
        input
            .strings
            .get(&pointer)
            .cloned()
            .or_else(|| input.data.string(pointer))
    };
    let name = match spec.source {
        ReaderSource::Container => string(2)?,
        ReaderSource::Table => string(1)?,
    };
    let namespace = (0..31)
        .filter_map(string)
        .find(|namespace| helper_key(&site.symbol) == Some(format!("{namespace}{name}")))?;
    Some((namespace, name, spec.value_type))
}

fn helper_key(symbol: &str) -> Option<String> {
    let helper = symbol
        .strip_prefix("NDefines::CDefineRegistryHelper_")
        .or_else(|| symbol.strip_prefix("NUncheckedDefines::CDefineRegistryHelper_"))?;
    Some(
        helper
            .strip_suffix("::ReadDefine(CDefinesContainer const&)")?
            .into(),
    )
}

fn address(operand: &str) -> Option<u64> {
    let text = operand.strip_prefix("#0x")?;
    u64::from_str_radix(text, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(address: u64, operation: &str, operands: &str) -> Instruction {
        Instruction {
            address,
            bytes: [0; 4],
            operation: operation.into(),
            operands: operands.into(),
        }
    }

    fn input(rows: Vec<Instruction>, value_type: Option<DefineValueType>) -> DefineInput {
        DefineInput {
            sites: vec![ReadSite {
                symbol: "NDefines::CDefineRegistryHelper_NCameraFOV::ReadDefine(CDefinesContainer const&)".into(),
                address: 0x1000,
                rows: Ok(rows),
            }],
            readers: BTreeMap::from([(
                0x5000,
                ReaderSpec { value_type, source: ReaderSource::Container },
            )]),
            strings: BTreeMap::from([
                (0x2100, "NCamera".into()),
                (0x3100, "FOV".into()),
            ]),
            data: ReadOnlyData::default(),
        }
    }

    #[test]
    fn reads_literal_key_and_type_from_the_called_reader() {
        let input = input(
            vec![
                row(0x1000, "adrp", "x1,#0x2000"),
                row(0x1004, "add", "x1,x1,#0x100"),
                row(0x1008, "adrp", "x2,#0x3000"),
                row(0x100c, "add", "x2,x2,#0x100"),
                row(0x1010, "b", "#0x5000"),
            ],
            Some(DefineValueType::Float),
        );
        let result = analyze(&input);
        let SiteOutcome::Resolved(define) = &result[0] else {
            panic!("the direct read resolves");
        };
        assert_eq!(define.namespace, "NCamera");
        assert_eq!(define.name, "FOV");
        assert_eq!(define.value_type, DefineValueType::Float);
    }

    #[test]
    fn an_unclassified_reader_is_a_gap() {
        let input = input(
            vec![
                row(0x1000, "adrp", "x1,#0x2000"),
                row(0x1004, "add", "x1,x1,#0x100"),
                row(0x1008, "adrp", "x2,#0x3000"),
                row(0x100c, "add", "x2,x2,#0x100"),
                row(0x1010, "b", "#0x5000"),
            ],
            None,
        );
        assert!(matches!(
            analyze(&input).as_slice(),
            [SiteOutcome::Unresolved {
                reason: "reader value type is not classified",
                ..
            }]
        ));
    }
}
