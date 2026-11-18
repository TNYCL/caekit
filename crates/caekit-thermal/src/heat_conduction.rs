//! Steady isotropic heat conduction with linear tetrahedral elements.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::ConductiveMaterial;
use caekit_mesh::tetra::{CheckedTetraMesh, Tet, TetraTopology, dot};
use caekit_mesh::{GeometryRevision, MeshMetrics, TetraMesh};
use caekit_numerics::{ConstrainedSystem, Convergence, LinearSettings, StopReason};
use caekit_types::error::{finite, nonempty, positive};
use caekit_types::{Balance, Error, Output, Quantity, SampleLocation, Solution};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeatConduction {
    pub geometry: GeometryRevision,
    pub mesh: TetraMesh,
    pub thermal: SolidThermal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SolidThermal {
    pub material: ConductiveMaterial,
    pub boundary_conditions: Vec<ThermalBoundary>,
    #[serde(default)]
    pub volumetric_heat_source_w_m3: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ThermalBoundary {
    Temperature {
        region: String,
        temperature_k: f64,
    },
    HeatFlux {
        region: String,
        /// Positive into the solid, opposite the outward normal.
        inward_heat_flux_w_m2: f64,
    },
    Insulation {
        region: String,
    },
    Convection {
        region: String,
        ambient_temperature_k: f64,
        film_coefficient_w_m2_k: f64,
    },
}

impl ThermalBoundary {
    fn region(&self) -> &str {
        match self {
            Self::Temperature { region, .. }
            | Self::HeatFlux { region, .. }
            | Self::Insulation { region }
            | Self::Convection { region, .. } => region,
        }
    }
}

/// Temperature samples follow the checked mesh's vertex order.
/// Keeping the offset separate preserves temperature differences for coupling.
pub struct SolidTemperature {
    datum_k: f64,
    offsets_k: Vec<f64>,
}

impl SolidTemperature {
    pub fn datum_k(&self) -> f64 {
        self.datum_k
    }

    pub fn offsets_k(&self) -> &[f64] {
        &self.offsets_k
    }
}

pub struct PreparedConduction<'a> {
    mesh: &'a TetraMesh,
    topology: &'a TetraTopology<'a>,
    thermal: &'a SolidThermal,
    conditions: BTreeMap<&'a str, &'a ThermalBoundary>,
    prescribed: Vec<Option<f64>>,
    datum_k: f64,
}

impl HeatConduction {
    pub fn validate(&self) -> Result<MeshMetrics, Error> {
        let mesh = self.checked_mesh()?;
        self.thermal.prepare(&mesh)?;
        Ok(mesh.into_metrics())
    }

    pub fn solve(
        &self,
        settings: LinearSettings,
        outputs: &[Output],
    ) -> Result<(MeshMetrics, Solution<Convergence>), Error> {
        let mesh = self.checked_mesh()?;
        let (solution, _) = self.thermal.prepare(&mesh)?.solve(settings, outputs)?;
        Ok((mesh.into_metrics(), solution))
    }

    fn checked_mesh(&self) -> Result<CheckedTetraMesh<'_>, Error> {
        self.geometry.validate("physics.geometry")?;
        self.mesh.checked("physics.mesh")
    }
}

impl SolidThermal {
    pub const OUTPUTS: &[Output] = &[
        Output::SolidTemperature,
        Output::SolidHeatFlux,
        Output::BoundaryHeatRate,
    ];
    const METHOD: &str = "steady_isotropic_heat_conduction_p1_tetrahedral_fem";

