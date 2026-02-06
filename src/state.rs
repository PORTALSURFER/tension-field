//! Versioned plugin state serialization for Tension Field.

use std::io::{Read, Write};

use crate::params::{STATE_VALUE_COUNT, default_state_values};

/// Four-byte magic marker for Tension Field state payloads (`TFST`).
pub(crate) const STATE_MAGIC: u32 = u32::from_le_bytes(*b"TFST");
/// Current state payload version.
pub(crate) const STATE_VERSION: u32 = 3;
/// Number of persisted meter values.
pub(crate) const METER_COUNT: usize = 9;

/// Complete serialized snapshot for CLAP state save/load.
#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) struct PluginStateSnapshot {
    /// Ordered parameter values in `PARAM_DEFS` order.
    pub(crate) param_values: [f32; STATE_VALUE_COUNT],
    /// UI meter values used to restore visual continuity.
    pub(crate) meter_values: [f32; METER_COUNT],
}

/// Decode failures for Tension Field plugin state.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum StateDecodeError {
    /// Stream I/O failed.
    Io,
    /// Payload header does not match expected format.
    InvalidPayload,
    /// Payload version is not supported by this build.
    UnsupportedVersion,
    /// Payload contains invalid floating-point data.
    NonFiniteValue,
}

impl StateDecodeError {
    /// Return a static plugin-facing error message.
    pub(crate) fn as_message(self) -> &'static str {
        match self {
            Self::Io => "Could not read Tension Field plugin state",
            Self::InvalidPayload => "Invalid Tension Field state payload",
            Self::UnsupportedVersion => "Unsupported Tension Field state version",
            Self::NonFiniteValue => "Invalid Tension Field state payload",
        }
    }
}

impl From<std::io::Error> for StateDecodeError {
    fn from(_value: std::io::Error) -> Self {
        Self::Io
    }
}

/// Write a full plugin snapshot to a CLAP-compatible stream.
pub(crate) fn write_snapshot<W: Write>(
    writer: &mut W,
    snapshot: &PluginStateSnapshot,
) -> std::io::Result<()> {
    writer.write_all(&STATE_MAGIC.to_le_bytes())?;
    writer.write_all(&STATE_VERSION.to_le_bytes())?;
    writer.write_all(&(STATE_VALUE_COUNT as u32).to_le_bytes())?;
    writer.write_all(&(METER_COUNT as u32).to_le_bytes())?;

    for value in snapshot.param_values {
        writer.write_all(&value.to_le_bytes())?;
    }
    for value in snapshot.meter_values {
        writer.write_all(&value.to_le_bytes())?;
    }
    Ok(())
}

/// Read a full plugin snapshot from a CLAP-compatible stream.
pub(crate) fn read_snapshot<R: Read>(
    reader: &mut R,
) -> Result<PluginStateSnapshot, StateDecodeError> {
    let magic = read_u32(reader)?;
    let version = read_u32(reader)?;
    let param_count = read_u32(reader)?;
    let meter_count = read_u32(reader)?;

    if magic != STATE_MAGIC {
        return Err(StateDecodeError::InvalidPayload);
    }
    if meter_count != METER_COUNT as u32 {
        return Err(StateDecodeError::InvalidPayload);
    }

    let mut param_values = default_state_values();
    match version {
        STATE_VERSION => {
            if param_count != STATE_VALUE_COUNT as u32 {
                return Err(StateDecodeError::InvalidPayload);
            }
            for value in &mut param_values {
                *value = read_f32(reader)?;
                if !value.is_finite() {
                    return Err(StateDecodeError::NonFiniteValue);
                }
            }
        }
        2 => {
            if param_count > STATE_VALUE_COUNT as u32 {
                return Err(StateDecodeError::InvalidPayload);
            }
            for value in param_values.iter_mut().take(param_count as usize) {
                *value = read_f32(reader)?;
                if !value.is_finite() {
                    return Err(StateDecodeError::NonFiniteValue);
                }
            }
        }
        _ => {
            return Err(StateDecodeError::UnsupportedVersion);
        }
    }

    let mut meter_values = [0.0; METER_COUNT];
    for value in &mut meter_values {
        *value = read_f32(reader)?;
        if !value.is_finite() {
            return Err(StateDecodeError::NonFiniteValue);
        }
    }

    Ok(PluginStateSnapshot {
        param_values,
        meter_values,
    })
}

