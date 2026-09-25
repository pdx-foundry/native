//! The LLDB strategy: the supervisor's side of the debugger worker. No caller or recipe can
//! substitute another debugger.
#![allow(unsafe_code)]
use crate::{
    protocol::observation::{self, ResumeGrant, WorkerHello, WorkerRequest},
    supervisor::SupervisorError,
    work_directory as files,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Read,
    os::unix::process::{CommandExt, ExitStatusExt},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Debug, Serialize, Deserialize)]
struct Tool {
    path: PathBuf,
    sha256: String,
    python: String,
    lldb: String,
    module: String,
}

fn worker_deadline_seconds(startup_seconds: u64) -> u64 {
    let margin = (startup_seconds / 10).clamp(1, 10);
    startup_seconds.saturating_sub(margin).max(1)
}

fn command_output(command: &mut Command, budget: Duration) -> Result<String, SupervisorError> {
    let deadline = Instant::now() + budget;
    let mut child = command
        .env_remove("PYTHONHOME")
        .env_remove("PYTHONPATH")
        .env_remove("DYLD_INSERT_LIBRARIES")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let stdout = child.stdout.take().unwrap();
    let (send, read) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = send.send(result);
    });
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(SupervisorError("Debugger tool probe timed out".into()));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let bytes = read
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| SupervisorError("Tool probe reader unavailable".into()))??;
    if !status.success() || bytes.len() > 1024 * 1024 {
        return Err(SupervisorError("Debugger tool probe failed".into()));
    }
    String::from_utf8(bytes).map_err(|error| SupervisorError(error.to_string()))
}

/// An LLDB command that prints the embedded Python version, the LLDB version and the `lldb`
/// module path as one `NATIVE=` JSON line, which `discover` reads.
const LLDB_IDENTITY_SCRIPT: &str = concat!(
    "script import json,sys,lldb; ",
    "print('NATIVE='+json.dumps(dict(",
    "python=sys.version,",
    "lldb=lldb.SBDebugger.GetVersionString(),",
    "module=lldb.__file__)))",
);

fn discover() -> Result<Tool, SupervisorError> {
    let path = PathBuf::from(
        command_output(
            Command::new("/usr/bin/xcrun").args(["--find", "lldb"]),
            Duration::from_secs(10),
        )?
        .trim(),
    )
    .canonicalize()?;
    let probe = command_output(
        Command::new(&path).args(["-b", "-x", "-o", LLDB_IDENTITY_SCRIPT, "-o", "quit"]),
        Duration::from_secs(10),
    )?;
    let body = probe
        .lines()
        .find_map(|line| line.strip_prefix("NATIVE="))
        .ok_or_else(|| SupervisorError("LLDB embedded Python unavailable".into()))?;
    let value: serde_json::Value = serde_json::from_str(body)?;
    let text = |name| {
        value[name]
            .as_str()
            .filter(|v| !v.is_empty())
            .map(String::from)
            .ok_or_else(|| SupervisorError("Incomplete LLDB Python probe".into()))
    };
    Ok(Tool {
        sha256: files::sha256(&fs::read(&path)?),
        path,
        python: text("python")?,
        lldb: text("lldb")?,
        module: text("module")?,
    })
}

pub(crate) fn probe_observer() -> Result<(), SupervisorError> {
    discover().map(|_| ())
}

pub(crate) struct Observer {
    output: PathBuf,
    request: WorkerRequest,
    tool: Tool,
    worker: Option<Child>,
    granted: bool,
    pause_generation: u64,
    launched: Option<Instant>,
    pub(crate) exited: Option<i64>,
}

