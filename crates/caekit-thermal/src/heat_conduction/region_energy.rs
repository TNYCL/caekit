use std::collections::BTreeMap;

use caekit_mesh::tetra::{TetraFaceAdjacency, dot};
use caekit_numerics::{ConstrainedSystem, Convergence, LinearSettings, StopReason};
use caekit_types::{Balance, Error, InterfaceValue, Output, Quantity, Solution};

use super::nonfinite;
use super::volume::RegionMaterials;

pub(super) struct RegionEnergy<'a> {
    materials: &'a RegionMaterials<'a>,
    faces: &'a TetraFaceAdjacency<'a>,
    generated_w: Vec<f64>,
    exterior_w: Vec<f64>,
}

#[derive(Default)]
struct InterfacePower {
    from: usize,
    to: usize,
    area: f64,
    power_w: f64,
}

impl<'a> RegionEnergy<'a> {
    pub fn new(materials: &'a RegionMaterials<'a>, faces: &'a TetraFaceAdjacency<'a>) -> Self {
        let count = materials.regions.len();
        Self {
            materials,
            faces,
            generated_w: vec![0.0; count],
            exterior_w: vec![0.0; count],
        }
    }

    pub fn add_generation(&mut self, cell: usize, power: f64) {
        self.generated_w[self.materials.cell_regions[cell]] += power;
    }

    pub fn add_exterior_heat(&mut self, region: &str, face: usize, power: f64) {
        let cell = self.exterior_cell(region, face);
        self.exterior_w[self.materials.cell_regions[cell]] += power;
    }

    pub fn exterior_cell(&self, region: &str, face: usize) -> usize {
        self.faces.boundary_cells[region][face]
    }

    pub fn report(
        &self,
        fluxes: &[[f64; 3]],
        settings: LinearSettings,
        outputs: &[Output],
        solution: &mut Solution<Convergence>,
    ) -> Result<(), Error> {
        let mut pairs = self.integrate_interfaces(fluxes)?;
        if !pairs.is_empty() {
            let trace = self.equilibrate(&mut pairs, settings);
            let converged = trace.stop_reason == StopReason::Converged;
            solution.convergence.push(trace);
            if !converged {
                return Ok(());
            }
        }

        let mut exchange_w = vec![0.0; self.materials.regions.len()];
        for pair in pairs.values() {
            exchange_w[pair.from] -= pair.power_w;
            exchange_w[pair.to] += pair.power_w;
        }
        for (i, region) in self.materials.regions.iter().enumerate() {
            solution.balances.push(Balance::new(
                &format!("solid_energy:{}", region.name),
                "W",
                [
                    ("volumetric_generation", self.generated_w[i]),
                    ("exterior_heat_input", self.exterior_w[i]),
                    ("interface_heat_input", exchange_w[i]),
                ],
            )?);
        }
        if outputs.contains(&Output::InterfaceHeatRate) {
            solution.quantities.insert(
                Output::InterfaceHeatRate,
                Quantity::ByInterface {
                    unit: "W".to_owned(),
                    values: pairs
                        .into_iter()
                        .map(|((from, to), pair)| InterfaceValue {
                            from_region: from.to_owned(),
                            to_region: to.to_owned(),
                            value: pair.power_w,
                        })
                        .collect(),
                },
            );
        }
        Ok(())
    }

    fn integrate_interfaces(
        &self,
        fluxes: &[[f64; 3]],
    ) -> Result<BTreeMap<(&str, &str), InterfacePower>, Error> {
        let mut pairs = BTreeMap::<_, InterfacePower>::new();
        for face in &self.faces.interior_faces {
            let [first, second] = face.cells.map(|cell| self.materials.cell_regions[cell]);
            if first == second {
                continue;
            }
            let a = self.materials.regions[first].name.as_str();
            let b = self.materials.regions[second].name.as_str();
            let (from, to, key, sign) = if a < b {
                (first, second, (a, b), 1.0)
            } else {
                (second, first, (b, a), -1.0)
            };
            let [q1, q2] = face.cells.map(|cell| fluxes[cell]);
            let average = std::array::from_fn(|axis| 0.5 * q1[axis] + 0.5 * q2[axis]);
            let pair = pairs.entry(key).or_insert(InterfacePower {
                from,
                to,
                ..InterfacePower::default()
            });
            pair.area += face.area;
            pair.power_w += sign * face.area * dot(average, face.outward);
        }
        if pairs
            .values()
            .any(|pair| !pair.area.is_finite() || !pair.power_w.is_finite())
        {
            return Err(nonfinite("integrated interface heat rate"));
        }
        Ok(pairs)
    }

    fn equilibrate(
        &self,
        pairs: &mut BTreeMap<(&str, &str), InterfacePower>,
        settings: LinearSettings,
    ) -> Convergence {
        let mut rhs: Vec<_> = self
            .generated_w
            .iter()
            .zip(&self.exterior_w)
            .map(|(generated, exterior)| generated + exterior)
            .collect();
        // Internal transfer cannot absorb the global solid imbalance.
        let mean = rhs.iter().sum::<f64>() / rhs.len() as f64;
        for power in &mut rhs {
            *power -= mean;
        }
        for pair in pairs.values() {
            rhs[pair.from] -= pair.power_w;
            rhs[pair.to] += pair.power_w;
        }

        // Minimize the squared change in interface power weighted by inverse area.
        // The connected region graph gives one free constant in the multiplier.
        let max_area = pairs.values().map(|pair| pair.area).fold(0.0, f64::max);
        let mut anchor = vec![None; rhs.len()];
        anchor[0] = Some(0.0);
        let mut graph = ConstrainedSystem::new(anchor, &rhs);
        for pair in pairs.values() {
            let weight = pair.area / max_area;
            graph.add(pair.from, pair.from, weight);
            graph.add(pair.to, pair.to, weight);
            graph.add(pair.from, pair.to, -weight);
            graph.add(pair.to, pair.from, -weight);
        }
        let (correction, trace) = graph.solve(settings, "solid_interface_equilibration", "W");
        if trace.stop_reason == StopReason::Converged {
            for pair in pairs.values_mut() {
                pair.power_w +=
                    (pair.area / max_area) * (correction[pair.from] - correction[pair.to]);
            }
        }
        trace
    }
}
