use super::*;
use crate::protocol::{self, Reply};
use std::{
    process::{Child, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub(super) fn run(
    context: crate::Native,
    hosting: Hosting,
    authorization: operation::Authorization,
    control: Option<(String, operation::ObservationControl)>,
    commands: mpsc::Receiver<DriverCommand>,
    stop: Arc<AtomicU8>,
    state: watch::Sender<State>,
) {
    let result = connect(
        context,
        hosting,
        authorization,
        control,
        commands,
        stop,
        &state,
    );
    state.send_modify(|state| state.finished = Some(result));
}

struct SupervisorChild(Option<Child>);
impl Drop for SupervisorChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            // The owner must finish independent game disposal, even when its caller stops waiting.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
    }
}

fn connect(
    context: crate::Native,
    hosting: Hosting,
    authorization: operation::Authorization,
    control: Option<(String, operation::ObservationControl)>,
    commands: mpsc::Receiver<DriverCommand>,
    stop: Arc<AtomicU8>,
    state: &watch::Sender<State>,
) -> Result<GameReport, GameError> {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| GameError::InvalidOptions(error.to_string()))?
        .as_nanos();
    let output = hosting
        .options
        .retention_directory
        .join(format!("game-{}-{id}", std::process::id()));
    let plan = context.prepare_session(output.clone(), &hosting.options, authorization, control)?;
    if matches!(commands.try_recv(), Err(mpsc::TryRecvError::Disconnected)) {
        return Err(GameError::Supervisor(
            "Startup cancelled before allocation".into(),
        ));
    }
    let mut child = hosting
        .command
        .lock()
        .map_err(|_| GameError::Supervisor("Supervisor command lock poisoned".into()))?
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|error| GameError::Supervisor(error.to_string()))?;
    let mut input = child.stdout.take().expect("piped output");
    let mut output_pipe = Some(child.stdin.take().expect("piped control"));
    let mut owner = SupervisorChild(Some(child));
    let (send, replies) = mpsc::sync_channel(4);
    std::thread::spawn(move || {
        loop {
            let reply = protocol::read::<Reply>(&mut input);
            let last = !reply
                .as_ref()
                .is_ok_and(|reply| !matches!(reply, Reply::Finished(_) | Reply::Rejected(_)));
            if send.send(reply).is_err() || last {
                break;
            }
        }
    });
    protocol::write(
        output_pipe.as_mut().unwrap(),
        &protocol::Hello::current(authorization),
    )?;
    let handshake = replies.recv_timeout(Duration::from_secs(15));
    if !matches!(handshake, Ok(Ok(Reply::Ready))) {
        drop(output_pipe.take());
        // No plan was transmitted; this helper cannot own a game.
        let child = owner.0.as_mut().unwrap();
        let _ = child.kill();
        let _ = child.wait();
        owner.0.take();
        return Err(GameError::Supervisor(
            "Supervisor handshake failed or timed out".into(),
        ));
    }
    protocol::write(output_pipe.as_mut().unwrap(), &plan.request)?;
    let mut deadline = Instant::now() + Duration::from_secs(hosting.options.startup_seconds + 90);
    let mut pending = BTreeMap::new();
    let mut sequence = 0_u64;
    let mut ending = false;
    loop {
        if !ending && stop.load(Ordering::SeqCst) != 0 {
            ending = true;
            let control = if stop.load(Ordering::SeqCst) == 2 {
                operation::Control::Cancel
            } else {
                operation::Control::Close
            };
            protocol::write(
                output_pipe
                    .as_mut()
                    .ok_or_else(|| GameError::Supervisor("Control closed".into()))?,
                &control,
            )?;
            deadline = Instant::now() + Duration::from_secs(90);
        }
        match commands.try_recv() {
            Ok(DriverCommand::Read { name, reply }) if !ending && pending.len() < 16 => {
                sequence += 1;
                pending.insert(sequence, reply);
                protocol::write(
                    output_pipe
                        .as_mut()
                        .ok_or_else(|| GameError::Supervisor("Control closed".into()))?,
                    &operation::Control::ReadRegistry {
                        name,
                        request: sequence,
                    },
                )?;
            }
            Ok(DriverCommand::Read { reply, .. }) => {
                let _ = reply.send(Err(if ending {
                    RegistryError::Closed
                } else {
                    RegistryError::Supervisor("Too many pending registry reads".into())
                }));
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                if output_pipe.take().is_some() {
                    ending = true;
                    deadline = Instant::now() + Duration::from_secs(90);
                }
            }
        }
        if Instant::now() >= deadline {
            return Err(GameError::Supervisor(
                "Supervisor response deadline elapsed; disposal remains unconfirmed".into(),
            ));
        }
        let reply = match replies.recv_timeout(Duration::from_millis(10)) {
            Ok(reply) => reply?,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(GameError::Supervisor(
                    "Supervisor channel closed without a final report".into(),
                ));
            }
        };
        match reply {
            Reply::Started { .. } => {}
            Reply::Paused {
                readiness,
                output: retained,
                registries,
            } => {
                if retained != output || state.borrow().paused.is_some() {
                    return Err(GameError::Supervisor("Unexpected session pause".into()));
                }
                let replay = replay_requests(&output, &registries)?;
                let mut snapshots =
                    replay_results(&replay, authorization == operation::Authorization::Admitted);
                for name in context.registry_names() {
                    snapshots.entry(name).or_insert_with(|| {
                        Err("Startup evidence retention failed for this registry".into())
                    });
                }
                state.send_modify(|state| {
                    state.paused = Some(Paused {
                        readiness,
                        registries: snapshots,
                        replay,
                    })
                });
                if !ending {
                    deadline =
                        Instant::now() + Duration::from_secs(hosting.options.idle_seconds + 90);
                }
            }
            Reply::RegistryRead { request } => {
                let Some(reply) = pending.remove(&request) else {
                    return Err(GameError::Supervisor(
                        "Unexpected read acknowledgement".into(),
                    ));
                };
                let _ = reply.send(Ok(()));
                if !ending {
                    deadline =
                        Instant::now() + Duration::from_secs(hosting.options.idle_seconds + 90);
                }
            }
            Reply::Finished(report) => {
                if report.origin != authorization.origin()
                    || report.composition != context.identity().0
                    || report.output != output
                {
                    return Err(GameError::Supervisor(
                        "Session report identity mismatch".into(),
                    ));
                }
                let replay = replay_requests(&output, &report.registries)?;
                let mut registries =
                    replay_results(&replay, authorization == operation::Authorization::Admitted);
                for name in context.registry_names() {
                    registries.entry(name).or_insert_with(|| {
                        Err("Final evidence retention failed for this registry".into())
                    });
                }
                if let Some(paused) = &state.borrow().paused {
                    for name in paused.registries.keys() {
                        registries.entry(name.clone()).or_insert_with(|| Err("Final evidence retention failed; startup evidence remains separately available".into()));
                    }
                }
                let mut result = GameReport {
                    context: context.identity(),
                    attempt: report.attempt,
                    readiness: state
                        .borrow()
                        .paused
                        .as_ref()
                        .map(|paused| paused.readiness),
                    outcome: report.outcome,
                    disposal: report.disposal,
                    reservation_resolved: report.reservation_resolved,
                    registries,
                    replay,
                    retained: output,
                    diagnostics: report.diagnostics,
                };
                drop(output_pipe.take());
                let until = Instant::now() + Duration::from_secs(2);
                loop {
                    match owner.0.as_mut().unwrap().try_wait() {
                        Ok(Some(status)) => {
                            if !status.success() {
                                result
                                    .diagnostics
                                    .push(format!("Supervisor exit: {status}"));
                            }
                            owner.0.take();
                            break;
                        }
                        Ok(None) if Instant::now() < until => {
                            std::thread::sleep(Duration::from_millis(10))
                        }
                        other => {
                            result
                                .diagnostics
                                .push(format!("Supervisor exit not yet confirmed: {other:?}"));
                            break;
                        }
                    }
                }
                return Ok(result);
            }
            Reply::Rejected(reason) => return Err(GameError::Supervisor(reason)),
            Reply::Ready => return Err(GameError::Supervisor("Repeated handshake".into())),
        }
    }
}