impl Observer {
    pub(in crate::binding) fn prepare(
        setup: crate::binding::platform::ObservationSetup<'_>,
    ) -> Result<Self, SupervisorError> {
        let crate::binding::platform::ObservationSetup {
            work_directory,
            attempt,
            executable,
            registries,
            fault,
            fixture,
            fixture_fault,
            modifiers,
            modifier_fault,
            startup_seconds,
            machine,
            package,
        } = setup;
        let tool = discover()?;
        let source = work_directory.join("source");
        super::lifecycle::private_directory(&source)?;
        let mut source_hashes = BTreeMap::new();
        for (name, bytes) in package {
            files::write_new(&source.join(name), bytes)?;
            source_hashes.insert(name.clone(), files::sha256(bytes));
        }
        files::write_new(&work_directory.join("raw-trace.jsonl"), b"")?;
        files::write_json(&work_directory.join("tool.json"), &tool)?;
        let request = WorkerRequest {
            version: observation::VERSION.into(),
            attempt: attempt.into(),
            game: 0,
            executable: executable
                .to_str()
                .ok_or_else(|| SupervisorError("Non-UTF8 executable path".into()))?
                .into(),
            target: files::sha256(&fs::read(executable)?),
            source_hashes,
            machine: machine.clone(),
            registries: registries.clone(),
            control_registry: fault.map(|fault| fault.registry.clone()),
            control: fixture_fault
                .or(modifier_fault)
                .or_else(|| fault.map(|fault| fault.control))
                .unwrap_or_default(),
            fixture,
            fixture_fault: fixture_fault.is_some(),
            modifiers,
            modifier_fault: modifier_fault.is_some(),
            deadline_seconds: worker_deadline_seconds(startup_seconds),
        };
        Ok(Self {
            output: work_directory.into(),
            request,
            tool,
            worker: None,
            granted: false,
            pause_generation: 0,
            launched: None,
            exited: None,
        })
    }

    pub(crate) fn guard(&self) -> PathBuf {
        self.output.join("source/guard.dylib")
    }

    pub(crate) fn start(&mut self, game: u32) -> Result<(), SupervisorError> {
        if files::sha256(&fs::read(&self.tool.path)?) != self.tool.sha256 {
            return Err(SupervisorError("Selected LLDB changed".into()));
        }
        self.request.game = game;
        files::write_json(&self.output.join("worker-request.json"), &self.request)?;
        let import = format!(
            "command script import {}",
            serde_json::to_string(&self.output.join("source/worker.py"))?
        );
        let mut command = Command::new(&self.tool.path);
        command
            .args([
                "-b",
                "-x",
                "-o",
                &import,
                "-o",
                "script worker.run(lldb.debugger)",
                "-o",
                "quit",
            ])
            .env_remove("PYTHONHOME")
            .env_remove("PYTHONPATH")
            .env_remove("DYLD_INSERT_LIBRARIES")
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .stdin(Stdio::null())
            .stdout(File::create(self.output.join("worker.stdout"))?)
            .stderr(File::create(self.output.join("worker.stderr"))?)
            .process_group(0);
        self.worker = Some(command.spawn()?);
        files::write_json(
            &self.output.join("worker-owned.json"),
            &super::lifecycle::process_identity(self.worker.as_ref().unwrap().id())?,
        )?;
        self.launched = Some(Instant::now());
        Ok(())
    }

    /// Move the worker on by one step and report whether it exited. This grants the resume and
    /// carries out a ready worker-loss fault. It never reaps the worker: the process-group
    /// identity remains reserved until cleanup.
    pub(crate) fn advance_worker(&mut self) -> Result<bool, SupervisorError> {
        let worker = self
            .worker
            .as_ref()
            .ok_or_else(|| SupervisorError("Worker not started".into()))?;
        let pid = worker.id();
        if !self.granted {
            let hello = self.output.join("hello.json");
            if hello.try_exists()? {
                let hello: WorkerHello =
                    serde_json::from_slice(&files::read_bounded(&hello, observation::MAX_RECORD)?)?;
                self.validate_hello(&hello, pid)?;
                files::publish_json(
                    &self.output.join("resume-granted.json"),
                    &ResumeGrant {
                        version: observation::VERSION.into(),
                        attempt: self.request.attempt.clone(),
                        game: self.request.game,
                        worker: pid,
                    },
                )?;
                self.granted = true;
            } else if self.launched.unwrap().elapsed() >= Duration::from_secs(15) {
                return Err(SupervisorError("Worker hello deadline elapsed".into()));
            }
        }
        if matches!(
            self.request.control,
            crate::protocol::session::ObservationControl::WorkerLoss
                | crate::protocol::session::ObservationControl::WorkerLossBeforeActivation
        ) && self.output.join("worker-loss-ready").try_exists()?
        {
            self.kill_group()?;
        }
        if fs::metadata(self.output.join("raw-trace.jsonl"))?.len() > observation::MAX_TRACE as u64
        {
            return Err(SupervisorError(
                "Worker trace exceeded storage bound".into(),
            ));
        }
        for name in [
            "worker.stdout",
            "worker.stderr",
            "game.stdout",
            "game.stderr",
        ] {
            if fs::metadata(self.output.join(name))?.len() > observation::MAX_TRACE as u64 {
                return Err(SupervisorError(format!(
                    "Diagnostic storage bound exceeded: {name}"
                )));
            }
        }
        let exited = worker_exited(pid)?;
        if exited && !self.granted {
            return Err(SupervisorError(
                "Worker exited before an accepted handshake".into(),
            ));
        }
        Ok(exited)
    }

