//! The private duration table that travels with one script check.
use super::grammar::registered_grammar;
use crate::DeclarationKind;
use crate::binding::{BoundAnalysis, PreparedDurations, scoped_operand_decoder};
use crate::engine::analysis::{
    declarations::Site,
    durations::{Combination, Group},
    grammar::GrammarResult,
    numeric::NumericFacts,
    scoped_numeric,
};
use crate::protocol::observation::{FixtureStorageBinding, FixtureStorageDecoder};
use crate::protocol::script_check::{DurationReceiver, DurationSlots, MAX_DURATION_RECEIVERS};
use std::collections::BTreeSet;

/// The receivers of the `kind` commands whose names occur as whole words in `text`, at most
/// `MAX_DURATION_RECEIVERS`. It reads the facts that the session prepared at its start, so it
/// never rereads the executable; without them it returns no receivers.
///
/// A name only nominates a receiver; the worker classifies each child by its vtable. A command
/// without an established grammar, or beyond the bound, has no receiver, so its children stay
/// unclassified.
pub(crate) fn duration_receivers(
    analysis: &BoundAnalysis,
    kind: DeclarationKind,
    text: &str,
) -> Vec<DurationReceiver> {
    let Some(PreparedDurations {
        input,
        declarations,
        numeric,
        scoped,
    }) = analysis.prepared_script_durations(kind)
    else {
        return Vec::new();
    };
    let declared: BTreeSet<&str> = declarations
        .sites
        .iter()
        .filter_map(|(_, site)| match site {
            Site::Declared { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    let named: BTreeSet<&str> = text
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .filter(|word| declared.contains(word))
        .collect();

    named
        .into_iter()
        .filter_map(|name| {
            let result = registered_grammar(input, declarations, name)?.ok()?;

            Some(receiver(&result, scoped, numeric))
        })
        .take(MAX_DURATION_RECEIVERS)
        .collect()
}

/// A receiver's observable top-level groups. An incomplete static inventory, or a group without
/// slots, leaves its groups incomplete.
fn receiver(
    result: &GrammarResult,
    scoped: &scoped_numeric::Facts,
    numeric: &NumericFacts,
) -> DurationReceiver {
    let slots: Vec<Option<DurationSlots>> = result
        .durations
        .groups
        .iter()
        .map(|group| group_slots(group, result, scoped, numeric))
        .collect();

    DurationReceiver {
        vtable: result.reader.vtable,
        groups_complete: result.durations_complete()
            && slots.iter().all(Option::is_some)
            && !result.durations.groups.iter().any(|group| {
                super::durations::scoped_literal_overlap(group, result, scoped, numeric)
            }),
        groups: slots.into_iter().flatten().collect(),
    }
}

/// Where a group keeps its count: a 32-bit count slot for a scaled-at-read group, or a scoped
/// operand and a factor slot for a shared-factor group.
fn group_slots(
    group: &Group,
    result: &GrammarResult,
    scoped: &scoped_numeric::Facts,
    numeric: &NumericFacts,
) -> Option<DurationSlots> {
    let offset = u64::try_from(group.destination).ok()?;
    let (decoder, factor_offset) = match group.combination.as_ref().ok()? {
        Combination::ScaledAtRead => (FixtureStorageDecoder::Integer, None),
        Combination::SharedFactor { factor_slot, .. } => {
            let point = result.scoped_destinations.get(&group.destination)?;
            let decoder = scoped_operand_decoder(*point, scoped, numeric)?;

            (decoder, Some(u64::try_from(*factor_slot).ok()?))
        }
    };

    Some(DurationSlots {
        units: group.units.iter().map(|unit| unit.key.clone()).collect(),
        count: FixtureStorageBinding { offset, decoder },
        factor_offset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires STELLARIS_PATH with the exact M45 build"]
    fn m45_checks_carry_the_named_receivers_and_their_count_slots() {
        let native = crate::Native::open(std::env::var_os("STELLARIS_PATH").unwrap()).unwrap();
        let analysis = native.bound().analysis.as_ref().unwrap();
        assert!(
            duration_receivers(analysis, DeclarationKind::Effect, "add_modifier = {}").is_empty()
        );
        analysis.prepare_script_durations().unwrap();
        let text = "set_timed_country_flag = { flag = x days = 7 }\n\
                    add_modifier = { modifier = y months = 2 }\n\
                    no_such_command = yes";

        let receivers = duration_receivers(analysis, DeclarationKind::Effect, text);

        let summary: Vec<_> = receivers
            .iter()
            .map(|receiver| {
                let [group] = receiver.groups.as_slice() else {
                    panic!("one duration group: {receiver:?}");
                };
                let scoped = matches!(
                    group.count.decoder,
                    FixtureStorageDecoder::ScopedNumeric { .. }
                );

                (
                    receiver.groups_complete,
                    group.units.clone(),
                    group.count.offset,
                    scoped,
                    group.factor_offset,
                )
            })
            .collect();
        let units = vec!["days".to_string(), "months".into(), "years".into()];

        assert_eq!(
            summary,
            [
                (true, units.clone(), 0xb0, false, None),
                (false, units, 0xa8, true, Some(0x2b0)),
            ]
        );
    }
}
