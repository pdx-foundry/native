//! The supervisor's record of what it did to the game and the worker.
//!
//! The events stay in memory for the reducers. Each event is also appended to
//! `owner-events.jsonl` in the work directory, for inspection when a session fails.
use crate::{engine::operations::event_stream::OwnerEvent, supervisor::SupervisorError};
use std::{fs::OpenOptions, io::Write, path::PathBuf};

pub(super) struct OwnerEvents {
    file: PathBuf,
    events: Vec<OwnerEvent>,
}

impl OwnerEvents {
    pub(super) fn new(work_directory: &std::path::Path) -> Self {
        Self {
            file: work_directory.join("owner-events.jsonl"),
            events: Vec::new(),
        }
    }

    pub(super) fn record(&mut self, event: OwnerEvent) -> Result<(), SupervisorError> {
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.file)?;
        serde_json::to_writer(&mut file, &event)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        self.events.push(event);
        Ok(())
    }

    pub(super) fn all(&self) -> &[OwnerEvent] {
        &self.events
    }
}
