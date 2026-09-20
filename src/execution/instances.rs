//! Host-wide launch exclusion: one Native-owned Stellaris game per host.
//!
//! The supervisor takes an OS lock, then requires every journal entry to be a recognized,
//! disposed record. Unknown versions or states, extra fields, unreadable or truncated records,
//! symlinks, pending writes, and unresolved reservations block a launch and are never rewritten.
//! A reservation is durable before profile allocation or spawn; the child incarnation is added
//! immediately after spawn. Disposal is marked only after direct-child reaping, or when no game
//! was launched.
//!
//! The runtime never clears a reservation from a free lock, an absent PID, or elapsed time.
//! The lock covers Native owners only: a bounded process inventory checks for ordinary Stellaris
//! instances before launch and during the job, and Native signals only its own direct child.
//! The README gives the one-time namespace setup commands.
use crate::{
    binding::{self, HostReservation, ProcessIdentity},
    supervisor::SupervisorError,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const VERSION: u32 = 1;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum State {
    Reserved,
    Disposed,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    version: u32,
    attempt: String,
    owner: ProcessIdentity,
    game: Option<ProcessIdentity>,
    output: PathBuf,
    state: State,
}

fn parse(bytes: &[u8]) -> Result<Journal, SupervisorError> {
    let journal: Journal = serde_json::from_slice(bytes)?;
    if journal.version != VERSION
        || journal.attempt.is_empty()
        || !journal.output.is_absolute()
        || journal.owner.pid == 0
    {
        return Err(SupervisorError("Unrecognized reservation envelope".into()));
    }
    Ok(journal)
}

pub(super) struct Reservation {
    host: HostReservation,
    journal: Journal,
}
impl Reservation {
    pub fn acquire(attempt: String, output: PathBuf) -> Result<Self, SupervisorError> {
        let host = binding::acquire_reservation()?;
        Self::reserve(host, attempt, output)
    }
    pub(super) fn reserve(
        host: HostReservation,
        attempt: String,
        output: PathBuf,
    ) -> Result<Self, SupervisorError> {
        validate_store(&host.root)?;
        let journal = Journal {
            version: VERSION,
            attempt,
            owner: binding::process_identity(std::process::id())?,
            game: None,
            output,
            state: State::Reserved,
        };
        let reservation = Self { host, journal };
        reservation.persist(true)?;
        Ok(reservation)
    }
    pub fn record_game(&mut self, identity: ProcessIdentity) -> Result<(), SupervisorError> {
        self.journal.game = Some(identity);
        self.persist(false)
    }
    pub fn snapshot(&self) -> Result<serde_json::Value, SupervisorError> {
        Ok(serde_json::to_value(&self.journal)?)
    }
    pub fn disposed(&mut self) -> Result<(), SupervisorError> {
        let previous = std::mem::replace(&mut self.journal.state, State::Disposed);
        if let Err(error) = self.persist(false) {
            self.journal.state = previous;
            return Err(error);
        }
        Ok(())
    }
    fn persist(&self, initial: bool) -> Result<(), SupervisorError> {
        let destination = self
            .host
            .root
            .join(format!("{}.json", self.journal.attempt));
        let temporary = self
            .host
            .root
            .join(format!("{}.pending", self.journal.attempt));
        let bytes = serde_json::to_vec(&self.journal)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        if initial {
            fs::hard_link(&temporary, destination)?;
            fs::remove_file(temporary)?;
        } else {
            fs::rename(temporary, destination)?;
        }
        File::open(&self.host.root)?.sync_all()?;
        Ok(())
    }
}

fn validate_store(root: &Path) -> Result<(), SupervisorError> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_name() == "lock" {
            continue;
        }
        if entry
            .path()
            .extension()
            .is_none_or(|extension| extension != "json")
        {
            return Err(SupervisorError(
                "Unrecognized or incomplete reservation entry".into(),
            ));
        }
        let mut bytes = Vec::new();
        binding::open_record(&entry.path())?
            .take(64 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        let journal = parse(&bytes)?;
        if entry.path().file_stem().and_then(|v| v.to_str()) != Some(&journal.attempt)
            || journal.state != State::Disposed
        {
            return Err(SupervisorError(format!(
                "Unresolved or inconsistent reservation: {}",
                entry.path().display()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn envelope() -> serde_json::Value {
        serde_json::json!({"version":1,"attempt":"one","owner":{"pid":12,"started_seconds":1,"started_microseconds":0},"game":null,"output":std::env::temp_dir().join("retained-one"),"state":"Disposed"})
    }
    #[test]
    fn rejects_unknown_versions_states_and_truncation() {
        let valid = envelope();
        assert!(parse(&serde_json::to_vec(&valid).unwrap()).is_ok());
        for (field, value) in [
            ("version", serde_json::json!(2)),
            ("state", serde_json::json!("Abandoned")),
            ("extra", serde_json::json!(true)),
        ] {
            let mut invalid = valid.clone();
            invalid[field] = value;
            assert!(parse(&serde_json::to_vec(&invalid).unwrap()).is_err());
        }
        let bytes = serde_json::to_vec(&valid).unwrap();
        for length in 0..bytes.len() {
            assert!(parse(&bytes[..length]).is_err());
        }
    }
}