    pub(crate) fn pause_witness(
        &mut self,
    ) -> Result<Option<observation::PauseWitness>, SupervisorError> {
        let path = self.output.join("session-paused.json");
        if !path.try_exists()? {
            return Ok(None);
        }
        self.pause_generation += 1;
        let check = observation::PauseCheck {
            attempt: self.request.attempt.clone(),
            game: self.request.game,
            generation: self.pause_generation,
        };
        let pending = self.output.join("pause-check.pending");
        files::write_json(&pending, &check)?;
        fs::rename(&pending, self.output.join("pause-check.json"))?;
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let witness: observation::PauseWitness =
                serde_json::from_slice(&files::read_bounded(&path, observation::MAX_RECORD)?)?;
            if witness.attempt != self.request.attempt
                || witness.game != self.request.game
                || self.worker.as_ref().map(Child::id) != Some(witness.worker)
                || witness.thread == 0
                || witness
                    .returned
                    .iter()
                    .any(|name| !self.request.registries.contains_key(name))
            {
                return Err(SupervisorError("Invalid session pause witness".into()));
            }
            // A fresh response proves the bound debugger still holds the same stopped frame.
            // macOS Mach debugger suspension does not reliably report the signal-stop SSTOP state.
            if witness.generation == self.pause_generation {
                return Ok(Some(witness));
            }
            if Instant::now() >= deadline {
                return Err(SupervisorError(
                    "Debugger pause confirmation timed out".into(),
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn validate_hello(&self, hello: &WorkerHello, pid: u32) -> Result<(), SupervisorError> {
        if hello.version != observation::VERSION
            || hello.attempt != self.request.attempt
            || hello.game != self.request.game
            || hello.worker != pid
            || hello.target != self.request.target
            || hello.source_hashes != self.request.source_hashes
            || hello.python != self.tool.python
            || hello.lldb != self.tool.lldb
            || hello.module != self.tool.module
        {
            return Err(SupervisorError("Worker hello identity mismatch".into()));
        }
        Ok(())
    }

    fn kill_group(&self) -> Result<(), SupervisorError> {
        let deadline = Instant::now() + Duration::from_secs(1);
        if let Some(worker) = &self.worker {
            if worker_exited(worker.id())?
                && group_members(
                    worker.id(),
                    deadline
                        .saturating_duration_since(Instant::now())
                        .min(Duration::from_secs(1)),
                )?
                .is_empty()
            {
                return Ok(());
            }
            // SAFETY: group leader is our unreaped direct child, so this group ID cannot be reused.
            if unsafe { libc::kill(-(worker.id() as i32), libc::SIGKILL) } < 0 {
                let error = std::io::Error::last_os_error();
                // Darwin gives EPERM for a group whose only members are already exiting. Give
                // that exit the rest of the budget before it counts as a failure.
                if error.raw_os_error() == Some(libc::EPERM) {
                    loop {
                        if worker_exited(worker.id())?
                            && group_members(worker.id(), Duration::from_secs(1))?.is_empty()
                        {
                            return Ok(());
                        }
                        if Instant::now() >= deadline {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(SupervisorError(format!(
                        "Stopping worker group {}: {error}; exited: {:?}; live members: {:?}",
                        worker.id(),
                        worker_exited(worker.id()),
                        group_members(worker.id(), Duration::from_secs(1)),
                    )));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn stop(&mut self) -> Result<(), SupervisorError> {
        let deadline = Instant::now() + Duration::from_secs(5);
        if let Some(worker) = &self.worker
            && files::write_json(&self.output.join("session-release"), &true).is_ok()
        {
            // Let debugserver finish a pending target exit and return it to its real parent.
            // Killing LLDB first can strand a SIGKILLed, Mach-suspended target under PID 1.
            let graceful_deadline = Instant::now() + Duration::from_secs(2);
            while !worker_exited(worker.id())? && Instant::now() < graceful_deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        self.kill_group()?;
        let Some(worker) = &mut self.worker else {
            return Ok(());
        };
        loop {
            if worker_exited(worker.id())?
                && group_members(
                    worker.id(),
                    deadline
                        .saturating_duration_since(Instant::now())
                        .min(Duration::from_secs(1)),
                )?
                .is_empty()
            {
                let status = worker.wait()?;
                self.exited = Some(
                    status
                        .code()
                        .map(i64::from)
                        .unwrap_or_else(|| -i64::from(status.signal().unwrap_or(1))),
                );
                self.worker = None;
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(SupervisorError(
                    "Worker reaping exceeded shutdown budget".into(),
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Observer {
    fn drop(&mut self) {
        let _ = self.kill_group();
    }
}

#[cfg(test)]
pub(crate) fn test_observer(
    root: &std::path::Path,
    command: &mut Command,
    game: u32,
    registry: Option<&str>,
) -> Observer {
    let registries = registry
        .map(|name| {
            BTreeMap::from([(
                name.into(),
                observation::RegistryBinding {
                    name: name.into(),
                    directory: name.into(),
                    load_entry: 0,
                    directory_offset: 0,
                    data_offset: 0,
                    count_offset: 0,
                    key_offset: Some(0),
                    key_unavailable: None,
                    pointer_size: 8,
                    string_tag_offset: 23,
                },
            )])
        })
        .unwrap_or_default();
    Observer {
        output: root.into(),
        request: WorkerRequest {
            version: observation::VERSION.into(),
            attempt: "unit".into(),
            game,
            executable: "/not-used".into(),
            target: "target".into(),
            source_hashes: BTreeMap::new(),
            machine: crate::binding::machine::resolve(object::Architecture::Aarch64).unwrap(),
            registries,
            control_registry: None,
            control: crate::protocol::session::ObservationControl::Normal,
            fixture: None,
            fixture_fault: false,
            modifiers: None,
            modifier_fault: false,
            deadline_seconds: 1,
        },
        tool: Tool {
            path: "/not-used".into(),
            sha256: "unused".into(),
            python: "python".into(),
            lldb: "lldb".into(),
            module: "module".into(),
        },
        worker: Some(command.process_group(0).spawn().unwrap()),
        granted: false,
        pause_generation: 0,
        launched: Some(Instant::now()),
        exited: None,
    }
}

fn worker_exited(pid: u32) -> Result<bool, SupervisorError> {
    // SAFETY: initialized siginfo; WNOWAIT observes only our unreaped direct child.
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    let code = unsafe {
        libc::waitid(
            libc::P_PID,
            pid,
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    if code < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(info.si_pid == pid as i32)
}

fn group_members(group: u32, budget: Duration) -> Result<Vec<u32>, SupervisorError> {
    let inventory = command_output(
        Command::new("/bin/ps").args(["-ax", "-o", "pid=,pgid=,stat="]),
        budget,
    )?;
    let mut members = Vec::new();
    for line in inventory.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() != 3 {
            return Err(SupervisorError("Incomplete worker group inventory".into()));
        }
        let pid = fields[0]
            .parse::<u32>()
            .map_err(|_| SupervisorError("Invalid worker group PID".into()))?;
        let pgid = fields[1]
            .parse::<u32>()
            .map_err(|_| SupervisorError("Invalid worker group ID".into()))?;
        if pgid == group && !fields[2].starts_with('Z') {
            members.push(pid);
        }
    }
    Ok(members)
}

pub(in crate::binding) fn package() -> BTreeMap<String, Vec<u8>> {
    [
        (
            "worker.py",
            include_bytes!("observation/worker.py").as_slice(),
        ),
        (
            "guard.dylib",
            include_bytes!(concat!(env!("OUT_DIR"), "/guard.dylib")).as_slice(),
        ),
    ]
    .into_iter()
    .map(|(name, bytes)| (name.into(), bytes.to_vec()))
    .chain(std::iter::once((
        "protocol.py".into(),
        observation::python_bindings().into_bytes(),
    )))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn worker_deadline_preserves_short_startup_budgets() {
        for (startup, worker) in [(2, 1), (5, 4), (10, 9), (30, 27), (180, 170)] {
            assert_eq!(worker_deadline_seconds(startup), worker);
        }
    }

    fn observer(root: &Path, command: &mut Command) -> Observer {
        test_observer(root, command, 123, None)
    }

    #[test]
    fn cleanup_reaps_natural_exit_without_signalling_an_empty_group() {
        let root = tempfile::tempdir().unwrap();
        let mut observer = observer(root.path(), Command::new("/bin/sh").args(["-c", "exit 0"]));
        let deadline = Instant::now() + Duration::from_secs(2);
        while !worker_exited(observer.worker.as_ref().unwrap().id()).unwrap() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        observer.stop().unwrap();
        assert_eq!(observer.exited, Some(0));
    }

    #[test]
    fn cleanup_kills_running_worker_and_keeps_actual_exit_status() {
        let root = tempfile::tempdir().unwrap();
        let mut observer = observer(root.path(), Command::new("/bin/sleep").arg("30"));
        observer.stop().unwrap();
        assert_eq!(observer.exited, Some(-9));
    }

    #[test]
    fn session_cleanup_allows_orderly_release_and_bounds_an_unresponsive_worker() {
        for cooperative in [true, false] {
            let root = tempfile::tempdir().unwrap();
            let mut command = Command::new("/bin/sh");
            command.current_dir(root.path()).args([
                "-c",
                if cooperative {
                    "while [ ! -f session-release ]; do sleep 0.01; done; exit 0"
                } else {
                    "exec sleep 30"
                },
            ]);
            let mut observer = observer(root.path(), &mut command);
            let started = Instant::now();
            observer.stop().unwrap();
            assert!(started.elapsed() < Duration::from_secs(5));
            assert_eq!(observer.exited, Some(if cooperative { 0 } else { -9 }));
        }
    }

    #[test]
    fn pause_confirmation_rejects_a_foreign_game_identity() {
        let root = tempfile::tempdir().unwrap();
        let mut observer = observer(root.path(), Command::new("/bin/sleep").arg("30"));
        files::write_json(
            &root.path().join("session-paused.json"),
            &observation::PauseWitness {
                attempt: "unit".into(),
                game: 999,
                worker: observer.worker.as_ref().unwrap().id(),
                thread: 7,
                returned: vec!["traditions".into()],
                generation: 0,
            },
        )
        .unwrap();
        assert!(
            observer
                .pause_witness()
                .unwrap_err()
                .to_string()
                .contains("Invalid session pause witness")
        );
        observer.stop().unwrap();
    }

    #[test]
    fn handshake_rejects_every_changed_identity_before_resume() {
        let root = tempfile::tempdir().unwrap();
        let mut observer = observer(root.path(), Command::new("/bin/sleep").arg("30"));
        let pid = observer.worker.as_ref().unwrap().id();
        let hello = serde_json::json!({"version": observation::VERSION, "attempt":"unit", "game":123, "worker":pid, "target":"target", "source_hashes":{}, "python":"python", "lldb":"lldb", "module":"module"});
        assert!(
            observer
                .validate_hello(&serde_json::from_value(hello.clone()).unwrap(), pid)
                .is_ok()
        );
        for field in [
            "version",
            "attempt",
            "target",
            "python",
            "lldb",
            "module",
            "game",
            "worker",
            "source_hashes",
        ] {
            let mut changed = hello.clone();
            changed[field] = match field {
                "game" | "worker" => serde_json::json!(0),
                "source_hashes" => serde_json::json!({"extra":"hash"}),
                _ => serde_json::json!("foreign"),
            };
            assert!(
                observer
                    .validate_hello(&serde_json::from_value(changed).unwrap(), pid)
                    .is_err(),
                "{field}"
            );
        }
        assert!(!root.path().join("resume-granted.json").exists());
        observer.stop().unwrap();
    }
}
