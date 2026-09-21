//! The thread that talks to the supervisor process for one session.
//!
//! It sends the handshake and the request, passes the caller's controls on, and publishes what
//! the supervisor replies. A failure of the connection is an `Error::Supervisor`: it never says
//! that the game is gone.
use super::*;
use crate::protocol::{self, Reply, session::Control};
use std::{
    process::{Child, Stdio},
    time::{Duration, Instant},
};

/// The supervisor's own budgets, which bound how long the driver waits for a reply.
pub(super) struct Timing {
    pub startup_seconds: u64,
    pub idle_seconds: u64,
}

/// Time that the supervisor may need, beyond its budget, to clean up and report.
const CLEANUP_SECONDS: u64 = 90;

pub(super) fn run(
    supervisor: Command,
    request: SessionRequest,
    timing: Timing,
    commands: mpsc::Receiver<DriverCommand>,
    stop: Arc<AtomicU8>,
    state: watch::Sender<State>,
) {
    let result = connect(supervisor, request, timing, commands, stop, &state)
        .map_err(|error| Error::Supervisor(error.to_string()));
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
    mut supervisor: Command,
    request: SessionRequest,
    timing: Timing,
    commands: mpsc::Receiver<DriverCommand>,
    stop: Arc<AtomicU8>,
    state: &watch::Sender<State>,
) -> Result<Finished, crate::supervisor::SupervisorError> {
    use crate::supervisor::SupervisorError;
    if matches!(commands.try_recv(), Err(mpsc::TryRecvError::Disconnected)) {
        return Err(SupervisorError(
            "Startup cancelled before the supervisor started".into(),
        ));
    }
    let mut child = supervisor
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
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
    protocol::write(output_pipe.as_mut().unwrap(), &protocol::Hello::current())?;
    let handshake = replies.recv_timeout(Duration::from_secs(15));
    if !matches!(handshake, Ok(Ok(Reply::Ready))) {
        drop(output_pipe.take());
        // No request was sent, so this supervisor cannot own a game.
        let child = owner.0.as_mut().unwrap();
        let _ = child.kill();
        let _ = child.wait();
        owner.0.take();
        return Err(SupervisorError(match handshake {
            Ok(Ok(Reply::Rejected(reason))) => reason,
            _ => "Supervisor handshake failed or timed out".into(),
        }));
    }
    protocol::write(output_pipe.as_mut().unwrap(), &request)?;
    let mut deadline =
        Instant::now() + Duration::from_secs(timing.startup_seconds + CLEANUP_SECONDS);
    let mut pending = BTreeMap::new();
    let mut sequence = 0_u64;
    let mut ending = false;
    loop {
        if !ending && stop.load(Ordering::SeqCst) != 0 {
            ending = true;
            let control = if stop.load(Ordering::SeqCst) == 2 {
                Control::Cancel
            } else {
                Control::Close
            };
            protocol::write(
                output_pipe
                    .as_mut()
                    .ok_or_else(|| SupervisorError("Control closed".into()))?,
                &control,
            )?;
            deadline = Instant::now() + Duration::from_secs(CLEANUP_SECONDS);
        }
        match commands.try_recv() {
            Ok(DriverCommand::Read { question, reply }) if !ending && pending.len() < 16 => {
                sequence += 1;
                pending.insert(sequence, reply);
                protocol::write(
                    output_pipe
                        .as_mut()
                        .ok_or_else(|| SupervisorError("Control closed".into()))?,
                    &match question {
                        ReadQuestion::Registry(name) => Control::ReadRegistry {
                            name,
                            request: sequence,
                        },
                        ReadQuestion::Fixture => Control::ReadFixture { request: sequence },
                    },
                )?;
            }
            Ok(DriverCommand::Read { reply, .. }) => {
                let _ = reply.send(Err(if ending {
                    Error::Closed
                } else {
                    Error::Supervisor("Too many pending registry reads".into())
                }));
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                if output_pipe.take().is_some() {
                    ending = true;
                    deadline = Instant::now() + Duration::from_secs(CLEANUP_SECONDS);
                }
            }
        }
        if Instant::now() >= deadline {
            return Err(SupervisorError(
                "Supervisor response deadline elapsed; disposal remains unconfirmed".into(),
            ));
        }
        let reply = match replies.recv_timeout(Duration::from_millis(10)) {
            Ok(reply) => reply?,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(SupervisorError(
                    "Supervisor channel closed without a final report".into(),
                ));
            }
        };
        match reply {
            Reply::Paused {
                readiness,
                registries,
                fixture,
            } => {
                if state.borrow().paused.is_some() {
                    return Err(SupervisorError("Unexpected second pause".into()));
                }
                state.send_modify(|state| {
                    state.paused = Some(Paused {
                        readiness,
                        registries,
                        fixture: *fixture,
                    })
                });
                if !ending {
                    deadline =
                        Instant::now() + Duration::from_secs(timing.idle_seconds + CLEANUP_SECONDS);
                }
            }
            Reply::ObservationRead { request } => {
                let Some(reply) = pending.remove(&request) else {
                    return Err(SupervisorError("Unexpected read acknowledgement".into()));
                };
                let _ = reply.send(Ok(()));
                if !ending {
                    deadline =
                        Instant::now() + Duration::from_secs(timing.idle_seconds + CLEANUP_SECONDS);
                }
            }
            Reply::Finished(report) => {
                let mut finished = Finished {
                    outcome: report.outcome,
                    disposal: report.disposal,
                    reservation_resolved: report.reservation_resolved,
                    diagnostics: report.diagnostics,
                };
                if !report.reservation_resolved {
                    finished
                        .diagnostics
                        .push("The host reservation is not resolved".into());
                }
                drop(output_pipe.take());
                // Give the supervisor a moment to exit, so that the caller leaves no child.
                let until = Instant::now() + Duration::from_secs(2);
                loop {
                    match owner.0.as_mut().unwrap().try_wait() {
                        Ok(Some(status)) => {
                            if !status.success() {
                                finished
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
                            finished
                                .diagnostics
                                .push(format!("Supervisor exit not yet confirmed: {other:?}"));
                            break;
                        }
                    }
                }
                return Ok(finished);
            }
            Reply::Rejected(reason) => return Err(SupervisorError(reason)),
            Reply::Ready => return Err(SupervisorError("Repeated handshake".into())),
        }
    }
}
