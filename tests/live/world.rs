//! World readiness, prepared effects and timed flag expiry on the supported build.
use super::*;
use std::path::PathBuf;

pub(super) fn request() -> pdx_native::WorldRequest {
    pdx_native::WorldRequest {
        save: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/world-m451/fixture.sav"),
        country: "United Nations of Earth".into(),
        effect: String::new(),
        days: 0,
        flags: Vec::new(),
    }
}

pub(super) async fn ready(_native: &Native) -> Outcome {
    let recordings = tempfile::tempdir()?;
    let native = Native::open(std::env::var_os("STELLARIS_PATH").unwrap())?
        .record_answers_to(recordings.path());
    let prepared = request();
    let mut game = native.start_game(options().world(prepared.clone())).await?;
    let mut result = async {
        if game.readiness() != GameReadiness::PausedInWorld {
            return Err("startup pause reported as a world".into());
        }
        let answer = game.observe_world().await?;
        println!("world: {}", serde_json::to_string_pretty(&answer)?);
        if answer.completeness != Completeness::Complete {
            return Err(format!("world observation incomplete: {:?}", answer.gaps).into());
        }
        if answer.value.initial_date != "2200.01.01"
            || answer.value.samples.len() != 1
            || answer.value.executed
        {
            return Err("empty world observation changed the prepared save's date".into());
        }
        if game.observe_world().await? != answer {
            return Err("reading a prepared world observation changed its result".into());
        }
        let recorded = Native::from_recorded_answers(recordings.path())?;
        let mut replay = recorded
            .start_game(GameOptions::new(Command::new("must-not-start")).world(prepared.clone()))
            .await?;
        let mut again = replay.observe_world().await?;
        again.source.basis = answer.source.basis;
        if again != answer || replay.close().await? != Disposal::NotApplicable {
            return Err("recorded world observation differs or started a process".into());
        }
        let mut different = prepared;
        different.days = 1;
        let mut replay = recorded
            .start_game(GameOptions::new(Command::new("must-not-start")).world(different))
            .await?;
        if !matches!(replay.observe_world().await, Err(Error::NotRecorded { .. })) {
            return Err("different world request read an earlier observation".into());
        }
        replay.close().await?;
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

pub(super) async fn expiry(native: &Native) -> Outcome {
    let cases = [
        ("native_650_mixed", "months = 2 days = 3"),
        ("native_650_one", "days = 1"),
        ("native_650_zero", "days = 0"),
        ("native_650_negative", "days = -1"),
        ("native_650_overflow", "years = 5965233"),
    ];
    let mut prepared = request();
    prepared.flags = cases.iter().map(|(name, _)| (*name).into()).collect();
    prepared.effect = cases
        .iter()
        .map(|(name, duration)| {
            format!("set_timed_country_flag = {{ flag = {name} {duration} }}\n")
        })
        .collect();
    prepared.days = 90;
    let original = std::fs::read(&prepared.save)?;
    let source = prepared.save.clone();
    let mut game = native.start_game(options().world(prepared)).await?;
    let mut result = async {
        let answer = game.observe_world().await?;
        if game.readiness() != GameReadiness::PausedInWorld
            || answer.completeness != Completeness::Complete
            || !answer.value.executed
            || answer.value.initial_date != "2200.01.01"
            || answer.value.samples.len() != 91
        {
            return Err(format!("expiry observation incomplete: {answer:?}").into());
        }
        let overflow = 5965233_i32.wrapping_mul(360);
        for sample in &answer.value.samples {
            let day = sample.day;
            let expected = [
                (day < 90).then_some(90 - day as i32),
                (day == 0).then_some(1),
                Some(if day == 0 { 0 } else { -1 }),
                Some(-1),
                Some(overflow),
            ];
            let observed: Vec<_> = sample.flags.iter().map(|flag| flag.remaining).collect();
            if observed != expected {
                return Err(format!(
                    "flag counts on day {day} ({}) were {observed:?}, expected {expected:?}",
                    sample.date
                )
                .into());
            }
        }
        if answer
            .value
            .samples
            .windows(2)
            .any(|pair| pair[0].date == pair[1].date)
        {
            return Err("engine date did not change after a day".into());
        }
        println!("flag expiry: {}", serde_json::to_string(&answer)?);
        if std::fs::read(source)? != original {
            return Err("world observation changed the source save".into());
        }
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

pub(super) async fn failure(native: &Native, control: Fault) -> Outcome {
    match native
        .start_game(
            options()
                .world(request())
                .fault(ObservationTarget::World, control),
        )
        .await
    {
        Err(Error::Startup {
            disposal: Disposal::Confirmed,
            reason,
        }) => {
            let expected = match control {
                Fault::MissingHook => "required world hook missing before resume",
                Fault::WorkerLoss => "WorkerLost",
                Fault::AccessFailure => "world observation access failure control",
                _ => return Err("unsupported world failure control".into()),
            };
            if !reason.contains(expected) {
                return Err(format!("world fault lost its cause: {reason}").into());
            }
            Ok(())
        }
        Err(error) => Err(format!("world fault did not confirm disposal: {error:?}").into()),
        Ok(mut game) => {
            let _ = game.close().await;
            Err("world observation started despite its fault".into())
        }
    }
}

pub(super) async fn cancel(native: &Native) -> Outcome {
    let mut game = native.start_game(options().world(request())).await?;
    game.cancel();
    let mut result = match game.observe_world().await {
        Err(Error::Closed) => Ok(()),
        other => Err(format!("world read after cancel: {other:?}").into()),
    };
    and_close(&mut result, &mut game).await;
    result
}

/// A later command must not run when the prepared effect contains a parser diagnostic.
pub(super) async fn rejected(native: &Native) -> Outcome {
    let mut prepared = request();
    prepared.effect =
        "native_unknown_effect = yes\nset_country_flag = native_650_rejected\n".into();
    prepared.flags = vec!["native_650_rejected".into()];
    prepared.days = 1;
    let mut game = native.start_game(options().world(prepared)).await?;
    let mut result = async {
        let answer = game.observe_world().await?;
        if answer.completeness != Completeness::Partial
            || answer.value.executed
            || answer.value.diagnostics.is_empty()
            || answer.value.samples.len() != 1
            || answer.value.samples[0].date != "2200.01.01"
            || answer.value.samples[0].flags[0].remaining.is_some()
        {
            return Err(format!("rejected effect ran or advanced time: {answer:?}").into());
        }
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

pub(super) async fn wrong_country(native: &Native) -> Outcome {
    let mut prepared = request();
    prepared.country = "Native absent country".into();
    match native.start_game(options().world(prepared)).await {
        Err(Error::Startup {
            reason,
            disposal: Disposal::Confirmed,
        }) if reason.contains("local human country differs") => Ok(()),
        Err(error) => Err(format!("wrong country failure: {error:?}").into()),
        Ok(mut game) => {
            let _ = game.close().await;
            Err("world observation accepted a different country".into())
        }
    }
}
