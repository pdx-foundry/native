use pdx_native::{Engine, GameError, GameOptions, Native, OpenRequest};
use std::{io, process::Command};

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--supervisor") {
        #[cfg(feature = "production")]
        pdx_native::supervisor::serve(io::stdin(), io::stdout())?;
        #[cfg(feature = "maintainer-tools")]
        pdx_native::investigation::serve(io::stdin(), io::stdout())?;
        return Ok(());
    }
    if args.len() < 2 {
        return Err(
            "usage: consumer INSTALLATION EXISTING_RETENTION_DIRECTORY [MODE] [CONTROL_REGISTRY]"
                .into(),
        );
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(async {
        let mode = args.get(2).map(String::as_str).unwrap_or("normal");
        let native = Native::open(OpenRequest {
            installation_hint: args[0].clone().into(),
        })?;
        // Static descriptions are independent of supervisor configuration and live readiness.
        eprintln!("description: {:?}", native.get_registry("traditions")?);
        let mut command = Command::new(std::env::current_exe()?);
        command.arg("--supervisor");
        let mut options = GameOptions::new(args[1].clone().into());
        if mode == "timeout" {
            options.startup_seconds = 1;
        }
        if mode == "idle-timeout" {
            options.idle_seconds = 1;
        }
        #[cfg(feature = "production")]
        let startup = {
            if ![
                "normal",
                "reverse",
                "cancel",
                "caller-loss",
                "timeout",
                "idle-timeout",
                "drop",
                "startup-drop",
                "runtime-shutdown",
                "read-cancel",
                "close-cancel",
                "hold",
            ]
            .contains(&mode)
            {
                return Err("unknown production mode".into());
            }
            let native = native.with_supervisor(command, options)?;
            async move { native.start_game().await }
        };
        #[cfg(feature = "maintainer-tools")]
        let startup = {
            let control = if [
                "normal",
                "reverse",
                "cancel",
                "caller-loss",
                "timeout",
                "idle-timeout",
                "drop",
                "startup-drop",
                "runtime-shutdown",
                "read-cancel",
                "close-cancel",
                "hold",
            ]
            .contains(&mode)
            {
                pdx_native::investigation::ObservationControl::Normal
            } else {
                serde_json::from_value(serde_json::Value::String(mode.into()))?
            };
            let registry = args.get(3).cloned().unwrap_or_else(|| "traditions".into());
            async move {
                pdx_native::investigation::start_game(&native, command, options, registry, control)
                    .await
            }
        };
        let startup = if mode == "startup-drop" {
            let result = {
                tokio::select! {
                    result = startup => Some(result),
                    _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => None,
                }
            };
            match result {
                Some(result) => result,
                None => {
                    tokio::time::sleep(std::time::Duration::from_secs(20)).await;
                    return Ok(());
                }
            }
        } else {
            startup.await
        };
        let mut game = match startup {
            Ok(game) => game,
            Err(GameError::StartupFailed(report)) => {
                println!("{}", serde_json::to_string_pretty(&report)?);
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        };
        eprintln!(
            "readiness: {:?}; registries: {:?}",
            game.readiness(),
            game.registry_availability()
        );
        if mode == "runtime-shutdown" {
            tokio::spawn(async move {
                let _game = game;
                std::future::pending::<()>().await;
            });
            tokio::task::yield_now().await;
            return Ok(());
        }
        if mode == "hold" {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
        if mode == "read-cancel" {
            assert!(
                tokio::time::timeout(
                    std::time::Duration::from_millis(1),
                    game.get_registry_items("traditions"),
                )
                .await
                .is_err()
            );
        }
        if mode == "caller-loss" {
            std::process::exit(0);
        }
        if mode == "drop" {
            drop(game);
            tokio::time::sleep(std::time::Duration::from_secs(20)).await;
            return Ok(());
        }
        if mode == "idle-timeout" {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        } else if mode == "cancel" {
            game.cancel();
        } else {
            let names = if mode == "reverse" {
                ["tradition_categories", "traditions"]
            } else {
                ["traditions", "tradition_categories"]
            };
            let mut observed = std::collections::BTreeMap::new();
            // Every control exercises both query orders, including unavailable registries.
            for name in names.into_iter().chain(names.into_iter().rev()) {
                let answer = game.get_registry_items(name).await;
                let value = match &answer {
                    Ok(snapshot) => serde_json::to_value(snapshot)?,
                    Err(error) => serde_json::Value::String(error.to_string()),
                };
                if let Some(previous) = observed.insert(name, value.clone()) {
                    assert_eq!(previous, value);
                }
                match answer {
                    Ok(snapshot) => {
                        let again = game.get_registry_items(name).await?;
                        assert_eq!(
                            serde_json::to_value(&snapshot)?,
                            serde_json::to_value(again)?
                        );
                        let replay =
                            Engine.replay_registry(game.replay_references()[name].clone())?;
                        let mut live = serde_json::to_value(snapshot)?;
                        live["origin"] = "replay".into();
                        assert_eq!(live, serde_json::to_value(replay)?);
                    }
                    Err(error) => eprintln!("{name}: {error}"),
                }
            }
        }
        if mode == "close-cancel" {
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(1), game.close())
                    .await
                    .is_err()
            );
        }
        let report = game.close().await?;
        assert_eq!(
            serde_json::to_value(&report)?,
            serde_json::to_value(game.close().await?)?
        );
        assert!(matches!(
            game.get_registry_items("traditions").await,
            Err(pdx_native::RegistryError::Closed)
        ));
        println!("{}", serde_json::to_string_pretty(&report)?);
        Ok(())
    });
    drop(runtime);
    if args.get(2).is_some_and(|mode| mode == "runtime-shutdown") {
        std::thread::sleep(std::time::Duration::from_secs(20));
    }
    result
}
