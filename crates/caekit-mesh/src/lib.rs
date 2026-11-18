//! Mesh geometry, region assignments, and mesh quality checks.

mod line;
pub mod tetra;

pub use line::{AxisFrame, GeometryRevision, LineMesh, MeshMetrics, MeshQuality};
pub use tetra::{FaceRegion, TetraMesh};