fn read_u32<R: Read>(reader: &mut R) -> Result<u32, StateDecodeError> {
    let mut bytes = [0u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_f32<R: Read>(reader: &mut R) -> Result<f32, StateDecodeError> {
    let mut bytes = [0u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(f32::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::{
        METER_COUNT, PluginStateSnapshot, STATE_MAGIC, STATE_VALUE_COUNT, STATE_VERSION,
        StateDecodeError, read_snapshot, write_snapshot,
    };

    #[test]
    fn roundtrip_preserves_snapshot() {
        let mut params = [0.0; STATE_VALUE_COUNT];
        let mut meters = [0.0; METER_COUNT];
        for (index, value) in params.iter_mut().enumerate() {
            *value = index as f32 * 0.125;
        }
        for (index, value) in meters.iter_mut().enumerate() {
            *value = index as f32 * 0.05;
        }

        let expected = PluginStateSnapshot {
            param_values: params,
            meter_values: meters,
        };

        let mut data = Vec::new();
        write_snapshot(&mut data, &expected).expect("state should serialize");

        let mut cursor = data.as_slice();
        let actual = read_snapshot(&mut cursor).expect("state should deserialize");

        assert_eq!(actual, expected);
    }

    #[test]
    fn invalid_magic_is_rejected() {
        let mut data = Vec::new();
        data.extend_from_slice(&0xDEADBEEFu32.to_le_bytes());
        data.extend_from_slice(&STATE_VERSION.to_le_bytes());
        data.extend_from_slice(&(STATE_VALUE_COUNT as u32).to_le_bytes());
        data.extend_from_slice(&(METER_COUNT as u32).to_le_bytes());
        data.resize(data.len() + (STATE_VALUE_COUNT + METER_COUNT) * 4, 0);

        let mut cursor = data.as_slice();
        let error = read_snapshot(&mut cursor).expect_err("invalid magic must fail");
        assert_eq!(error, StateDecodeError::InvalidPayload);
    }

    #[test]
    fn invalid_version_is_rejected() {
        let mut data = Vec::new();
        data.extend_from_slice(&STATE_MAGIC.to_le_bytes());
        data.extend_from_slice(&99u32.to_le_bytes());
        data.extend_from_slice(&(STATE_VALUE_COUNT as u32).to_le_bytes());
        data.extend_from_slice(&(METER_COUNT as u32).to_le_bytes());
        data.resize(data.len() + (STATE_VALUE_COUNT + METER_COUNT) * 4, 0);

        let mut cursor = data.as_slice();
        let error = read_snapshot(&mut cursor).expect_err("invalid version must fail");
        assert_eq!(error, StateDecodeError::UnsupportedVersion);
    }

    #[test]
    fn v2_snapshot_migrates_missing_param_values() {
        let legacy_param_count = STATE_VALUE_COUNT as u32 - 3;
        let mut data = Vec::new();
        data.extend_from_slice(&STATE_MAGIC.to_le_bytes());
        data.extend_from_slice(&2u32.to_le_bytes());
        data.extend_from_slice(&legacy_param_count.to_le_bytes());
        data.extend_from_slice(&(METER_COUNT as u32).to_le_bytes());

        for index in 0..legacy_param_count {
            data.extend_from_slice(&((index as f32) * 0.01).to_le_bytes());
        }
        for _ in 0..METER_COUNT {
            data.extend_from_slice(&0.0f32.to_le_bytes());
        }

        let mut cursor = data.as_slice();
        let snapshot = read_snapshot(&mut cursor).expect("v2 state should migrate");

        assert!((snapshot.param_values[0] - 0.0).abs() < 1.0e-6);
        assert!((snapshot.param_values[legacy_param_count as usize - 1] - 0.47).abs() < 1.0e-6);
        assert!(
            snapshot.param_values[(legacy_param_count as usize)..]
                .iter()
                .all(|value| value.is_finite())
        );
    }
}
