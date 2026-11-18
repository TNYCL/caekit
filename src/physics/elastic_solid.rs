//! Linear three-dimensional elasticity on positively oriented tetrahedra.

use serde::{Deserialize, Serialize};

use crate::tetra::{Tet, TetraTopology, cross, dot, subtract};
use crate::{Error, GeometryRevision, MaterialSource, MeshMetrics, TetraMesh};
use caekit_numerics::{ConstrainedSystem, Convergence, LinearSettings, StopReason, norm};
use caekit_types::error::{finite, nonempty, positive};
use caekit_types::{Balance, Output, Quantity, SampleLocation, Solution};

use super::laminar_channel::Pressure;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElasticSolid {
    pub geometry: GeometryRevision,
    pub mesh: TetraMesh,
    pub material: IsotropicElasticMaterial,
    pub boundary_conditions: Vec<SolidBoundary>,
    #[serde(default)]
    pub body_force_n_m3: [f64; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IsotropicElasticMaterial {
    pub id: String,
    pub young_modulus_pa: f64,
    pub poisson_ratio: f64,
    pub source: MaterialSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SolidBoundary {
    Displacement {
        region: String,
        /// A null component remains free.
        components_m: [Option<f64>; 3],
    },
    Traction {
        region: String,
        components_pa: [f64; 3],
    },
    Pressure {
        region: String,
        applied: Pressure,
        ambient: Pressure,
    },
}

impl SolidBoundary {
    fn region(&self) -> &str {
        match self {
            Self::Displacement { region, .. }
            | Self::Traction { region, .. }
            | Self::Pressure { region, .. } => region,
        }
    }
}

#[derive(Clone, Copy)]
struct Lame {
    lambda: f64,
    mu: f64,
}

impl Lame {
    fn from_material(material: &IsotropicElasticMaterial) -> Result<Self, Error> {
        let nu = material.poisson_ratio;
        let modulus = material.young_modulus_pa;
        let mu = (0.5 * modulus) / (1.0 + nu);
        let lambda = 2.0 * (mu * nu) / (1.0 - 2.0 * nu);
        positive("elastic shear modulus", mu)?;
        finite("first Lame parameter", lambda)?;
        Ok(Self { lambda, mu })
    }

    fn stiffness(self, grad_a: [f64; 3], grad_b: [f64; 3], i: usize, j: usize) -> f64 {
        let shear = self.mu * grad_a[j] * grad_b[i];
        let dilatation = self.lambda * grad_a[i] * grad_b[j];
        let diagonal = if i == j {
            self.mu * dot(grad_a, grad_b)
        } else {
            0.0
        };
        shear + dilatation + diagonal
    }

    fn stress(self, strain: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
        let trace: f64 = (0..3).map(|i| strain[i][i]).sum();
        std::array::from_fn(|i| {
            std::array::from_fn(|j| {
                let diagonal = if i == j { self.lambda * trace } else { 0.0 };
                2.0 * (self.mu * strain[i][j]) + diagonal
            })
        })
    }
}

impl ElasticSolid {
    pub(crate) const OUTPUTS: &[Output] = &[
        Output::DisplacementVector,
        Output::CauchyStress,
        Output::VonMisesStress,
        Output::NodalReaction,
        Output::ElasticEnergy,
    ];
    const METHOD: &str = "static_small_strain_isotropic_p1_tetrahedral_fem";

    pub(crate) fn validate(&self) -> Result<MeshMetrics, Error> {
        let topology = self.mesh.check("physics.mesh")?;
        self.geometry.validate("physics.geometry")?;
        nonempty("physics.material.id", &self.material.id)?;
        positive(
            "physics.material.young_modulus_pa",
            self.material.young_modulus_pa,
        )?;
        if !(-1.0 < self.material.poisson_ratio && self.material.poisson_ratio < 0.5) {
            return Err(Error::invalid(
                "physics.material.poisson_ratio",
                "must be strictly between -1 and 0.5",
            ));
        }
        Lame::from_material(&self.material)?;
        self.material.source.validate("physics.material.source")?;
        for (i, value) in self.body_force_n_m3.iter().enumerate() {
            finite(&format!("physics.body_force_n_m3[{i}]"), *value)?;
        }
        let values = self.prescribed(&topology)?;
        if !supports_all_rigid_modes(&self.mesh.vertices_m, &values) {
            return Err(Error::invalid(
                "physics.boundary_conditions",
                "prescribed displacement components must restrain all six rigid-body modes",
            ));
        }
        Ok(topology.metrics)
    }

    fn prescribed(&self, topology: &TetraTopology<'_>) -> Result<Vec<Option<f64>>, Error> {
        let dofs =
            self.mesh
                .vertices_m
                .len()
                .checked_mul(3)
                .ok_or_else(|| Error::InsufficientMemory {
                    resource: "solid degrees of freedom".to_owned(),
                })?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(dofs)
            .map_err(|_| Error::InsufficientMemory {
                resource: "solid degrees of freedom".to_owned(),
            })?;
        values.resize(dofs, None);

        for (i, boundary) in self.boundary_conditions.iter().enumerate() {
            let path = format!("physics.boundary_conditions[{i}]");
            let region = boundary.region();
            let Some(faces) = topology.boundaries.get(region) else {
                return Err(Error::invalid(
                    path,
                    format!("unknown exterior region {region:?}"),
                ));
            };
            match boundary {
                SolidBoundary::Displacement { components_m, .. } => {
                    if !components_m.iter().any(Option::is_some) {
                        return Err(Error::invalid(
                            path,
                            "prescribe at least one displacement component on the region",
                        ));
                    }
                    for (axis, value) in components_m.iter().enumerate() {
                        let Some(value) = value else { continue };
                        finite(&format!("{path}.components_m[{axis}]"), *value)?;
                        for node in faces.iter().flat_map(|face| face.nodes) {
                            let existing = &mut values[3 * node + axis];
                            if existing.is_some_and(|other| other != *value) {
                                return Err(Error::invalid(
                                    path,
                                    format!(
                                        "conflicting displacement at vertex {node}, component {axis}"
                                    ),
                                ));
                            }
                            *existing = Some(*value);
                        }
                    }
                }
                SolidBoundary::Traction { components_pa, .. } => {
                    for (axis, value) in components_pa.iter().enumerate() {
                        finite(&format!("{path}.components_pa[{axis}]"), *value)?;
                    }
                }
                SolidBoundary::Pressure {
                    applied, ambient, ..
                } => {
                    applied.validate(&format!("{path}.applied"))?;
                    ambient.validate(&format!("{path}.ambient"))?;
                    finite(
                        &format!("{path}.pressure_difference"),
                        applied.difference_pa(*ambient),
                    )?;
                }
            }
        }
        Ok(values)
    }

    pub(crate) fn solve(
        &self,
        settings: LinearSettings,
        outputs: &[Output],
    ) -> Result<Solution<Convergence>, Error> {
        let topology = self.mesh.check("physics.mesh")?;
        let prescribed = self.prescribed(&topology)?;
        let lame = Lame::from_material(&self.material)?;
        let vertices = &self.mesh.vertices_m;
        let loads = self.assemble_forces(&topology);
        let mut system = ConstrainedSystem::new(prescribed.clone(), &loads);
        for (tet, nodes) in topology.elements.iter().zip(&self.mesh.cells) {
            for (a, node_a) in nodes.iter().enumerate() {
                for (b, node_b) in nodes.iter().enumerate() {
                    for i in 0..3 {
                        for j in 0..3 {
                            let stiffness = tet.volume
                                * lame.stiffness(tet.gradients[a], tet.gradients[b], i, j);
                            system.add(3 * node_a + i, 3 * node_b + j, stiffness);
                        }
                    }
                }
            }
        }

        let (displacement, convergence) = system.solve(settings, "solid_elastic_equilibrium", "N");
        let stop_reason = convergence.stop_reason;
        let mut result = Solution::new(Self::METHOD, vec![convergence]);
        if stop_reason != StopReason::Converged {
            return Ok(result);
        }

        let u: Vec<[f64; 3]> = displacement
            .chunks_exact(3)
            .map(|values| [values[0], values[1], values[2]])
            .collect();
        let mut internal = vec![[0.0; 3]; vertices.len()];
        let mut stresses = Vec::new();
        let mut von_mises = Vec::new();
        let want_stress = outputs.contains(&Output::CauchyStress);
        let want_von_mises = outputs.contains(&Output::VonMisesStress);
        let mut energy = 0.0;
        for (tet, &nodes) in topology.elements.iter().zip(&self.mesh.cells) {
            let strain = strain(tet, nodes.map(|i| u[i]));
            let stress = lame.stress(strain);
            for (a, node) in nodes.into_iter().enumerate() {
                for (i, row) in stress.iter().enumerate() {
                    internal[node][i] += tet.volume * dot(*row, tet.gradients[a]);
                }
            }
            energy += 0.5
                * tet.volume
                * stress
                    .iter()
                    .zip(strain)
                    .map(|(stress, strain)| dot(*stress, strain))
                    .sum::<f64>();
            if want_stress {
                stresses.push(stress);
            }
            if want_von_mises {
                von_mises.push(von_mises_stress(stress));
            }
        }
        let reactions: Vec<[f64; 3]> = internal
            .iter()
            .enumerate()
            .map(|(node, force)| {
                std::array::from_fn(|i| {
                    let dof = 3 * node + i;
                    if prescribed[dof].is_some() {
                        force[i] - loads[dof]
                    } else {
                        0.0
                    }
                })
            })
            .collect();
        result.balances = mechanical_balances(vertices, &loads, &reactions)?;
        result
            .quantities
            .insert(Output::ElasticEnergy, Quantity::scalar("J", energy));
        if outputs.contains(&Output::DisplacementVector) {
            result.quantities.insert(
                Output::DisplacementVector,
                Quantity::VectorField {
                    unit: "m".to_owned(),
                    location: SampleLocation::Node,
                    positions_m: vertices.clone(),
                    values: u,
                },
            );
        }
        if outputs.contains(&Output::NodalReaction) {
            result.quantities.insert(
                Output::NodalReaction,
                Quantity::VectorField {
                    unit: "N".to_owned(),
                    location: SampleLocation::Node,
                    positions_m: vertices.clone(),
                    values: reactions,
                },
            );
        }
        if want_stress {
            result.quantities.insert(
                Output::CauchyStress,
                Quantity::TensorField {
                    unit: "Pa".to_owned(),
                    location: SampleLocation::Cell,
                    positions_m: topology.elements.iter().map(|tet| tet.centroid).collect(),
                    values: stresses,
                },
            );
        }
        if want_von_mises {
            result.quantities.insert(
                Output::VonMisesStress,
                Quantity::field(
                    "Pa",
                    SampleLocation::Cell,
                    topology.elements.iter().map(|tet| tet.centroid).collect(),
                    von_mises,
                ),
            );
        }
        Ok(result)
    }

    fn assemble_forces(&self, topology: &TetraTopology<'_>) -> Vec<f64> {
        let mut loads = vec![0.0; 3 * self.mesh.vertices_m.len()];
        for (tet, nodes) in topology.elements.iter().zip(&self.mesh.cells) {
            for node in nodes {
                for (i, force) in self.body_force_n_m3.iter().enumerate() {
                    loads[3 * node + i] += (0.25 * tet.volume) * force;
                }
            }
        }
        for condition in &self.boundary_conditions {
            if let SolidBoundary::Displacement { .. } = condition {
                continue;
            }
            for face in &topology.boundaries[condition.region()] {
                let traction = match condition {
                    SolidBoundary::Traction { components_pa, .. } => *components_pa,
                    SolidBoundary::Pressure {
                        applied, ambient, ..
                    } => {
                        let delta = applied.difference_pa(*ambient);
                        face.outward.map(|n| -delta * n)
                    }
                    SolidBoundary::Displacement { .. } => {
                        unreachable!("displacement is not a load")
                    }
                };
                for node in face.nodes {
                    for (i, component) in traction.into_iter().enumerate() {
                        loads[3 * node + i] += (face.area / 3.0) * component;
                    }
                }
            }
        }
        loads
    }
}

fn supports_all_rigid_modes(vertices: &[[f64; 3]], prescribed: &[Option<f64>]) -> bool {
    let reference = vertices[0];
    let size = vertices
        .iter()
        .map(|&p| norm(&subtract(p, reference)))
        .fold(0.0, f64::max);
    let mut basis: Vec<[f64; 6]> = Vec::with_capacity(6);
    for (node, &point) in vertices.iter().enumerate() {
        let [x, y, z] = subtract(point, reference).map(|p| p / size);
        let modes = [
            [1.0, 0.0, 0.0, 0.0, z, -y],
            [0.0, 1.0, 0.0, -z, 0.0, x],
            [0.0, 0.0, 1.0, y, -x, 0.0],
        ];
        for (component, mut row) in modes.into_iter().enumerate() {
            if prescribed[3 * node + component].is_none() {
                continue;
            }
            for mode in &basis {
                let projection: f64 = row.iter().zip(mode).map(|(a, b)| a * b).sum();
                for (value, basis_value) in row.iter_mut().zip(mode) {
                    *value -= projection * basis_value;
                }
            }
            let magnitude = norm(&row);
            if magnitude > 1.0e-10 {
                basis.push(row.map(|x| x / magnitude));
            }
            if basis.len() == 6 {
                return true;
            }
        }
    }
    false
}

fn strain(tet: &Tet, u: [[f64; 3]; 4]) -> [[f64; 3]; 3] {
    // Subtract a common displacement so rigid translation cancels before multiplication.
    let differences = u.map(|value| subtract(value, u[0]));
    let gradient: [[f64; 3]; 3] = std::array::from_fn(|i| {
        std::array::from_fn(|j| {
            (1..4)
                .map(|a| differences[a][i] * tet.gradients[a][j])
                .sum()
        })
    });
    std::array::from_fn(|i| std::array::from_fn(|j| 0.5 * gradient[i][j] + 0.5 * gradient[j][i]))
}

fn von_mises_stress(s: [[f64; 3]; 3]) -> f64 {
    let diagonal = [s[0][0] - s[1][1], s[1][1] - s[2][2], s[2][2] - s[0][0]];
    norm(&[
        diagonal[0] * std::f64::consts::FRAC_1_SQRT_2,
        diagonal[1] * std::f64::consts::FRAC_1_SQRT_2,
        diagonal[2] * std::f64::consts::FRAC_1_SQRT_2,
        3.0_f64.sqrt() * s[0][1],
        3.0_f64.sqrt() * s[0][2],
        3.0_f64.sqrt() * s[1][2],
    ])
}

fn mechanical_balances(
    vertices: &[[f64; 3]],
    loads: &[f64],
    reactions: &[[f64; 3]],
) -> Result<Vec<Balance>, Error> {
    let mut force = [[0.0; 3]; 2];
    let mut moment = [[0.0; 3]; 2];
    let origin = vertices[0];
    for ((&position, applied), &reaction) in
        vertices.iter().zip(loads.chunks_exact(3)).zip(reactions)
    {
        let lever = subtract(position, origin);
        for (group, contributions) in [[applied[0], applied[1], applied[2]], reaction]
            .into_iter()
            .enumerate()
        {
            let torque = cross(lever, contributions);
            for i in 0..3 {
                force[group][i] += contributions[i];
                moment[group][i] += torque[i];
            }
        }
    }
    let mut balances = Vec::new();
    for (i, axis) in ["x", "y", "z"].into_iter().enumerate() {
        balances.push(Balance::new(
            &format!("solid_force_{axis}"),
            "N",
            [("applied", force[0][i]), ("reaction", force[1][i])],
        )?);
        balances.push(Balance::new(
            &format!("solid_moment_{axis}"),
            "N m",
            [("applied", moment[0][i]), ("reaction", moment[1][i])],
        )?);
    }
    Ok(balances)
}