    pub fn prepare<'a>(
        &'a self,
        checked: &'a CheckedTetraMesh<'a>,
    ) -> Result<PreparedConduction<'a>, Error> {
        nonempty("physics.thermal.material.id", &self.material.id)?;
        positive(
            "physics.thermal.material.thermal_conductivity_w_m_k",
            self.material.thermal_conductivity_w_m_k,
        )?;
        self.material
            .source
            .validate("physics.thermal.material.source")?;
        finite(
            "physics.thermal.volumetric_heat_source_w_m3",
            self.volumetric_heat_source_w_m3,
        )?;

        let (mesh, topology) = (checked.mesh(), checked.topology());
        let mut prescribed = vec![None; mesh.vertices_m.len()];
        let mut conditions = BTreeMap::new();
        let mut fixed_datum = None;
        let mut ambient_datum = None;
        for (i, condition) in self.boundary_conditions.iter().enumerate() {
            let path = format!("physics.thermal.boundary_conditions[{i}]");
            let region = condition.region();
            let faces = topology.boundaries.get(region).ok_or_else(|| {
                Error::invalid(&path, format!("unknown exterior region {region:?}"))
            })?;
            if conditions.insert(region, condition).is_some() {
                return Err(Error::invalid(
                    path,
                    format!("region {region:?} already has a thermal boundary condition"),
                ));
            }
            match *condition {
                ThermalBoundary::Temperature { temperature_k, .. } => {
                    positive(&format!("{path}.temperature_k"), temperature_k)?;
                    for node in faces.iter().flat_map(|face| face.nodes) {
                        if prescribed[node].is_some_and(|other| other != temperature_k) {
                            return Err(Error::invalid(
                                path,
                                format!("conflicting prescribed temperatures at vertex {node}"),
                            ));
                        }
                        prescribed[node] = Some(temperature_k);
                    }
                    fixed_datum.get_or_insert(temperature_k);
                }
                ThermalBoundary::HeatFlux {
                    inward_heat_flux_w_m2,
                    ..
                } => finite(
                    &format!("{path}.inward_heat_flux_w_m2"),
                    inward_heat_flux_w_m2,
                )?,
                ThermalBoundary::Insulation { .. } => {}
                ThermalBoundary::Convection {
                    ambient_temperature_k,
                    film_coefficient_w_m2_k,
                    ..
                } => {
                    positive(
                        &format!("{path}.ambient_temperature_k"),
                        ambient_temperature_k,
                    )?;
                    positive(
                        &format!("{path}.film_coefficient_w_m2_k"),
                        film_coefficient_w_m2_k,
                    )?;
                    positive(
                        &format!("{path} convection surface area"),
                        faces.iter().map(|face| face.area).sum(),
                    )?;
                    if faces.iter().any(|face| {
                        let weight = film_weight(face.area, film_coefficient_w_m2_k);
                        !weight.is_finite() || weight <= 0.0
                    }) {
                        return Err(Error::invalid(
                            path,
                            "convection face conductance must be positive and finite",
                        ));
                    }
                    ambient_datum.get_or_insert(ambient_temperature_k);
                }
            }
        }
        let datum_k = match (fixed_datum, ambient_datum) {
            (Some(k), _) => k,
            (None, Some(ambient)) => convection_datum(
                ambient,
                self.volumetric_heat_source_w_m3,
                topology,
                &conditions,
            ),
            (None, None) => {
                return Err(Error::invalid(
                    "physics.thermal.boundary_conditions",
                    "a prescribed temperature or positive convection is required to determine absolute temperature",
                ));
            }
        };
        Ok(PreparedConduction {
            mesh,
            topology,
            thermal: self,
            conditions,
            prescribed,
            datum_k,
        })
    }
}

