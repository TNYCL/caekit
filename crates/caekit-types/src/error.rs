//! Error type shared by model reading, validation and solving, with scalar validation helpers.

use std::fmt;

/// A syntactically invalid or unsupported model, or an invalid simulation result.
#[derive(Debug)]
pub enum Error {
    /// `serde_json` rejected the document: malformed JSON, or JSON that does not match the model
    /// schema (unknown or missing key, wrong value type).
    ///
    /// Any `serde_json::Error` converts into this variant through `From`, and it is the only
    /// variant that reports a `source`.
    Json(serde_json::Error),
    /// The document's `schema_version`, carried here, is not one this reader supports; the only
    /// supported version is 1.
    UnsupportedModelVersion(u32),
    /// A model input violates a validation rule.
    ///
    /// Solvers also return it for a computed value that fails a physical check, such as a solved
    /// temperature at or below absolute zero. Displayed as `path: message`.
    InvalidModel {
        /// Location of the offending value: normally its dotted JSON path with array indices, such
        /// as `physics.geometry.length_m`; for a computed value, a description of that value.
        path: String,
        /// The violated requirement, for example `must be positive and finite`.
        message: String,
    },
    /// A computed value, balance sum or reported quantity is NaN or infinite, so the calculation
    /// left the finite binary64 range.
    NonFiniteResult {
        /// Name of the value that is not finite, for example a balance name or an output.
        quantity: String,
    },
    /// The size of a required buffer overflowed `usize`, or its memory could not be reserved.
    InsufficientMemory {
        /// Description of the buffer that could not be sized or allocated.
        resource: String,
    },
}

impl Error {
    /// Builds an [`Error::InvalidModel`] from the location of a value and the requirement it
    /// violates.
    pub fn invalid(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self::InvalidModel {
            path: path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(f, "invalid model JSON: {error}"),
            Self::UnsupportedModelVersion(version) => {
                write!(
                    f,
                    "unsupported model schema version {version}; supported version is 1"
                )
            }
            Self::InvalidModel { path, message } => write!(f, "{path}: {message}"),
            Self::InsufficientMemory { resource } => {
                write!(f, "insufficient memory for {resource}")
            }
            Self::NonFiniteResult { quantity } => {
                write!(
                    f,
                    "{quantity}: calculation exceeded the finite binary64 range"
                )
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

/// Checks that `value` is neither NaN nor infinite.
///
/// Returns [`Error::InvalidModel`] at `path` with the message `must be finite` otherwise.
pub fn finite(path: &str, value: f64) -> Result<(), Error> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(Error::invalid(path, "must be finite"))
    }
}

/// Checks that `value` is finite and strictly greater than zero.
///
/// Returns [`Error::InvalidModel`] at `path` with the message `must be positive and finite`
/// otherwise; zero, negative values, NaN and infinities are all rejected.
pub fn positive(path: &str, value: f64) -> Result<(), Error> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(Error::invalid(path, "must be positive and finite"))
    }
}

/// Checks that `value` contains at least one non-whitespace character.
///
/// Returns [`Error::InvalidModel`] at `path` with the message `must be nonempty` otherwise.
pub fn nonempty(path: &str, value: &str) -> Result<(), Error> {
    if value.trim().is_empty() {
        Err(Error::invalid(path, "must be nonempty"))
    } else {
        Ok(())
    }
}
