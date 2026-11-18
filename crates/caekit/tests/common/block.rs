use std::collections::BTreeSet;

use caekit::{FaceRegion, TetraMesh};

pub fn structured_block(dimensions: [f64; 3], divisions: usize) -> TetraMesh {
    let side = divisions + 1;
    let index = |x, y, z| (x * side + y) * side + z;
    let mut vertices_m = Vec::new();
    let mut cells = Vec::new();
    for x in 0..side {
        for y in 0..side {
            for z in 0..side {
                vertices_m.push(std::array::from_fn(|axis| {
                    dimensions[axis] * ([x, y, z][axis] as f64 / divisions as f64)
                }));
                if x == divisions || y == divisions || z == divisions {
                    continue;
                }
                let corners =
                    std::array::from_fn::<_, 8, _>(|i| index(x + i / 4, y + i / 2 % 2, z + i % 2));
                for tet in [
                    [0, 4, 6, 7],
                    [0, 5, 4, 7],
                    [0, 6, 2, 7],
                    [0, 2, 3, 7],
                    [0, 1, 5, 7],
                    [0, 3, 1, 7],
                ] {
                    cells.push(tet.map(|i| corners[i]));
                }
            }
        }
    }

    let mut exterior = BTreeSet::new();
    for &[a, b, c, d] in &cells {
        for mut face in [[a, b, c], [a, b, d], [a, c, d], [b, c, d]] {
            face.sort_unstable();
            if !exterior.insert(face) {
                exterior.remove(&face);
            }
        }
    }
    let boundary_regions = ["x0", "x1", "y0", "y1", "z0", "z1"]
        .into_iter()
        .enumerate()
        .map(|(i, name)| {
            let axis = i / 2;
            let coordinate = if i % 2 == 0 { 0.0 } else { dimensions[axis] };
            FaceRegion {
                name: name.to_owned(),
                faces: exterior
                    .iter()
                    .copied()
                    .filter(|face| {
                        face.iter()
                            .all(|&node| vertices_m[node][axis] == coordinate)
                    })
                    .collect(),
            }
        })
        .collect();
    TetraMesh {
        vertices_m,
        cells,
        boundary_regions,
    }
}

/// Four equal-weight interior samples, exact for degree-two tetrahedral integrals.
pub const TETRA_SAMPLES: [[f64; 4]; 4] = [[A, B, B, B], [B, A, B, B], [B, B, A, B], [B, B, B, A]];
const A: f64 = 0.585_410_196_624_968_5;
const B: f64 = 0.138_196_601_125_010_5;

pub fn interpolate(values: [f64; 4], weights: [f64; 4]) -> f64 {
    values.into_iter().zip(weights).map(|(v, w)| v * w).sum()
}
