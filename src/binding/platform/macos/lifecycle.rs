//! Darwin ownership primitives. Lifecycle policy belongs to execution.
#![allow(unsafe_code)]
use crate::supervisor::SupervisorError;
use std::{
    ffi::CString,
    fs::{self, File, OpenOptions},
    io, mem,
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
    },
    path::Path,
    ptr,
    time::{Duration, Instant},
};

const NAMESPACE: &str = "/Library/Application Support/PDX Native/instances";

pub(crate) fn available() -> Result<(), SupervisorError> {
    Ok(())
}

use crate::binding::ProcessIdentity;

fn process_info(pid: u32) -> io::Result<libc::proc_bsdinfo> {
    // SAFETY: proc_pidinfo writes at most the provided size into an initialized POD struct.
    unsafe {
        let mut info: libc::proc_bsdinfo = mem::zeroed();
        let size = mem::size_of_val(&info) as i32;
        let read = libc::proc_pidinfo(
            pid as i32,
            libc::PROC_PIDTBSDINFO,
            0,
            &mut info as *mut _ as *mut _,
            size,
        );
        if read != size {
            return Err(io::Error::last_os_error());
        }
        Ok(info)
    }
}

pub(crate) fn process_identity(pid: u32) -> Result<ProcessIdentity, SupervisorError> {
    let info = process_info(pid)?;
    Ok(ProcessIdentity {
        pid,
        started_seconds: info.pbi_start_tvsec,
        started_microseconds: info.pbi_start_tvusec,
    })
}

pub(crate) fn prepare_owner(controller: u32) -> Result<(), SupervisorError> {
    if process_info(std::process::id())?.pbi_ppid != controller {
        return Err(SupervisorError(
            "Supervisor must be a direct child of its controller at handshake".into(),
        ));
    }
    // SAFETY: setsid has no pointer arguments. A new session isolates caller terminal signals.
    if unsafe { libc::setsid() } < 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(())
}

pub(crate) fn conflicting_game(owned: Option<u32>) -> Result<bool, SupervisorError> {
    use std::{
        io::Read,
        process::{Command, Stdio},
    };
    // ps uses Darwin's system-wide process inventory, which includes protected system
    // processes that deny PROC_PIDTBSDINFO. Do not confuse those with invisible game PIDs.
    let mut command = Command::new("/bin/ps");
    command
        .args(["-axww", "-o", "pid=,comm="])
        .env_clear()
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn()?;
    let stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let deadline = Instant::now() + Duration::from_secs(1);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(SupervisorError("Process inventory timed out".into()));
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let bytes = reader
        .join()
        .map_err(|_| SupervisorError("Process inventory reader lost".into()))??;
    if !status.success() || bytes.is_empty() || bytes.len() > 2 * 1024 * 1024 {
        return Err(SupervisorError("Incomplete process inventory".into()));
    }
    let inventory = std::str::from_utf8(&bytes)
        .map_err(|_| SupervisorError("Unreadable process inventory".into()))?;
    for line in inventory.lines() {
        let line = line.trim();
        let (pid, executable) = line
            .split_once(char::is_whitespace)
            .ok_or_else(|| SupervisorError("Malformed process inventory".into()))?;
        let pid: u32 = pid
            .parse()
            .map_err(|_| SupervisorError("Malformed process identity".into()))?;
        if Some(pid) == owned {
            continue;
        }
        let name = Path::new(executable.trim())
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if name.eq_ignore_ascii_case("stellaris") {
            // A disappearing candidate is conservative conflict evidence too. Never kill it.
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) struct HostReservation {
    _lock: File,
}

pub(crate) fn acquire_reservation() -> Result<HostReservation, SupervisorError> {
    let root = Path::new(NAMESPACE);
    let parent = root.parent().unwrap();
    let metadata = fs::symlink_metadata(parent)?;
    if !metadata.is_dir() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
        return Err(SupervisorError(
            "Reservation parent must be a root-owned protected directory".into(),
        ));
    }
    acquire_at(root)
}

pub(in crate::binding) fn acquire_at(root: &Path) -> Result<HostReservation, SupervisorError> {
    let metadata = fs::symlink_metadata(root)?;
    // SAFETY: geteuid has no arguments and no side effects.
    if !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err(SupervisorError(
            "Reservation directory must be owned by this account with mode 0700".into(),
        ));
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(root.join("lock"))?;
    let metadata = lock.metadata()?;
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err(SupervisorError("Unsafe reservation lock".into()));
    }
    lock.try_lock().map_err(|error| {
        SupervisorError(format!("Host reservation busy or unavailable: {error}"))
    })?;
    Ok(HostReservation { _lock: lock })
}

pub(crate) fn private_directory(path: &Path) -> Result<(), SupervisorError> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new().mode(0o700).create(path)?;
    Ok(())
}

