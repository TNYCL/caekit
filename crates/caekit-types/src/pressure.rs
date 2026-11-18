use serde::{Deserialize, Serialize};

use crate::{Error, error::positive};

/// Absolute pressure or a gauge reading with an explicitly specified absolute reference.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "reference", rename_all = "snake_case", deny_unknown_fields)]
pub enum Pressure {
    Absolute {
        value_pa: f64,
    },
    Gauge {
        value_pa: f64,
        reference_absolute_pa: f64,
    },
}

impl Pressure {
    fn parts(self) -> (f64, f64) {
        match self {
            Self::Absolute { value_pa } => (0.0, value_pa),
            Self::Gauge {
                value_pa,
                reference_absolute_pa,
            } => (reference_absolute_pa, value_pa),
        }
    }

    pub fn absolute_pa(self) -> f64 {
        let (reference, value) = self.parts();
        reference + value
    }

    pub fn difference_pa(self, other: Self) -> f64 {
        let (reference, value) = self.parts();
        let (other_reference, other_value) = other.parts();
        if reference == other_reference {
            value - other_value
        } else {
            self.absolute_pa() - other.absolute_pa()
        }
    }

    pub fn validate(self, path: &str) -> Result<(), Error> {
        if let Self::Gauge {
            reference_absolute_pa,
            ..
        } = self
        {
            positive(
                &format!("{path}.reference_absolute_pa"),
                reference_absolute_pa,
            )?;
        }
        positive(&format!("{path}.absolute_pressure_pa"), self.absolute_pa())
    }
}
