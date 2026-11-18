//! Structural mechanics models and solvers.

pub mod axial_bar;
pub mod elastic_solid;

// A constant displacement in each Cartesian direction has zero strain.
// Remove it before elimination so it cannot set the residual scale.
fn relative_constraints<const D: usize>(
    prescribed: &[Option<f64>],
) -> ([f64; D], Vec<Option<f64>>) {
    let translation = std::array::from_fn(|axis| {
        prescribed
            .iter()
            .skip(axis)
            .step_by(D)
            .flatten()
            .next()
            .copied()
            .unwrap_or(0.0)
    });
    let relative = prescribed
        .iter()
        .enumerate()
        .map(|(i, value)| value.map(|u| u - translation[i % D]))
        .collect();
    (translation, relative)
}