impl PreparedConduction<'_> {
    pub fn solve(
        &self,
        settings: LinearSettings,
        outputs: &[Output],
    ) -> Result<(Solution<Convergence>, Option<SolidTemperature>), Error> {
        let loads = self.assemble_loads()?;
        let prescribed = self
            .prescribed
            .iter()
            .map(|value| value.map(|k| k - self.datum_k))
            .collect();
        let mut system = ConstrainedSystem::new(prescribed, &loads);
        let k = self.thermal.material.thermal_conductivity_w_m_k;
        for (tet, nodes) in self.topology.elements.iter().zip(&self.mesh.cells) {
            for (a, &row) in nodes.iter().enumerate() {
                for (b, &column) in nodes.iter().enumerate() {
                    let entry = (tet.volume * dot(tet.gradients[a], tet.gradients[b])) * k;
                    if !entry.is_finite() {
                        return Err(nonfinite("solid thermal stiffness"));
                    }
                    system.add(row, column, entry);
                }
            }
        }
        for (&region, &condition) in &self.conditions {
            if let ThermalBoundary::Convection {
                film_coefficient_w_m2_k,
                ..
            } = condition
            {
                for face in &self.topology.boundaries[region] {
                    let weight = film_weight(face.area, *film_coefficient_w_m2_k);
                    for row in face.nodes {
                        for column in face.nodes {
                            system.add(
                                row,
                                column,
                                if row == column { 2.0 * weight } else { weight },
                            );
                        }
                    }
                }
            }
        }

        let (offsets_k, convergence) = system.solve(settings, "solid_heat_conduction", "W");
        let converged = convergence.stop_reason == StopReason::Converged;
        let mut solution = Solution::new(SolidThermal::METHOD, vec![convergence]);
        if !converged {
            return Ok((solution, None));
        }
        let mut temperatures_k = Vec::new();
        let want_temperature = outputs.contains(&Output::SolidTemperature);
        for (node, (&offset, prescribed)) in offsets_k.iter().zip(&self.prescribed).enumerate() {
            let temperature = prescribed.unwrap_or(self.datum_k + offset);
            if !temperature.is_finite() {
                return Err(nonfinite("solid temperature"));
            }
            if temperature <= 0.0 {
                return Err(Error::invalid(
                    format!("solid temperature at vertex {node}"),
                    "solution must be above absolute zero",
                ));
            }
            if want_temperature {
                temperatures_k.push(temperature);
            }
        }
        if want_temperature {
            solution.quantities.insert(
                Output::SolidTemperature,
                Quantity::field(
                    "K",
                    SampleLocation::Node,
                    self.mesh.vertices_m.clone(),
                    temperatures_k,
                ),
            );
        }
        self.heat_rates(&offsets_k, outputs, &mut solution)?;
        Ok((
            solution,
            Some(SolidTemperature {
                datum_k: self.datum_k,
                offsets_k,
            }),
        ))
    }

    fn assemble_loads(&self) -> Result<Vec<f64>, Error> {
        let mut loads = vec![0.0; self.prescribed.len()];
        let generation = self.thermal.volumetric_heat_source_w_m3;
        for (tet, nodes) in self.topology.elements.iter().zip(&self.mesh.cells) {
            for &node in nodes {
                loads[node] += (tet.volume / 4.0) * generation;
            }
        }
        for (&region, &condition) in &self.conditions {
            for face in &self.topology.boundaries[region] {
                let load = match *condition {
                    ThermalBoundary::HeatFlux {
                        inward_heat_flux_w_m2,
                        ..
                    } => (face.area / 3.0) * inward_heat_flux_w_m2,
                    ThermalBoundary::Convection {
                        ambient_temperature_k,
                        film_coefficient_w_m2_k,
                        ..
                    } => {
                        4.0 * film_weight(face.area, film_coefficient_w_m2_k)
                            * (ambient_temperature_k - self.datum_k)
                    }
                    _ => continue,
                };
                for node in face.nodes {
                    loads[node] += load;
                }
            }
        }
        if loads.iter().all(|value| value.is_finite()) {
            Ok(loads)
        } else {
            Err(nonfinite("solid thermal loads"))
        }
    }

    fn heat_rates(
        &self,
        offsets_k: &[f64],
        outputs: &[Output],
        solution: &mut Solution<Convergence>,
    ) -> Result<(), Error> {
        let want_flux = outputs.contains(&Output::SolidHeatFlux);
        let mut fluxes = Vec::new();
        let mut reactions = vec![0.0; self.prescribed.len()];
        let mut generation_w = 0.0;
        let k = self.thermal.material.thermal_conductivity_w_m_k;
        for (tet, &nodes) in self.topology.elements.iter().zip(&self.mesh.cells) {
            let flux = heat_flux(tet, k, nodes.map(|node| offsets_k[node]));
            if !flux.iter().all(|component| component.is_finite()) {
                return Err(nonfinite("solid heat flux"));
            }
            let generated = tet.volume * self.thermal.volumetric_heat_source_w_m3;
            generation_w += generated;
            for (a, node) in nodes.into_iter().enumerate() {
                if self.prescribed[node].is_some() {
                    reactions[node] -= tet.volume * dot(tet.gradients[a], flux) + 0.25 * generated;
                }
            }
            if want_flux {
                fluxes.push(flux);
            }
        }

        let mut flux_w = 0.0;
        let mut convection_w = 0.0;
        let mut regions = BTreeMap::new();
        let want_rates = outputs.contains(&Output::BoundaryHeatRate);
        for (&region, faces) in &self.topology.boundaries {
            let mut power = 0.0;
            match self.conditions.get(region).copied() {
                Some(&ThermalBoundary::HeatFlux {
                    inward_heat_flux_w_m2,
                    ..
                }) => {
                    for face in faces {
                        let load = (face.area / 3.0) * inward_heat_flux_w_m2;
                        for node in face.nodes {
                            reactions[node] -= load;
                        }
                        power += 3.0 * load;
                    }
                    flux_w += power;
                }
                Some(&ThermalBoundary::Convection {
                    ambient_temperature_k,
                    film_coefficient_w_m2_k,
                    ..
                }) => {
                    let ambient_offset = ambient_temperature_k - self.datum_k;
                    for face in faces {
                        let weight = film_weight(face.area, film_coefficient_w_m2_k);
                        let delta = face
                            .nodes
                            .map(|node| weight * (ambient_offset - offsets_k[node]));
                        let sum: f64 = delta.into_iter().sum();
                        for (node, own) in face.nodes.into_iter().zip(delta) {
                            reactions[node] -= sum + own;
                        }
                        power += 4.0 * sum;
                    }
                    convection_w += power;
                }
                _ => {}
            }
            if want_rates {
                regions.insert(region.to_owned(), power);
            }
        }

        let reaction_w = reactions
            .iter()
            .zip(&self.prescribed)
            .filter_map(|(&heat, value)| value.is_some().then_some(heat))
            .sum();
        solution.balances.push(Balance::new(
            "solid_energy",
            "W",
            [
                ("volumetric_generation", generation_w),
                ("prescribed_heat_flux", flux_w),
                ("convection", convection_w),
                ("prescribed_temperature_reaction", reaction_w),
            ],
        )?);
        if want_rates {
            self.distribute_reactions(&reactions, &mut regions)?;
            solution
                .quantities
                .insert(Output::BoundaryHeatRate, Quantity::by_region("W", regions));
        }
        if want_flux {
            solution.quantities.insert(
                Output::SolidHeatFlux,
                Quantity::VectorField {
                    unit: "W/m^2".to_owned(),
                    location: SampleLocation::Cell,
                    positions_m: self
                        .topology
                        .elements
                        .iter()
                        .map(|tet| tet.centroid)
                        .collect(),
                    values: fluxes,
                },
            );
        }
        Ok(())
    }

    fn distribute_reactions(
        &self,
        reactions: &[f64],
        regions: &mut BTreeMap<String, f64>,
    ) -> Result<(), Error> {
        let fixed_regions = self
            .conditions
            .iter()
            .filter(|(_, condition)| matches!(condition, ThermalBoundary::Temperature { .. }))
            .map(|(&region, _)| (region, &self.topology.boundaries[region]));
        // A vertex's reaction is shared by its prescribed-temperature triangles in area proportion.
        let mut nodal_area = vec![0.0; self.prescribed.len()];
        for (_, faces) in fixed_regions.clone() {
            for face in faces {
                for node in face.nodes {
                    nodal_area[node] += face.area;
                }
            }
        }
        if !nodal_area.iter().all(|area| area.is_finite()) {
            return Err(nonfinite("prescribed-temperature tributary area"));
        }
        for (region, faces) in fixed_regions {
            let heat = faces
                .iter()
                .flat_map(|face| {
                    face.nodes
                        .map(|node| reactions[node] * (face.area / nodal_area[node]))
                })
                .sum();
            *regions.get_mut(region).expect("known boundary region") = heat;
        }
        Ok(())
    }
}

