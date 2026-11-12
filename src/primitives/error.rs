//! Shared error type.

use core::fmt;
use std::path::{Path, PathBuf};

/// Errors produced by superb.
///
/// Each variant is something an application can act on: a missing device
/// is not a missing file, and a file error carries its path. The variants
/// that own heap data (a path, a name, a reason) are only ever constructed
/// off the audio thread; everything the audio path returns is `Copy`-sized.
///
/// ```
/// use superb::primitives::SuperbError;
///
/// let mismatch = SuperbError::ChannelMismatch {
///     expected: 2,
///     actual: 1,
/// };
/// assert_eq!(mismatch.to_string(), "channel mismatch: expected 2, got 1");
/// let missing = SuperbError::NotFound {
///     kind: "node",
///     id: 7,
/// };
/// assert_eq!(missing.to_string(), "node with id 7 not found");
/// let message: SuperbError = "device vanished".into();
/// assert_eq!(message.to_string(), "device vanished");
///
/// // A file error keeps what was being done, where, and why.
/// let gone = std::io::Error::from(std::io::ErrorKind::NotFound);
/// let error = SuperbError::io("read", "/music/vox.wav", &gone);
/// match &error {
///     SuperbError::Io { path, kind, .. } => {
///         assert_eq!(path.as_deref(), Some(std::path::Path::new("/music/vox.wav")));
///         assert_eq!(*kind, std::io::ErrorKind::NotFound);
///     }
///     _ => unreachable!(),
/// }
/// assert!(error.to_string().starts_with("cannot read /music/vox.wav: "));
/// let busy = SuperbError::InvalidState {
///     why: "a recording is already running",
/// };
/// assert_eq!(busy.to_string(), "a recording is already running");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SuperbError {
    /// A numeric argument was outside its valid range.
    InvalidArgument {
        /// Name of the offending parameter.
        what: &'static str,
    },
    /// A fixed-capacity container is full (audio-thread safe containers never grow).
    CapacityExceeded {
        /// Name of the container.
        what: &'static str,
        /// Capacity of the container.
        capacity: usize,
    },
    /// Two buffers or nodes have incompatible channel counts.
    ChannelMismatch {
        /// Expected channel count.
        expected: usize,
        /// Channel count actually provided.
        actual: usize,
    },
    /// Two buffers have incompatible frame counts.
    FrameMismatch {
        /// Expected frame count.
        expected: usize,
        /// Frame count actually provided.
        actual: usize,
    },
    /// An ID did not refer to a live object.
    NotFound {
        /// Kind of object (e.g. `"node"`).
        kind: &'static str,
        /// The raw ID value.
        id: u64,
    },
    /// A requested connection would create a cycle in a graph.
    Cycle,
    /// An operation was attempted before the object was prepared.
    NotPrepared,
    /// A file or folder could not be read, written, created or removed.
    Io {
        /// What was being done: `"read"`, `"write"`, `"create"`, `"open"`.
        action: &'static str,
        /// The file or folder, when the operation had one (a stream has none).
        path: Option<PathBuf>,
        /// The operating system's kind of failure.
        kind: std::io::ErrorKind,
        /// The operating system's message.
        reason: String,
    },
    /// Data was not in the format expected: not a WAV file, a session file
    /// from a newer version, JSON that does not parse, an unknown codec.
    Format {
        /// The file, when it is known.
        path: Option<PathBuf>,
        /// What was wrong with it.
        reason: String,
    },
    /// No audio device of that name, or no default device.
    DeviceNotFound {
        /// The name asked for, or `"default output"` / `"default input"`.
        name: String,
    },
    /// The audio or MIDI backend refused: it could not list devices, open
    /// or start a stream, or the device does not support the configuration.
    Device(String),
    /// A clip names audio that is not in memory.
    MissingAudio {
        /// The key the clip refers to.
        key: PathBuf,
    },
    /// The call is not possible as things stand: a recording is already
    /// running, no track is armed, a device has the audio thread.
    InvalidState {
        /// What is in the way, as a sentence.
        why: &'static str,
    },
    /// The crate was built without the feature this needs, or the thing is
    /// not implemented.
    Unsupported(String),
    /// Anything else, with a human readable message: a thread that
    /// panicked, a plugin's own error.
    Message(String),
}

impl fmt::Display for SuperbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SuperbError::InvalidArgument { what } => write!(f, "invalid argument: {what}"),
            SuperbError::CapacityExceeded { what, capacity } => {
                write!(f, "capacity of {what} exceeded (capacity = {capacity})")
            }
            SuperbError::ChannelMismatch { expected, actual } => {
                write!(f, "channel mismatch: expected {expected}, got {actual}")
            }
            SuperbError::FrameMismatch { expected, actual } => {
                write!(f, "frame mismatch: expected {expected}, got {actual}")
            }
            SuperbError::NotFound { kind, id } => write!(f, "{kind} with id {id} not found"),
            SuperbError::Cycle => write!(f, "operation would create a cycle"),
            SuperbError::NotPrepared => write!(f, "object has not been prepared"),
            SuperbError::Io {
                action,
                path: Some(path),
                reason,
                ..
            } => write!(f, "cannot {action} {}: {reason}", path.display()),
            SuperbError::Io {
                action,
                path: None,
                reason,
                ..
            } => write!(f, "cannot {action}: {reason}"),
            SuperbError::Format {
                path: Some(path),
                reason,
            } => write!(f, "{}: {reason}", path.display()),
            SuperbError::Format { path: None, reason } => f.write_str(reason),
            SuperbError::DeviceNotFound { name } => write!(f, "no audio device: {name}"),
            SuperbError::MissingAudio { key } => {
                write!(f, "no audio named {:?} is in memory", key.display())
            }
            SuperbError::InvalidState { why } => f.write_str(why),
            SuperbError::Device(msg)
            | SuperbError::Unsupported(msg)
            | SuperbError::Message(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for SuperbError {}

impl SuperbError {
    /// A file error: what was being done (`"read"`, `"write"`, `"create"`),
    /// to which path, and the operating system's answer.
    pub fn io(action: &'static str, path: impl AsRef<Path>, error: &std::io::Error) -> Self {
        SuperbError::Io {
            action,
            path: Some(path.as_ref().to_path_buf()),
            kind: error.kind(),
            reason: error.to_string(),
        }
    }

    /// Data that was not in the format expected, in `path` when it came
    /// from a file.
    pub fn format(path: Option<&Path>, reason: impl Into<String>) -> Self {
        SuperbError::Format {
            path: path.map(Path::to_path_buf),
            reason: reason.into(),
        }
    }
}

impl From<String> for SuperbError {
    fn from(value: String) -> Self {
        SuperbError::Message(value)
    }
}

impl From<&str> for SuperbError {
    fn from(value: &str) -> Self {
        SuperbError::Message(value.to_owned())
    }
}

/// Convenience alias used across the SDK.
pub type Result<T> = core::result::Result<T, SuperbError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_human_readable() {
        let e = SuperbError::ChannelMismatch {
            expected: 2,
            actual: 1,
        };
        assert_eq!(e.to_string(), "channel mismatch: expected 2, got 1");
        let e = SuperbError::NotFound {
            kind: "node",
            id: 7,
        };
        assert_eq!(e.to_string(), "node with id 7 not found");
        let e: SuperbError = "device vanished".into();
        assert_eq!(e.to_string(), "device vanished");
    }

    #[test]
    fn implements_std_error() {
        fn takes_error<E: std::error::Error>(_: E) {}
        takes_error(SuperbError::Cycle);
    }
}
