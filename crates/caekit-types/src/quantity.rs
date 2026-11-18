use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Error;

/// Result quantity that a model can request and a solver can report.
///
/// Names are written in snake_case in JSON (`AxialStress` is `axial_stress`). They are the entries
/// of a model's `outputs` list and the keys of the `quantities` map in a run report, where they
/// appear in the declaration order below. Each physics case offers a subset, named in the variant
/// docs together with the [`Quantity`] kind, unit and sampling its solver produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Output {
    /// `axial_bar`: displacement along the positive local axis in m, a [`Quantity::Field`] at the
    /// mesh vertices in input order.
    AxialDisplacement,
    /// `axial_bar`: axial stress `E (u_right − u_left)/h` in Pa, tension positive, a
    /// [`Quantity::Field`] at the element midpoints in mesh order.
    AxialStress,
    /// `axial_bar`: axial displacement in m of the two bar ends, a [`Quantity::ByRegion`] keyed by
    /// the mesh's start and end region names.
    EndDisplacement,
    /// `axial_bar`: force in N that the support exerts on the bar, positive along the local axis, a
    /// [`Quantity::ByRegion`] with one entry per end that has a prescribed displacement.
    ReactionForce,
    /// `laminar_channel` and `cooling_channel`: axial velocity in m/s, a [`Quantity::Field`] at the
    /// transverse cell centres, positioned at mid-length of the channel.
    AxialVelocity,
    /// `laminar_channel` and `cooling_channel`: volume flow rate from inlet to outlet in m³/s (unit
    /// text `m^3/s`), a [`Quantity::Scalar`].
    VolumeFlowRate,
    /// `laminar_channel` and `cooling_channel`: mass flow rate in kg/s, the density times the
    /// volume flow rate, a [`Quantity::Scalar`].
    MassFlowRate,
    /// `laminar_channel` and `cooling_channel`: inlet pressure minus outlet pressure in Pa, a
    /// [`Quantity::Scalar`].
    PressureLoss,
    /// `laminar_channel` and `cooling_channel`: absolute pressure in Pa at the two ports, a
    /// [`Quantity::ByRegion`] keyed by the inlet and outlet region names.
    AbsolutePressure,
    /// `laminar_channel` and `cooling_channel`: Reynolds number from the mean velocity and the
    /// hydraulic diameter of twice the gap, a dimensionless [`Quantity::Scalar`] with unit text
    /// `1`.
    ChannelReynolds,
    /// `laminar_channel` and `cooling_channel`: viscous force in N that each plate exerts on the
    /// fluid along the axial direction, negative when it opposes the flow, a [`Quantity::ByRegion`]
    /// keyed by the transverse mesh's start and end region names.
    WallDrag,
    /// `cooling_channel`: coolant bulk temperature in K, a [`Quantity::Field`] at the cell centres
    /// of the coolant axial mesh, positioned at mid-gap.
    FluidBulkTemperature,
    /// `cooling_channel`: wall temperature in K, a [`Quantity::Field`] at
    /// [`SampleLocation::Sample`] points: for each axial wall panel centre in mesh order, one value
    /// per vertex of the through-thickness mesh, whose coordinate is zero at the hot face.
    WallTemperature,
    /// `cooling_channel`: coolant bulk temperature in K in the last cell of the coolant axial mesh,
    /// a [`Quantity::Scalar`].
    OutletTemperature,
    /// `cooling_channel`: total heat rate in W passing from the wall to the coolant, positive when
    /// the coolant is heated, a [`Quantity::Scalar`].
    HeatTransferRate,
    /// `cooling_channel`: heat flux in W/m² (unit text `W/m^2`) leaving the wall towards the
    /// coolant, positive when the coolant is heated, a [`Quantity::Field`] at
    /// [`SampleLocation::InterfaceFace`] centres of the axial wall panels on the wetted face.
    InterfaceHeatFlux,
    /// `elastic_solid` and `thermoelastic_solid`: displacement in m, a [`Quantity::VectorField`] of
    /// global components at the mesh vertices in input order.
    DisplacementVector,
    /// `elastic_solid` and `thermoelastic_solid`: Cauchy stress in Pa, tension positive, a
    /// [`Quantity::TensorField`] of global components at the tetrahedron centroids in cell order.
    CauchyStress,
    /// `elastic_solid` and `thermoelastic_solid`: von Mises equivalent stress in Pa, a
    /// [`Quantity::Field`] at the tetrahedron centroids in cell order.
    VonMisesStress,
    /// `elastic_solid` and `thermoelastic_solid`: support reaction in N acting on the solid, a
    /// [`Quantity::VectorField`] at the mesh vertices with positive components along the global
    /// axes and zero on unconstrained components.
    NodalReaction,
    /// `elastic_solid` and `thermoelastic_solid`: elastic strain energy in J integrated over the
    /// solid, a [`Quantity::Scalar`].
    ElasticEnergy,
    /// `heat_conduction`, `multimaterial_heat_conduction` and `thermoelastic_solid`: absolute
    /// temperature in K, a [`Quantity::Field`] at the mesh vertices in input order.
    SolidTemperature,
    /// `heat_conduction`, `multimaterial_heat_conduction` and `thermoelastic_solid`: heat flux
    /// `q = −k∇T` in W/m² (unit text `W/m^2`), a [`Quantity::VectorField`] of global components at
    /// the tetrahedron centroids in cell order.
    SolidHeatFlux,
    /// `heat_conduction`, `multimaterial_heat_conduction` and `thermoelastic_solid`: integrated
    /// heat rate in W through each named exterior boundary region, positive into the solid, a
    /// [`Quantity::ByRegion`].
    BoundaryHeatRate,
    /// `multimaterial_heat_conduction`: integrated heat rate in W across the shared faces of each
    /// pair of volume regions, a [`Quantity::ByInterface`] ordered by region names and positive
    /// from `from_region` to `to_region`.
    InterfaceHeatRate,
}

