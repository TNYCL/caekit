//! Small-strain static elasticity for a straight bar with uniform cross-section.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::error::{finite, nonempty, positive};
use crate::result::{Balance, Output, Quantity, SampleLocation, Solution};
use crate::{AxisFrame, Error, GeometryRevision, LineMesh, MaterialSource, MeshMetrics};
use caekit_numerics::{ConstrainedSystem, LinearSettings, StopReason};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AxialBar {
    pub geometry: BarGeometry,
    pub mesh: LineMesh,
    pub material: ElasticMaterial,
    pub boundary_conditions: Vec<BarBoundary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarGeometry {
    pub identity: GeometryRevision,
    pub frame: AxisFrame,
    pub length_m: f64,
    pub cross_section_area_m2: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElasticMaterial {
    pub id: String,
    pub young_modulus_pa: f64,
    pub source: MaterialSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BarBoundary {
    Displacement { region: String, value_m: f64 },
    Force { region: String, value_n: f64 },
}

impl BarBoundary {
    fn region(&self) -> &str {
        match self {
            Self::Displacement { region, .. } | Self::Force { region, .. } => region,
        }
    }
}

impl AxialBar {
    pub(crate) const OUTPUTS: &[Output] = &[
        Output::AxialDisplacement,
        Output::AxialStress,
        Output::EndDisplacement,
        Output::ReactionForce,
    ];
    const METHOD: &str = "static_small_strain_axial_bar_p1_fem";

    pub(crate) fn validate(&self) -> Result<MeshMetrics, Error> {
        self.geometry
            .identity
            .validate("physics.geometry.identity")?;
        self.geometry.frame.validate("physics.geometry.frame")?;
        positive("physics.geometry.length_m", self.geometry.length_m)?;
        positive(
            "physics.geometry.cross_section_area_m2",
            self.geometry.cross_section_area_m2,
        )?;
        nonempty("physics.material.id", &self.material.id)?;
        positive(
            "physics.material.young_modulus_pa",
            self.material.young_modulus_pa,
        )?;
        self.material.source.validate("physics.material.source")?;

        let mesh = self.mesh.validate("physics.mesh", self.geometry.length_m)?;
        let mut assigned = [false; 2];
        let mut fixed = false;
        for (i, boundary) in self.boundary_conditions.iter().enumerate() {
            let path = format!("physics.boundary_conditions[{i}]");
            let end = match self.mesh.endpoint(boundary.region()) {
                Some(0) => 0,
                Some(_) => 1,
                None => {
                    return Err(Error::invalid(
                        path,
                        format!("unknown endpoint region {:?}", boundary.region()),
                    ));
                }
            };
            if std::mem::replace(&mut assigned[end], true) {
                return Err(Error::invalid(path, "endpoint already has a condition"));
            }
            match boundary {
                BarBoundary::Displacement { value_m, .. } => {
                    finite(&path, *value_m)?;
                    fixed = true;
                }
                BarBoundary::Force { value_n, .. } => finite(&path, *value_n)?,
            }
        }
        if !assigned.into_iter().all(|x| x) {
            return Err(Error::invalid(
                "physics.boundary_conditions",
                "each endpoint requires a displacement or force; specify zero for a free end",
            ));
        }
        if !fixed {
            return Err(Error::invalid(
                "physics.boundary_conditions",
                "prescribe at least one displacement to remove rigid translation",
            ));
        }
        Ok(mesh)
    }

    pub(crate) fn solve(&self, settings: LinearSettings) -> Result<Solution, Error> {
        let vertices = &self.mesh.vertices_m;
        let mut prescribed = vec![None; vertices.len()];
        let mut loads = vec![0.0; vertices.len()];
        for boundary in &self.boundary_conditions {
            let node = self
                .mesh
                .endpoint(boundary.region())
                .expect("validated endpoint");
            match boundary {
                BarBoundary::Displacement { value_m, .. } => prescribed[node] = Some(*value_m),
                BarBoundary::Force { value_n, .. } => loads[node] = *value_n,
            }
        }

        let area = self.geometry.cross_section_area_m2;
        let modulus = self.material.young_modulus_pa;
        let mut system = ConstrainedSystem::new(prescribed, &loads);
        for (element, edge) in vertices.windows(2).enumerate() {
            let stiffness = modulus * area / (edge[1] - edge[0]);
            system.add(element, element, stiffness);
            system.add(element, element + 1, -stiffness);
            system.add(element + 1, element, -stiffness);
            system.add(element + 1, element + 1, stiffness);
        }

        let (displacements, convergence) = system.solve(settings, "elastic_equilibrium", "N");
        let reason = convergence.stop_reason;
        let mut result = Solution::new(Self::METHOD, vec![convergence]);
        if reason != StopReason::Converged {
            return Ok(result);
        }

        let frame = &self.geometry.frame;
        let stresses: Vec<_> = vertices
            .windows(2)
            .zip(displacements.windows(2))
            .map(|(edge, values)| modulus * ((values[1] - values[0]) / (edge[1] - edge[0])))
            .collect();
        let mut reactions = BTreeMap::new();
        for boundary in &self.boundary_conditions {
            if let BarBoundary::Displacement { region, .. } = boundary {
                let force = if region == &self.mesh.start_region {
                    -stresses[0] * area
                } else {
                    stresses[stresses.len() - 1] * area
                };
                reactions.insert(region.clone(), force);
            }
        }
        result.balances.push(Balance::new(
            "global_axial_force",
            "N",
            [
                ("applied_force", loads.iter().sum()),
                ("support_reaction", reactions.values().sum()),
            ],
        )?);
        result.quantities.insert(
            Output::EndDisplacement,
            Quantity::ByRegion {
                unit: "m".to_owned(),
                values: BTreeMap::from([
                    (self.mesh.start_region.clone(), displacements[0]),
                    (
                        self.mesh.end_region.clone(),
                        displacements[vertices.len() - 1],
                    ),
                ]),
            },
        );
        result.quantities.insert(
            Output::AxialDisplacement,
            Quantity::Field {
                unit: "m".to_owned(),
                location: SampleLocation::Node,
                positions_m: vertices.iter().map(|&x| frame.position(x)).collect(),
                values: displacements,
            },
        );
        result.quantities.insert(
            Output::AxialStress,
            Quantity::Field {
                unit: "Pa".to_owned(),
                location: SampleLocation::Cell,
                positions_m: vertices
                    .windows(2)
                    .map(|edge| frame.position(edge[0] + 0.5 * (edge[1] - edge[0])))
                    .collect(),
                values: stresses,
            },
        );
        result.quantities.insert(
            Output::ReactionForce,
            Quantity::ByRegion {
                unit: "N".to_owned(),
                values: reactions,
            },
        );
        Ok(result)
    }
}
