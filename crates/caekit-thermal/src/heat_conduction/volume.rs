use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{ConductiveMaterial, ThermalBoundary};
use caekit_types::Error;
use caekit_types::error::{finite, nonempty, positive};

/// Thermal definition of a multimaterial solid, read from `physics.thermal` in the
/// `multimaterial_heat_conduction` case.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MultimaterialThermal {
    /// Materials that the volume regions can refer to. Must contain at least one entry, and the
    /// `id` values must be unique. Several regions may use the same material.
    pub materials: Vec<ConductiveMaterial>,
    /// Partition of the mesh cells into named regions, each with one material and one heat source.
    /// Every cell must belong to exactly one region. Two different regions form an interface
    /// wherever their cells share a face, including when they use the same material.
    pub volume_regions: Vec<ThermalVolumeRegion>,
    /// Conditions on named exterior regions of the mesh, at most one per region. A region without a
    /// condition is insulated. At least one `temperature` or `convection` condition is required to
    /// determine the absolute temperature.
    pub boundary_conditions: Vec<ThermalBoundary>,
}

/// A named set of mesh cells that share one material and one uniform heat source.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThermalVolumeRegion {
    /// Region name, nonempty and unique among the volume regions. It labels the region's
    /// `solid_energy:<name>` balance and identifies it in interface heat rates, whose direction
    /// follows the lexicographic order of the two names.
    pub name: String,
    /// The `id` of the entry of `materials` whose conductivity applies to the region's cells. It
    /// must match an entry exactly.
    pub material_id: String,
    /// Zero-based indices into the tetrahedral mesh's cells.
    /// Must be nonempty and in range. Each mesh cell must be listed exactly once across all
    /// regions.
    pub cells: Vec<usize>,
    /// Uniform volumetric heat generation in the region, in W/m³. Positive values generate heat and
    /// negative values remove it. Must be finite. Defaults to 0 when omitted.
    #[serde(default)]
    pub volumetric_heat_source_w_m3: f64,
}

#[derive(Clone, Copy)]
pub(super) struct CellProperties {
    pub conductivity: f64,
    pub generation: f64,
}

pub(super) enum CellMaterials<'a> {
    Uniform(CellProperties),
    ByRegion(RegionMaterials<'a>),
}

pub(super) struct RegionMaterials<'a> {
    pub regions: &'a [ThermalVolumeRegion],
    pub properties: Vec<CellProperties>,
    pub cell_regions: Vec<usize>,
}

impl CellMaterials<'_> {
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
    pub(super) fn resolve(&self, cell_count: usize) -> Result<RegionMaterials<'_>, Error> {
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
            regions: &self.volume_regions,
            properties,
            cell_regions,
        })
    }
}