/// Kind of point at which the values of a field quantity are sampled.
///
/// Written in snake_case in JSON, for example `interface_face`. The coordinates of the points are
/// the quantity's `positions_m`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SampleLocation {
    /// Mesh vertices, in the vertex order of the mesh.
    Node,
    /// Cell centres in cell order: element midpoints of a line mesh or centroids of tetrahedra.
    Cell,
    /// Centres of the faces of an interface between two coupled domains, as used for the wetted
    /// wall panels of the cooling channel.
    InterfaceFace,
    /// Points listed in `positions_m` that are not the vertices, cells or interface faces of one
    /// mesh, as used for the cooling-channel wall temperature.
    Sample,
}

/// A reported value with its unit and, for the field kinds, its sample positions.
///
/// The variant is selected by the `kind` key in JSON, written in snake_case: `scalar`, `field`,
/// `vector_field`, `tensor_field`, `by_region` or `by_interface`. The solvers fill `positions_m`
/// and `values` of the field kinds as parallel lists, so entry `i` of `values` belongs to position
/// `i`; the type itself does not enforce equal lengths.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Quantity {
    /// A single number for the whole case.
    Scalar {
        /// Unit of `value` as text, such as `Pa` or `m^3/s`; a dimensionless value uses `1`.
        unit: String,
        /// The value, in `unit`.
        value: f64,
    },
    /// One scalar at each sample point.
    Field {
        /// Unit of every entry of `values`, as text.
        unit: String,
        /// Kind of point the samples are taken at.
        location: SampleLocation,
        /// Global Cartesian coordinates in m of each sample point, in the order of `values`.
        positions_m: Vec<[f64; 3]>,
        /// Scalar at each sample point, in `unit`.
        values: Vec<f64>,
    },
    /// One three-component vector at each sample point.
    VectorField {
        /// Unit of every component in `values`, as text.
        unit: String,
        /// Kind of point the samples are taken at.
        location: SampleLocation,
        /// Global Cartesian coordinates in m of each sample point, in the order of `values`.
        positions_m: Vec<[f64; 3]>,
        /// Global `[x, y, z]` components at each sample point, in `unit`.
        values: Vec<[f64; 3]>,
    },
    /// One 3×3 tensor at each sample point.
    TensorField {
        /// Unit of every component in `values`, as text.
        unit: String,
        /// Kind of point the samples are taken at.
        location: SampleLocation,
        /// Global Cartesian coordinates in m of each sample point, in the order of `values`.
        positions_m: Vec<[f64; 3]>,
        /// Global components at each sample point as three rows, indexed `[row][column]`, in
        /// `unit`.
        values: Vec<[[f64; 3]; 3]>,
    },
    /// One scalar for each named region.
    ByRegion {
        /// Unit of every entry of `values`, as text.
        unit: String,
        /// Value in `unit` for each region name. JSON keys appear in sorted name order.
        values: BTreeMap<String, f64>,
    },
    /// One scalar for each pair of regions that exchange the quantity.
    ByInterface {
        /// Unit of the `value` of every entry, as text.
        unit: String,
        /// One entry per pair of regions, each stating its own positive direction.
        values: Vec<InterfaceValue>,
    },
}

/// A value transferred between two named regions, with its positive direction.
///
/// For `interface_heat_rate` the names are volume regions and `from_region` sorts lexicographically
/// before `to_region`.
#[derive(Debug, Serialize, Deserialize)]
pub struct InterfaceValue {
    /// Name of the region that a positive `value` leaves.
    pub from_region: String,
    /// Name of the region that a positive `value` enters.
    pub to_region: String,
    /// Positive for transfer from `from_region` to `to_region`.
    pub value: f64,
}

impl Quantity {
    /// Builds a [`Quantity::Scalar`] from a unit text and a value.
    pub fn scalar(unit: &str, value: f64) -> Self {
        Self::Scalar {
            unit: unit.to_owned(),
            value,
        }
    }

