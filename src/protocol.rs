//! The private wire between a caller and its supervisor process: the handshake, the replies,
//! and the message framing. `session` holds the request, the controls and the final report;
//! `observation` holds the wire between the supervisor and its debugger worker.
use crate::supervisor::SupervisorError;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::io::{Read, Write};

pub(crate) mod hooks;
pub(crate) mod observation;
pub(crate) mod session;

const VERSION: u32 = 9;
/// A `Paused` reply holds the items of every observed registry, which the worker's stream
/// bounds, and the loaded modifier table, which its file bounds; the other messages are small.
const MAX_MESSAGE: usize = observation::MAX_TRACE + observation::MAX_MODIFIER_TABLE + 64 * 1024;

/// Identity of this build of Native: the package version and the build script's stamp. The
/// caller and the supervisor must link the same build, because they share this private wire and
/// the engine bindings.
pub(crate) const BUILD: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    "+",
    env!("PDX_NATIVE_BUILD_STAMP")
);

/// The caller's first message.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Hello {
    pub version: u32,
    pub build: String,
    /// Process number of the caller. The supervisor must be its direct child.
    pub controller: u32,
}
impl Hello {
    pub fn current() -> Self {
        Self {
            version: VERSION,
            build: BUILD.into(),
            controller: std::process::id(),
        }
    }
    pub fn validate(&self) -> Result<(), SupervisorError> {
        if self.version != VERSION {
            return Err(SupervisorError("Protocol version mismatch".into()));
        }
        if self.build != BUILD {
            return Err(SupervisorError(format!(
                "The supervisor links Native build {BUILD}, and the caller links {}",
                self.build
            )));
        }
        if self.controller == std::process::id() {
            return Err(SupervisorError(
                "The supervisor is not a separate process".into(),
            ));
        }
        Ok(())
    }
}

/// What the supervisor sends to the caller.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum Reply {
    /// The handshake or the request was refused. No game was started.
    Rejected(String),
    /// The handshake succeeded; the supervisor waits for the request.
    Ready,
    /// The game is held at its pause, with what the session established about each registry.
    Paused {
        readiness: crate::GameReadiness,
        fixture: Box<Option<Result<crate::Answer<crate::FixtureObservation>, crate::Error>>>,
        modifiers: Box<
            Option<
                Result<
                    crate::engine::operations::loaded_modifiers::ObservedModifiers,
                    crate::Error,
                >,
            >,
        >,
        registries: std::collections::BTreeMap<
            String,
            crate::engine::operations::registry_items::RegistryItems,
        >,
    },
    /// Acknowledges a registry, fixture or modifier read.
    ObservationRead { request: u64 },
    /// The session is over. Always the last message.
    Finished(Box<session::SessionReport>),
}

pub(crate) fn read<T: DeserializeOwned>(mut input: impl Read) -> Result<T, SupervisorError> {
    let mut length = [0; 4];
    input.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_MESSAGE {
        return Err(SupervisorError("Protocol message exceeds bound".into()));
    }
    let mut bytes = vec![0; length];
    input.read_exact(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub(crate) fn write(
    output: &mut impl Write,
    value: &impl Serialize,
) -> Result<(), SupervisorError> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_MESSAGE {
        return Err(SupervisorError("Protocol message exceeds bound".into()));
    }
    output.write_all(&(bytes.len() as u32).to_be_bytes())?;
    output.write_all(&bytes)?;
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_mismatch_oversize_truncation_and_unknown_fields() {
        let valid = Hello {
            version: VERSION,
            build: BUILD.into(),
            controller: u32::MAX,
        };
        assert!(valid.validate().is_ok());
        assert!(
            Hello {
                build: "different-linked-build".into(),
                version: VERSION,
                controller: u32::MAX,
            }
            .validate()
            .is_err()
        );
        assert!(
            Hello {
                version: VERSION + 1,
                ..valid
            }
            .validate()
            .is_err()
        );
        assert!(read::<Hello>((MAX_MESSAGE as u32 + 1).to_be_bytes().as_slice()).is_err());
        let mut frame = Vec::new();
        write(
            &mut frame,
            &serde_json::json!({"version":VERSION,"build":BUILD,"controller":1,"bypass":true}),
        )
        .unwrap();
        assert!(read::<Hello>(frame.as_slice()).is_err());
        for length in 0..4 {
            assert!(read::<Hello>(&[0; 4][..length]).is_err());
        }
    }
}
