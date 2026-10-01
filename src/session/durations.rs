//! Attach a command's duration groups, their omitted counts, and gaps for what is not established.
use crate::engine::analysis::{
    durations::{self, Combination, Consumption, Group},
    grammar::GrammarResult,
    numeric::NumericFacts,
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
    numeric: &NumericFacts,
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
    let mut scoped_overlap = false;
    let groups: Vec<Duration> = result
        .durations
        .groups
        .iter()
        .map(|group| {
            let duration = public(group, result, scoped);
            let keys = group
                .units
                .iter()
                .map(|unit| unit.key.as_str())
                .collect::<Vec<_>>()
                .join(", ");

            if scoped_literal_overlap(group, result, scoped, numeric) {
                scoped_overlap = true;
                gap(
                    GapKind::ReaderSemantics,
                    format!(
                        "Duration keys {keys}: a scoped operand may share the count's literal slot; \
                     mixing its selection rules with scaled integer reads is not established \
                     (duration-scoped-literal)."
                    ),
                );
            }

            for (kind, detail) in group_gaps(group, &duration) {
                gap(kind, format!("Duration keys {keys}: {detail}"));
            }

            duration
        })
        .collect();
    if result.nested_durations() {
        gap(
            GapKind::ReaderSemantics,
            "Duration keys in nested blocks are not reported.".into(),
        );
    }

    if let Some(stop) = result.durations.unresolved.first() {
        gap(
            GapKind::ReaderSemantics,
            format!(
                "The code after a key's reader could not be followed ({}), so a duration group \
                 may be missing.",
                stop.reason
            ),
        );
    }

    value.durations = if result.durations_complete() && !scoped_overlap {
        GrammarProperty::Known(groups)
    } else if groups.is_empty() && value.fixed_keys == GrammarProperty::Unresolved {
        GrammarProperty::Unresolved
    } else {
        GrammarProperty::Partial(groups)
    };
}

/// A scaled integer count that overlaps, or may overlap, scoped literal storage. The existing
/// combinations do not describe the operand's preserved references mixed with scaled literals.
pub(super) fn scoped_literal_overlap(
    group: &Group,
    result: &GrammarResult,
    scoped: &Facts,
    numeric: &NumericFacts,
) -> bool {
    if group.combination != Ok(Combination::ScaledAtRead) {
        return false;
    }

    // ScaledAtRead is proved only for a word load and a word owner store.
    let count_end = group.destination.checked_add(4);
    if result.scoped_destinations.iter().any(|(&operand, point)| {
        let Some(Ok(subtype)) = scoped.subtypes.get(point) else {
            return true;
        };
        let Subtype::Numeric {
            literal,
            token_reader,
        } = subtype
        else {
            return false;
        };
        let width = numeric.token_readers.get(token_reader).and_then(|reader| {
            let conversion = match &reader.conversion {
                GrammarProperty::Known(Some(conversion))
                | GrammarProperty::Partial(Some(conversion)) => conversion,
                _ => return None,
            };
            match conversion.width_bits {
                GrammarProperty::Known(bits @ (32 | 64)) => Some(i64::from(bits / 8)),
                _ => None,
            }
        });
        let literal_start = i64::try_from(*literal)
            .ok()
            .and_then(|literal| operand.checked_add(literal));
        let literal_end = literal_start
            .zip(width)
            .and_then(|(start, width)| start.checked_add(width));
        match (count_end, literal_start, literal_end) {
            (Some(count_end), Some(start), Some(end)) => {
                group.destination < end && start < count_end
            }
            _ => true,
        }
    }) {
        return true;
    }

    result
        .fields
        .fields
        .iter()
        .flat_map(|field| &field.readers)
        .any(|join| {
            let crate::engine::analysis::fields::ReaderJoin::Joined { callee, .. } = join else {
                return false;
            };
            if crate::engine::analysis::readers::classify_callee(callee)
                != crate::ReaderKind::ScopedNumeric
            {
                return false;
            }
            let Some(operand) = crate::engine::analysis::readers::destination(join) else {
                return true;
            };
            if let Some(point) = result.scoped_destinations.get(&operand)
                && matches!(scoped.subtypes.get(point), Some(Ok(_)))
            {
                return false;
            }
            // Without a subtype, the literal width is not proved.
            true
        })
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
        (Ok(_), Err(stop)) if stop.reason == "duration-consumption" => gaps.push((
            GapKind::OutsideMethod,
            "what consumes the count is outside this method.".into(),
        )),
        (Ok(_), Err(stop)) => gaps.push((
            GapKind::ReaderSemantics,
            format!(
                "the flag-store countdown is not established ({}).",
                reason(stop)
            ),
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