fn film_weight(area: f64, coefficient: f64) -> f64 {
    // On a linear triangle, the consistent surface mass diagonal is twice the off-diagonal.
    (area / 12.0) * coefficient
}

// With no fixed-temperature boundary, energy conservation determines the
// h-weighted mean surface temperature. Remove this constant mode before PCG.
fn convection_datum(
    ambient: f64,
    source: f64,
    topology: &TetraTopology<'_>,
    conditions: &BTreeMap<&str, &ThermalBoundary>,
) -> f64 {
    let mut heat: f64 = topology
        .elements
        .iter()
        .map(|tet| source * tet.volume)
        .sum();
    let mut conductance = 0.0;
    for (&region, &condition) in conditions {
        for face in &topology.boundaries[region] {
            match *condition {
                ThermalBoundary::HeatFlux {
                    inward_heat_flux_w_m2,
                    ..
                } => {
                    heat += face.area * inward_heat_flux_w_m2;
                }
                ThermalBoundary::Convection {
                    film_coefficient_w_m2_k,
                    ambient_temperature_k,
                    ..
                } => {
                    let h_area = face.area * film_coefficient_w_m2_k;
                    conductance += h_area;
                    heat += h_area * (ambient_temperature_k - ambient);
                }
                _ => {}
            }
        }
    }
    let mean = ambient + heat / conductance;
    if conductance.is_finite() && mean.is_finite() && mean > 0.0 {
        mean
    } else {
        ambient
    }
}

fn heat_flux(tet: &Tet, k: f64, offsets: [f64; 4]) -> [f64; 3] {
    std::array::from_fn(|axis| {
        -k * (1..4)
            .map(|a| (offsets[a] - offsets[0]) * tet.gradients[a][axis])
            .sum::<f64>()
    })
}

fn nonfinite(quantity: &str) -> Error {
    Error::NonFiniteResult {
        quantity: quantity.to_owned(),
    }
}
