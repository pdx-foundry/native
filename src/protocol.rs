use crate::supervisor::SupervisorError;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::io::{Read, Write};

const VERSION: u32 = 4;
const MAX_MESSAGE: usize = 64 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Hello {
    pub version: u32,
    pub build: String,
    pub controller: u32,
    pub authorization: crate::operation::Authorization,
}
impl Hello {
    pub fn current(authorization: crate::operation::Authorization) -> Self {
        Self {
            version: VERSION,
            build: env!("PDX_NATIVE_BUILD").into(),
            controller: std::process::id(),
            authorization,
        }
    }
    pub fn validate(&self) -> Result<(), SupervisorError> {
        if self.version != VERSION
            || self.build != env!("PDX_NATIVE_BUILD")
            || self.controller == std::process::id()
        {
            return Err(SupervisorError(
                "Protocol/build mismatch or supervisor is not a separate process".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum Reply {
    Rejected(String),
    Ready,
    Started {
        attempt: String,
        game: u32,
    },
    Paused {
        readiness: crate::GameReadiness,
        output: std::path::PathBuf,
        registries: std::collections::BTreeMap<String, crate::ArtifactReference>,
    },
    RegistryRead {
        request: u64,
    },
    Finished(Box<crate::operation::AttemptReport>),
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
            build: env!("PDX_NATIVE_BUILD").into(),
            controller: u32::MAX,
            authorization: crate::operation::Authorization::Admitted,
        };
        assert!(valid.validate().is_ok());
        assert!(
            Hello {
                build: "different-linked-build".into(),
                version: VERSION,
                controller: u32::MAX,
                authorization: crate::operation::Authorization::Admitted
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
        write(&mut frame, &serde_json::json!({"version":VERSION,"build":env!("PDX_NATIVE_BUILD"),"controller":1,"bypass":true})).unwrap();
        assert!(read::<Hello>(frame.as_slice()).is_err());
        for length in 0..4 {
            assert!(read::<Hello>(&[0; 4][..length]).is_err());
        }
    }
    #[test]
    fn incompatible_mode_is_rejected_before_owner_setup() {
        let json = serde_json::json!({"version": VERSION, "build": env!("PDX_NATIVE_BUILD"), "controller": u32::MAX, "authorization": "unrecognized"});
        assert!(serde_json::from_value::<Hello>(json).is_err());
    }
}

pub(crate) mod observation;