pub(crate) struct OwnedGame {
    pid: i32,
    reaped: bool,
    exit: Option<i64>,
}
impl OwnedGame {
    pub fn exit_status(&self) -> Option<i64> {
        self.exit
    }
    pub fn pid(&self) -> u32 {
        self.pid as u32
    }
    pub fn identity(&self) -> Result<ProcessIdentity, SupervisorError> {
        process_identity(self.pid())
    }
    pub fn suspended(&self) -> Result<bool, SupervisorError> {
        Ok(process_info(self.pid())?.pbi_status == libc::SSTOP)
    }
    pub fn dispose(&mut self, budget: Duration) -> Result<(), SupervisorError> {
        if self.reaped {
            return Ok(());
        }
        // SAFETY: this PID remains our unreaped direct child, so it cannot have been reused.
        // Never reap before the last signal. A consumer must not install a competing reaper.
        if unsafe { libc::kill(self.pid, libc::SIGKILL) } < 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(error.into());
            }
        }
        let deadline = Instant::now() + budget;
        loop {
            let mut status = 0;
            // SAFETY: valid output pointer; WNOHANG keeps cleanup bounded.
            let reaped = unsafe { libc::waitpid(self.pid, &mut status, libc::WNOHANG) };
            if reaped == self.pid && (libc::WIFEXITED(status) || libc::WIFSIGNALED(status)) {
                self.reaped = true;
                self.exit = Some(i64::from(status));
                return Ok(());
            }
            if reaped < 0 {
                let error = io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::EINTR) {
                    return Err(error.into());
                }
            }
            if Instant::now() >= deadline {
                return Err(SupervisorError(
                    "Child reaping exceeded disposal budget".into(),
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for OwnedGame {
    fn drop(&mut self) {
        // Best effort only; explicit disposal supplies the confirmed result.
        if !self.reaped {
            // SAFETY: unreaped direct-child identity is still retained.
            unsafe {
                libc::kill(self.pid, libc::SIGKILL);
            }
        }
    }
}

struct SpawnSettings {
    attributes: libc::posix_spawnattr_t,
    actions: libc::posix_spawn_file_actions_t,
}
impl Drop for SpawnSettings {
    fn drop(&mut self) {
        // SAFETY: both objects were initialized successfully by settings().
        unsafe {
            libc::posix_spawnattr_destroy(&mut self.attributes);
            libc::posix_spawn_file_actions_destroy(&mut self.actions);
        }
    }
}
fn checked(code: i32) -> Result<(), SupervisorError> {
    if code != 0 {
        return Err(io::Error::from_raw_os_error(code).into());
    }
    Ok(())
}
fn cpath(path: &Path) -> Result<CString, SupervisorError> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| SupervisorError("Path contains NUL".into()))
}
fn settings(
    root: &Path,
    output: &Path,
    spawn_preference: i32,
) -> Result<SpawnSettings, SupervisorError> {
    // SAFETY: initialized opaque objects are retained until spawn; CStrings outlive each API
    // call, whose documented file actions copy their path arguments.
    unsafe {
        let mut attributes = mem::zeroed();
        let mut actions = mem::zeroed();
        checked(libc::posix_spawnattr_init(&mut attributes))?;
        if let Err(error) = checked(libc::posix_spawn_file_actions_init(&mut actions)) {
            libc::posix_spawnattr_destroy(&mut attributes);
            return Err(error);
        }
        let mut settings = SpawnSettings {
            attributes,
            actions,
        };
        checked(libc::posix_spawnattr_setflags(
            &mut settings.attributes,
            (libc::POSIX_SPAWN_START_SUSPENDED | libc::POSIX_SPAWN_CLOEXEC_DEFAULT) as i16,
        ))?;
        let mut arch: libc::cpu_type_t = spawn_preference;
        let mut count = 0;
        checked(libc::posix_spawnattr_setbinpref_np(
            &mut settings.attributes,
            1,
            &mut arch,
            &mut count,
        ))?;
        if count != 1 {
            return Err(SupervisorError("ARM64 spawn preference unavailable".into()));
        }
        // Darwin's addchdir_np is present in the deployment target but absent in libc's bindings.
        unsafe extern "C" {
            fn posix_spawn_file_actions_addchdir_np(
                actions: *mut libc::posix_spawn_file_actions_t,
                path: *const libc::c_char,
            ) -> i32;
        }
        checked(posix_spawn_file_actions_addchdir_np(
            &mut settings.actions,
            cpath(root)?.as_ptr(),
        ))?;
        checked(libc::posix_spawn_file_actions_addopen(
            &mut settings.actions,
            0,
            c"/dev/null".as_ptr(),
            libc::O_RDONLY,
            0,
        ))?;
        for (fd, name) in [(1, "game.stdout"), (2, "game.stderr")] {
            checked(libc::posix_spawn_file_actions_addopen(
                &mut settings.actions,
                fd,
                cpath(&output.join(name))?.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
                0o600,
            ))?;
        }
        Ok(settings)
    }
}

/// A suspended child with no guard library, for tests of ownership and disposal.
#[cfg(test)]
pub(crate) fn spawn(
    executable: &Path,
    root: &Path,
    output: &Path,
    spawn_preference: i32,
) -> Result<OwnedGame, SupervisorError> {
    spawn_guarded(executable, root, output, spawn_preference, None)
}

pub(crate) fn spawn_guarded(
    executable: &Path,
    root: &Path,
    output: &Path,
    spawn_preference: i32,
    guard: Option<&Path>,
) -> Result<OwnedGame, SupervisorError> {
    let settings = settings(root, output, spawn_preference)?;
    let executable = cpath(executable)?;
    let profile = output.join("profile");
    let mut userdir = b"-userdir=".to_vec();
    userdir.extend(profile.as_os_str().as_bytes());
    let mut arguments = vec![
        executable.clone(),
        CString::new("-gdpr-compliant").unwrap(),
        CString::new(userdir).map_err(|_| SupervisorError("Invalid profile path".into()))?,
    ];
    if guard.is_some() {
        arguments.push(CString::new("-debug_mode").unwrap());
    }
    let mut argv: Vec<_> = arguments.iter().map(|v| v.as_ptr() as *mut _).collect();
    argv.push(ptr::null_mut());
    let home = cpath(&profile)?;
    let mut env = vec![
        CString::new([b"HOME=".as_slice(), home.as_bytes()].concat()).unwrap(),
        CString::new("PATH=/usr/bin:/bin").unwrap(),
        CString::new("SDL_MAC_BACKGROUND_APP=1").unwrap(),
    ];
    if let Some(guard) = guard {
        env.push(
            CString::new(
                [
                    b"DYLD_INSERT_LIBRARIES=".as_slice(),
                    cpath(guard)?.as_bytes(),
                ]
                .concat(),
            )
            .unwrap(),
        );
    }
    let mut envp: Vec<_> = env.iter().map(|v| v.as_ptr() as *mut _).collect();
    envp.push(ptr::null_mut());
    let mut pid = 0;
    // SAFETY: null-terminated pointer arrays and initialized settings live across posix_spawn.
    checked(unsafe {
        libc::posix_spawn(
            &mut pid,
            executable.as_ptr(),
            &settings.actions,
            &settings.attributes,
            argv.as_ptr(),
            envp.as_ptr(),
        )
    })?;
    Ok(OwnedGame {
        pid,
        reaped: false,
        exit: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn ordinary_game_inventory_keeps_long_paths_and_never_signals_conflicts() {
        let _guard = crate::binding::LIFECYCLE_TEST_LOCK.lock().unwrap();
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("long-installation-path-".repeat(8));
        fs::create_dir(&directory).unwrap();
        let executable = directory.join("stellaris");
        let source = directory.join("fixture.c");
        fs::write(
            &source,
            "#include <unistd.h>\nint main(void) { sleep(30); return 0; }\n",
        )
        .unwrap();
        assert!(
            std::process::Command::new("/usr/bin/cc")
                .arg(&source)
                .arg("-o")
                .arg(&executable)
                .status()
                .unwrap()
                .success()
        );
        let mut ordinary = std::process::Command::new(&executable).spawn().unwrap();
        // A relocated Apple platform executable can be killed asynchronously by code signing.
        // Compile our own harmless fixture and verify it remains running before the inventory.
        std::thread::sleep(Duration::from_millis(100));
        let conflict = conflicting_game(None);
        let alive = ordinary.try_wait().unwrap().is_none();
        ordinary.kill().unwrap();
        ordinary.wait().unwrap();
        assert!(conflict.unwrap());
        assert!(alive);
    }

    #[test]
    fn lock_excludes_second_owner_and_rejects_symlink() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let first = acquire_at(root.path()).unwrap();
        assert!(acquire_at(root.path()).is_err());
        drop(first);
        // Parallel tests can briefly fork with CLOEXEC descriptors before their exec closes
        // them. Require release within a bound, rather than depending on that scheduling gap.
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            match acquire_at(root.path()) {
                Ok(reservation) => {
                    drop(reservation);
                    break;
                }
                Err(error) if Instant::now() >= deadline => {
                    panic!("Lock remained unavailable: {error}")
                }
                Err(_) => std::thread::sleep(Duration::from_millis(5)),
            }
        }
        fs::remove_file(root.path().join("lock")).unwrap();
        std::os::unix::fs::symlink("missing", root.path().join("lock")).unwrap();
        assert!(acquire_at(root.path()).is_err());
    }
}
