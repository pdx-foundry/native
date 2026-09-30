//! Validate a prepared world's observation before it reaches the caller.
use crate::protocol::world::WorldResult;
use crate::supervisor::SupervisorError;
use crate::{
    Answer, Basis, BuildId, Completeness, Gap, GapKind, Source, WorldObservation, WorldRequest,
};

pub(crate) fn answer(
    result: WorldResult,
    request: &WorldRequest,
    attempt: &str,
    game: u32,
    thread: u64,
    build: BuildId,
) -> Result<Answer<WorldObservation>, SupervisorError> {
    let value = result.observation;
    let session_matches = result.attempt == attempt
        && result.game == game
        && result.thread == thread
        && value.country == request.country;
    let effect_state_valid = !value.executed || !request.effect.is_empty();
    let diagnostics_bounded = value.diagnostics.len() <= crate::script::MAX_DIAGNOSTICS
        && value
            .diagnostics
            .iter()
            .all(|message| message.len() <= crate::script::MAX_TEXT_BYTES + 32);
    let expected_samples = if value.executed || request.effect.is_empty() {
        request.days as usize + 1
    } else {
        1
    };
    let samples_match_request = value.samples.len() == expected_samples
        && value.samples.iter().enumerate().all(|(day, sample)| {
            sample.day as usize == day
                && !sample.date.is_empty()
                && sample.date.len() <= 32
                && sample
                    .flags
                    .iter()
                    .map(|flag| &flag.name)
                    .eq(request.flags.iter())
        });
    let initial_date_matches = !value.initial_date.is_empty()
        && value.initial_date.len() <= 32
        && value
            .samples
            .first()
            .is_some_and(|sample| sample.date == value.initial_date);
    if !(session_matches
        && effect_state_valid
        && diagnostics_bounded
        && samples_match_request
        && initial_date_matches)
    {
        return Err(SupervisorError(
            "invalid or foreign prepared world observation".into(),
        ));
    }
    let mut gaps = Vec::new();
    if !value.diagnostics.is_empty() || (!request.effect.is_empty() && !value.executed) {
        gaps.push(Gap { kind: GapKind::IncompleteObservation, subject: None,
            detail: "The prepared effect did not execute, or engine diagnostics occurred during the observation.".into() });
    }
    Ok(Answer {
        completeness: Completeness::from_gaps(&gaps),
        value,
        gaps,
        source: Source::new(build, "observe-world/v1", Basis::LiveObservation),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{WorldFlag, WorldSample};

    fn request() -> WorldRequest {
        WorldRequest {
            save: "/private/fixture.sav".into(),
            country: "Earth".into(),
            effect: "set_timed_country_flag = { flag = native_flag days = 1 }".into(),
            days: 1,
            flags: vec!["native_flag".into()],
        }
    }

    fn observed() -> WorldResult {
        WorldResult {
            attempt: "attempt".into(),
            game: 3,
            thread: 4,
            observation: WorldObservation {
                country: "Earth".into(),
                initial_date: "2200.01.01".into(),
                executed: true,
                diagnostics: vec![],
                samples: vec![
                    WorldSample {
                        day: 0,
                        date: "2200.01.01".into(),
                        flags: vec![WorldFlag {
                            name: "native_flag".into(),
                            remaining: Some(1),
                        }],
                    },
                    WorldSample {
                        day: 1,
                        date: "2200.01.02".into(),
                        flags: vec![WorldFlag {
                            name: "native_flag".into(),
                            remaining: None,
                        }],
                    },
                ],
            },
        }
    }

    fn joined(result: WorldResult) -> Result<Answer<WorldObservation>, SupervisorError> {
        answer(result, &request(), "attempt", 3, 4, BuildId("build".into()))
    }

    #[test]
    fn count_and_absence_are_distinct_world_observations() {
        let answer = joined(observed()).unwrap();
        assert_eq!(answer.completeness, Completeness::Complete);
        assert_eq!(answer.value.samples[0].flags[0].remaining, Some(1));
        assert_eq!(answer.value.samples[1].flags[0].remaining, None);
    }

    #[test]
    fn foreign_country_thread_and_missing_day_cannot_be_joined() {
        let mut country = observed();
        country.observation.country = "Other".into();
        assert!(joined(country).is_err());
        let mut thread = observed();
        thread.thread = 5;
        assert!(joined(thread).is_err());
        let mut skipped_day = observed();
        skipped_day.observation.samples[1].day = 2;
        assert!(joined(skipped_day).is_err());
        let mut missing = observed();
        missing.observation.samples.pop();
        assert!(joined(missing).is_err());
    }

    #[test]
    fn rejected_effect_preserves_initial_sample_and_is_partial() {
        let mut rejected = observed();
        rejected.observation.executed = false;
        rejected.observation.samples.truncate(1);
        rejected
            .observation
            .diagnostics
            .push("validation: wrong scope".into());
        assert_eq!(
            joined(rejected).unwrap().completeness,
            Completeness::Partial
        );
    }
}
