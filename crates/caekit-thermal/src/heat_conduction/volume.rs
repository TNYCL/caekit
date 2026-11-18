use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{ConductiveMaterial, ThermalBoundary};
use caekit_types::Error;
use caekit_types::error::{finite, nonempty, positive};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MultimaterialThermal {
    pub materials: Vec<ConductiveMaterial>,
    pub volume_regions: Vec<ThermalVolumeRegion>,
    pub boundary_conditions: Vec<ThermalBoundary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThermalVolumeRegion {
    pub name: String,
    pub material_id: String,
    /// Zero-based indices into the tetrahedral mesh's cells.
    pub cells: Vec<usize>,
    #[serde(default)]
    pub volumetric_heat_source_w_m3: f64,
}

#[derive(Clone, Copy)]
pub(super) struct CellProperties {
    pub conductivity: f64,
    pub generation: f64,
}

pub(super) enum CellMaterials {
    Uniform(CellProperties),
    ByRegion(RegionMaterials),
}

pub(super) struct RegionMaterials {
    pub properties: Vec<CellProperties>,
    pub cell_regions: Vec<usize>,
}

impl CellMaterials {
    pub fn at(&self, cell: usize) -> CellProperties {
        match self {
            Self::Uniform(properties) => *properties,
            Self::ByRegion(regions) => regions.properties[regions.cell_regions[cell]],
        }
    }
}

pub(super) fn validate_material(path: &str, material: &ConductiveMaterial) -> Result<(), Error> {
    nonempty(&format!("{path}.id"), &material.id)?;
    positive(
        &format!("{path}.thermal_conductivity_w_m_k"),
        material.thermal_conductivity_w_m_k,
    )?;
    material.source.validate(&format!("{path}.source"))
}

impl MultimaterialThermal {
    pub(super) fn resolve(&self, cell_count: usize) -> Result<RegionMaterials, Error> {
        if self.materials.is_empty() {
            return Err(Error::invalid(
                "physics.thermal.materials",
                "requires at least one material",
            ));
        }
        let mut materials = BTreeMap::new();
        for (i, material) in self.materials.iter().enumerate() {
            let path = format!("physics.thermal.materials[{i}]");
            validate_material(&path, material)?;
            if materials.insert(material.id.as_str(), material).is_some() {
                return Err(Error::invalid(
                    format!("{path}.id"),
                    format!("duplicate material ID {:?}", material.id),
                ));
            }
        }

        let mut names = BTreeSet::new();
        let mut cell_regions = vec![usize::MAX; cell_count];
        let mut properties = Vec::with_capacity(self.volume_regions.len());
        for (i, region) in self.volume_regions.iter().enumerate() {
            let path = format!("physics.thermal.volume_regions[{i}]");
            nonempty(&format!("{path}.name"), &region.name)?;
            if !names.insert(region.name.as_str()) {
                return Err(Error::invalid(
                    format!("{path}.name"),
                    format!("duplicate volume region name {:?}", region.name),
                ));
            }
            let &material = materials.get(region.material_id.as_str()).ok_or_else(|| {
                Error::invalid(
                    format!("{path}.material_id"),
                    format!(
                        "volume region {:?} references unknown material {:?}",
                        region.name, region.material_id
                    ),
                )
            })?;
            finite(
                &format!("{path}.volumetric_heat_source_w_m3"),
                region.volumetric_heat_source_w_m3,
            )?;
            if region.cells.is_empty() {
                return Err(Error::invalid(
                    format!("{path}.cells"),
                    format!("volume region {:?} has no cells", region.name),
                ));
            }
            for (j, &cell) in region.cells.iter().enumerate() {
                let owner = cell_regions.get_mut(cell).ok_or_else(|| {
                    Error::invalid(
                        format!("{path}.cells[{j}]"),
                        format!(
                            "volume region {:?} references invalid cell {cell}",
                            region.name
                        ),
                    )
                })?;
                if *owner != usize::MAX {
                    return Err(Error::invalid(
                        format!("{path}.cells[{j}]"),
                        format!(
                            "volume region {:?} assigns cell {cell}, already assigned to {:?}",
                            region.name, self.volume_regions[*owner].name
                        ),
                    ));
                }
                *owner = i;
            }
            properties.push(CellProperties {
                conductivity: material.thermal_conductivity_w_m_k,
                generation: region.volumetric_heat_source_w_m3,
            });
        }
        if let Some(cell) = cell_regions.iter().position(|&owner| owner == usize::MAX) {
            return Err(Error::invalid(
                "physics.thermal.volume_regions",
                format!("cell {cell} has no volume region assignment"),
            ));
        }
        Ok(RegionMaterials {
            properties,
            cell_regions,
        })
    }
}