    /// Builds a [`Quantity::Field`] from a unit text, sample location, positions and values.
    ///
    /// `positions_m` and `values` are expected to have equal length and matching order; this is not
    /// checked.
    pub fn field(
        unit: &str,
        location: SampleLocation,
        positions_m: Vec<[f64; 3]>,
        values: Vec<f64>,
    ) -> Self {
        Self::Field {
            unit: unit.to_owned(),
            location,
            positions_m,
            values,
        }
    }

    /// Builds a [`Quantity::ByRegion`] from a unit text and `(region name, value)` pairs.
    ///
    /// A name that occurs more than once keeps its last value.
    pub fn by_region(unit: &str, values: impl IntoIterator<Item = (String, f64)>) -> Self {
        Self::ByRegion {
            unit: unit.to_owned(),
            values: values.into_iter().collect(),
        }
    }

    /// Returns `true` when every value and, for the field kinds, every position coordinate is
    /// finite.
    ///
    /// NaN and infinities make it `false`. Units and region names are not inspected.
    pub fn all_finite(&self) -> bool {
        match self {
            Self::Scalar { value, .. } => value.is_finite(),
            Self::Field {
                positions_m,
                values,
                ..
            } => {
                positions_m.iter().flatten().all(|x| x.is_finite())
                    && values.iter().all(|x| x.is_finite())
            }
            Self::VectorField {
                positions_m,
                values,
                ..
            } => {
                positions_m.iter().flatten().all(|x| x.is_finite())
                    && values.iter().flatten().all(|x| x.is_finite())
            }
            Self::TensorField {
                positions_m,
                values,
                ..
            } => {
                positions_m.iter().flatten().all(|x| x.is_finite())
                    && values.iter().flatten().flatten().all(|x| x.is_finite())
            }
            Self::ByRegion { values, .. } => values.values().all(|x| x.is_finite()),
            Self::ByInterface { values, .. } => values.iter().all(|entry| entry.value.is_finite()),
        }
    }
}

/// Named signed contributions to a conserved quantity and their sum, as reported by a solver.
///
/// A run report lists the balances of a converged solution independently of the requested outputs.
/// The sum shows how closely the discrete solution conserves the quantity.
#[derive(Debug, Serialize, Deserialize)]
pub struct Balance {
    /// Identifier of the balance, such as `solid_energy` or `global_axial_force`. The per-region
    /// energy balances append the volume region name after a colon.
    pub name: String,
    /// Unit of every term and of `imbalance`, as text, for example `W` or `N`.
    pub unit: String,
    /// Signed contribution of each named term, in `unit`. JSON keys appear in sorted name order.
    pub terms: BTreeMap<String, f64>,
    /// Signed sum of the terms under the solver's stated sign convention.
    pub imbalance: f64,
}

impl Balance {
    /// Builds a balance whose `imbalance` is the sum of its terms.
    ///
    /// Terms are stored by name, so a name that occurs more than once keeps its last value, and the
    /// sum is taken over the stored terms in sorted name order. Returns [`Error::NonFiniteResult`]
    /// carrying `name` when the sum is NaN or infinite.
    pub fn new(
        name: &str,
        unit: &str,
        terms: impl IntoIterator<Item = (&'static str, f64)>,
    ) -> Result<Self, Error> {
        let terms: BTreeMap<_, _> = terms
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect();
        let imbalance: f64 = terms.values().sum();
        if !imbalance.is_finite() {
            return Err(Error::NonFiniteResult {
                quantity: name.to_owned(),
            });
        }
        Ok(Self {
            name: name.to_owned(),
            unit: unit.to_owned(),
            terms,
            imbalance,
        })
    }
}

/// What a solver returns before it is packaged into a run report.
///
/// `Trace` is the convergence record type of a numerical stage; the CAEKit solvers use
/// `caekit_numerics::Convergence`. A solve that stops without convergence returns the traces of the
/// stages that ran and leaves `balances` and `quantities` empty.
pub struct Solution<Trace> {
    /// Identifier of the physical model and discretization, for example
    /// `static_small_strain_axial_bar_p1_fem`; it becomes the report's `method`.
    pub method: &'static str,
    /// Convergence record of each numerical stage, in the order the stages ran.
    pub convergence: Vec<Trace>,
    /// Conservation balances of the converged solution.
    pub balances: Vec<Balance>,
    /// Computed quantities by output name. A solver may store more than the model requested;
    /// `caekit::solve` keeps only the requested ones in the report.
    pub quantities: BTreeMap<Output, Quantity>,
}

impl<Trace> Solution<Trace> {
    /// Starts a solution with a method identifier and stage records, and with no balances or
    /// quantities.
    pub fn new(method: &'static str, convergence: Vec<Trace>) -> Self {
        Self {
            method,
            convergence,
            balances: Vec::new(),
            quantities: BTreeMap::new(),
        }
    }
}
