//! Attach a command's duration groups, their omitted counts, and gaps for what is not established.
use crate::engine::analysis::{
    durations::{self, Combination, Consumption, Group},
    fields::ReaderJoin,
    grammar::GrammarResult,
    scoped_numeric::{Facts, Subtype},
    stop::Unresolved,
};
use crate::{
    CommandGrammar, Duration, DurationCombination, DurationConsumption, DurationUnit, Gap, GapKind,
    GapSubject, GrammarProperty,
};

pub(super) fn grammar(
    value: &mut CommandGrammar,
    result: &GrammarResult,
    scoped: &Facts,
    name: &str,
    gaps: &mut Vec<Gap>,
) {
    let mut gap = |kind, detail: String| {
        let gap = Gap {
            kind,
            subject: Some(GapSubject::answer_item(name)),
            detail,
        };

        if !gaps.contains(&gap) {
            gaps.push(gap);
        }
    };
    let groups: Vec<Duration> = result
        .durations
        .iter()
        .map(|group| {
            let duration = public(group, result, scoped);
            let keys = group
                .units
                .iter()
                .map(|unit| unit.key.as_str())
                .collect::<Vec<_>>()
                .join(", ");

            for (kind, detail) in group_gaps(group, &duration) {
                gap(kind, format!("Duration keys {keys}: {detail}"));
            }

            duration
        })
        .collect();
    let every_key_joined = result
        .fields
        .fields
        .iter()
        .flat_map(|field| &field.readers)
        .all(|join| !matches!(join, ReaderJoin::Missing(_)));
    let nested_groups = result
        .nested
        .values()
        .any(|nested| !nested.durations.is_empty());

    if nested_groups {
        gap(
            GapKind::ReaderSemantics,
            "Duration keys in nested blocks are not reported.".into(),
        );
    }

    value.durations = match &value.fixed_keys {
        GrammarProperty::Known(_) if every_key_joined && !nested_groups => {
            GrammarProperty::Known(groups)
        }
        GrammarProperty::Unresolved if groups.is_empty() => GrammarProperty::Unresolved,
        _ => GrammarProperty::Partial(groups),
    };
}

fn public(group: &Group, result: &GrammarResult, scoped: &Facts) -> Duration {
    let combination = match &group.combination {
        Ok(Combination::ScaledAtRead) => GrammarProperty::Known(DurationCombination::ScaledAtRead),
        Ok(Combination::SharedFactor { initial_factor, .. }) => {
            GrammarProperty::Known(DurationCombination::SharedFactor {
                initial_factor: *initial_factor,
            })
        }
        Err(_) => GrammarProperty::Unresolved,
    };
    let established = group.combination.is_ok();
    let units = group
        .units
        .iter()
        .map(|unit| DurationUnit {
            key: unit.key.clone(),
            factor: match &unit.factor {
                Ok(factor) if established => GrammarProperty::Known(*factor),
                Ok(factor) => GrammarProperty::Partial(*factor),
                Err(_) => GrammarProperty::Unresolved,
            },
        })
        .collect();
    let consumption = match group.consumption {
        Ok(Consumption::FlagCountdown) => {
            GrammarProperty::Known(DurationConsumption::FlagCountdown)
        }
        Err(_) => GrammarProperty::Unresolved,
    };
    let omitted_count = omitted_count(group, result, scoped)
        .map_or(GrammarProperty::Unresolved, GrammarProperty::Known);

    Duration {
        units,
        combination,
        omitted_count,
        consumption,
    }
}

/// The initial count: the count slot, or the operand's initial literal times the initial factor.
fn omitted_count(group: &Group, result: &GrammarResult, scoped: &Facts) -> Option<i64> {
    match group.combination {
        Ok(Combination::ScaledAtRead) => durations::word(&group.initial, group.destination),
        Ok(Combination::SharedFactor { initial_factor, .. }) => {
            let point = result.scoped_destinations.get(&group.destination)?;
            let Some(Ok(Subtype::Numeric { literal, .. })) = scoped.subtypes.get(point) else {
                return None;
            };
            let literal = durations::word(&group.initial, group.destination + *literal as i64)?;

            Some((literal as i32).wrapping_mul(initial_factor as i32).into())
        }
        Err(_) => None,
    }
}

fn group_gaps(group: &Group, duration: &Duration) -> Vec<(GapKind, String)> {
    let mut gaps = Vec::new();
    let reason = |stop: &Unresolved| stop.reason;

    match (&group.combination, &group.consumption) {
        (Err(stop), _) => gaps.push((
            GapKind::ReaderSemantics,
            format!("the combination is not established ({}).", reason(stop)),
        )),
        (Ok(_), Err(_)) => gaps.push((
            GapKind::OutsideMethod,
            "what consumes the count is outside this method.".into(),
        )),
        (Ok(_), Ok(Consumption::FlagCountdown)) => gaps.push((
            GapKind::OutsideMethod,
            "how often the flag store updates, and so the expiry date, is not established \
             statically."
                .into(),
        )),
    }

    if group.combination.is_ok() && duration.omitted_count == GrammarProperty::Unresolved {
        gaps.push((
            GapKind::ReaderSemantics,
            "the omitted count is not established.".into(),
        ));
    }

    gaps
}
