// Temporary investigation probes; not part of the Native API or the measured baseline.
mod sdk559_profile {
    use std::{
        fs::OpenOptions,
        io::Write,
        time::{Instant, SystemTime, UNIX_EPOCH},
    };
    pub(crate) struct Span {
        label: &'static str,
        started: Instant,
        wall: f64,
    }
    impl Span {
        pub(crate) fn new(label: &'static str) -> Self {
            Self {
                label,
                started: Instant::now(),
                wall: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs_f64(),
            }
        }
    }
    impl Drop for Span {
        fn drop(&mut self) {
            let seconds = self.started.elapsed().as_secs_f64();
            if let Some(directory) = std::env::var_os("NATIVE_PROFILE_DIR") {
                let path = std::path::Path::new(&directory)
                    .join(format!("rust-{}.jsonl", std::process::id()));
                let mut file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .unwrap();
                writeln!(
                    file,
                    "{}",
                    serde_json::json!({"label": self.label, "start": self.wall, "seconds": seconds})
                )
                .unwrap();
            }
        }
    }
}
