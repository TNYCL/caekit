use std::fmt;

/// A syntactically invalid or unsupported model, or an invalid simulation result.
#[derive(Debug)]
pub enum Error {
    Json(serde_json::Error),
    UnsupportedModelVersion(u32),
    InvalidModel { path: String, message: String },
    NonFiniteResult { quantity: String },
    InsufficientMemory { resource: String },
}

impl Error {
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

pub fn finite(path: &str, value: f64) -> Result<(), Error> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(Error::invalid(path, "must be finite"))
    }
}

pub fn positive(path: &str, value: f64) -> Result<(), Error> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(Error::invalid(path, "must be positive and finite"))
    }
}

pub fn nonempty(path: &str, value: &str) -> Result<(), Error> {
    if value.trim().is_empty() {
        Err(Error::invalid(path, "must be nonempty"))
    } else {
        Ok(())
    }
}
