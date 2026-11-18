use serde::{Deserialize, Serialize};

use crate::Error;
use crate::error::{nonempty, positive};

/// Identity of the geometry on which the mesh and boundary assignments are based.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeometryRevision {
    pub id: String,
    pub revision: String,
}

impl GeometryRevision {
    pub(crate) fn validate(&self, path: &str) -> Result<(), Error> {
        nonempty(&format!("{path}.id"), &self.id)?;
        nonempty(&format!("{path}.revision"), &self.revision)
    }
}

/// An oriented one-dimensional coordinate, embedded in the global Cartesian frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AxisFrame {
    pub origin_m: [f64; 3],
    pub direction: [f64; 3],
}

impl AxisFrame {
    pub(crate) fn validate(&self, path: &str) -> Result<(), Error> {
        if self.origin_m.iter().any(|x| !x.is_finite()) {
            return Err(Error::invalid(path, "frame origin must be finite"));
        }
        let norm = self
            .direction
            .iter()
            .fold(0.0_f64, |norm, x| norm.hypot(*x));
        if !norm.is_finite() || (norm - 1.0).abs() > 1.0e-12 {
            return Err(Error::invalid(
                path,
                "frame direction must be a unit vector",
            ));
        }
        Ok(())
    }

    pub(crate) fn position(&self, distance_m: f64) -> [f64; 3] {
        std::array::from_fn(|i| self.origin_m[i] + distance_m * self.direction[i])
    }
}

/// Contiguous segments between ordered vertices. The two names identify the exterior endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineMesh {
    pub vertices_m: Vec<f64>,
    pub start_region: String,
    pub end_region: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshMetrics {
    pub name: String,
    pub cell_count: usize,
    pub smallest_cell_m: f64,
    pub smallest_cell_index: usize,
    pub largest_cell_m: f64,
}

impl LineMesh {
    pub(crate) fn validate(&self, path: &str, length: f64) -> Result<MeshMetrics, Error> {
        let vertices = &self.vertices_m;
        if vertices.len() < 2 || vertices.first() != Some(&0.0) || vertices.last() != Some(&length)
        {
            return Err(Error::invalid(
                format!("{path}.vertices_m"),
                "requires at least two vertices spanning zero to the geometric extent",
            ));
        }
        nonempty(&format!("{path}.start_region"), &self.start_region)?;
        nonempty(&format!("{path}.end_region"), &self.end_region)?;
        if self.start_region == self.end_region {
            return Err(Error::invalid(path, "endpoint region names must differ"));
        }

        let mut metrics = MeshMetrics {
            name: path.to_owned(),
            cell_count: vertices.len() - 1,
            smallest_cell_m: f64::INFINITY,
            smallest_cell_index: 0,
            largest_cell_m: 0.0,
        };
        for (i, edge) in vertices.windows(2).enumerate() {
            let length = edge[1] - edge[0];
            positive(&format!("{path}.vertices_m[{}]", i + 1), length)?;
            if length < metrics.smallest_cell_m {
                metrics.smallest_cell_m = length;
                metrics.smallest_cell_index = i;
            }
            metrics.largest_cell_m = metrics.largest_cell_m.max(length);
        }
        Ok(metrics)
    }

    pub(crate) fn endpoint(&self, region: &str) -> Option<usize> {
        if region == self.start_region {
            Some(0)
        } else if region == self.end_region {
            Some(self.vertices_m.len() - 1)
        } else {
            None
        }
    }
}
