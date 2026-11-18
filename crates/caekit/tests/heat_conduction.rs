#[path = "common/block.rs"]
mod block;
mod common;

use caekit::physics::heat_conduction::{HeatConduction, ThermalBoundary};
use caekit::{Model, Output, Physics, Quantity, RunReport, SampleLocation, StopReason, solve};

use block::{TETRA_SAMPLES, interpolate, structured_block};
use common::{close, region};

fn example() -> Model {
    Model::from_json(include_bytes!("../../../examples/heat_conduction.json")).unwrap()
}

fn heat_case(model: &mut Model) -> &mut HeatConduction {
    let Physics::HeatConduction(case) = &mut model.physics else {
        panic!("expected heat conduction");
    };
    case
}

fn temperatures(report: &RunReport) -> (&[[f64; 3]], &[f64]) {
    let Quantity::Field {
        unit,
        location: SampleLocation::Node,
        positions_m,
        values,
    } = &report.quantities[&Output::SolidTemperature]
    else {
        panic!("expected nodal temperature");
    };
    assert_eq!(unit, "K");
    (positions_m, values)
}

fn fluxes(report: &RunReport) -> (&[[f64; 3]], &[[f64; 3]]) {
    let Quantity::VectorField {
        unit,
        location: SampleLocation::Cell,
        positions_m,
        values,
    } = &report.quantities[&Output::SolidHeatFlux]
    else {
        panic!("expected cell heat flux vectors");
    };
    assert_eq!(unit, "W/m^2");
    (positions_m, values)
}

fn temperature(region: &str, temperature_k: f64) -> ThermalBoundary {
    ThermalBoundary::Temperature {
        region: region.to_owned(),
        temperature_k,
    }
}

#[test]
fn linear_temperature_profile_flux_and_reactions() {
    let report = solve(&example()).unwrap();
    assert_eq!(report.stop_reason, StopReason::Converged);
    let [stage] = report.convergence.as_slice() else {
        panic!("expected one thermal stage");
    };
    assert_eq!(stage.residual_unit, "W");
    assert!(stage.final_true_residual_l2.unwrap() <= stage.residual_threshold.unwrap());
    let (nodes, values) = temperatures(&report);
    assert!(nodes.contains(&[0.05; 3]));
    for (&[x, _, _], &value) in nodes.iter().zip(values) {
        close(value, 400.0 - 1_000.0 * x, 1.0e-8);
    }
    let (_, q) = fluxes(&report);
    assert_eq!(q.len(), 12);
    for v in q {
        for (&actual, expected) in v.iter().zip([20_000.0, 0.0, 0.0]) {
            close(actual, expected, 1.0e-6);
        }
    }
    for (name, expected) in [("x0", 200.0), ("x1", -200.0), ("y0", 0.0), ("z1", 0.0)] {
        close(
            region(&report, Output::BoundaryHeatRate, name),
            expected,
            1.0e-8,
        );
    }
    assert_eq!(report.balances[0].name, "solid_energy");
    assert_eq!(report.balances[0].unit, "W");
    close(report.balances[0].imbalance, 0.0, 1.0e-8);
}

#[test]
fn prescribed_flux_and_convection_set_the_temperature_without_dirichlet() {
    let mut model = example();
    heat_case(&mut model).thermal.boundary_conditions = vec![
        ThermalBoundary::HeatFlux {
            region: "x0".to_owned(),
            inward_heat_flux_w_m2: 1_000.0,
        },
        ThermalBoundary::Convection {
            region: "x1".to_owned(),
            ambient_temperature_k: 300.0,
            film_coefficient_w_m2_k: 100.0,
        },
    ];
    let report = solve(&model).unwrap();
    assert_eq!(report.stop_reason, StopReason::Converged);
    let (nodes, values) = temperatures(&report);
    for (&[x, _, _], &value) in nodes.iter().zip(values) {
        close(value, 315.0 - 50.0 * x, 1.0e-8);
    }
    for v in fluxes(&report).1 {
        for (&actual, expected) in v.iter().zip([1_000.0, 0.0, 0.0]) {
            close(actual, expected, 1.0e-6);
        }
    }
    close(
        region(&report, Output::BoundaryHeatRate, "x0"),
        10.0,
        1.0e-8,
    );
    close(
        region(&report, Output::BoundaryHeatRate, "x1"),
        -10.0,
        1.0e-8,
    );
    close(report.balances[0].imbalance, 0.0, 1.0e-8);
}

#[test]
fn weak_convection_resolves_the_mean_temperature_and_small_gradient() {
    let mut model = example();
    let thermal = &mut heat_case(&mut model).thermal;
    thermal.material.thermal_conductivity_w_m_k = 400.0;
    thermal.boundary_conditions = vec![
        ThermalBoundary::HeatFlux {
            region: "x0".into(),
            inward_heat_flux_w_m2: 0.1,
        },
        ThermalBoundary::Convection {
            region: "x1".into(),
            ambient_temperature_k: 300.0,
            film_coefficient_w_m2_k: 0.01,
        },
    ];
    let report = solve(&model).unwrap();
    assert_eq!(report.stop_reason, StopReason::Converged);
    let (nodes, values) = temperatures(&report);
    for (&[x, _, _], &value) in nodes.iter().zip(values) {
        close(value, 310.0 + (0.1 - x) * 0.1 / 400.0, 1e-9);
    }
    for flux in fluxes(&report).1 {
        close(flux[0], 0.1, 1e-9);
    }
    close(
        region(&report, Output::BoundaryHeatRate, "x1"),
        -0.001,
        1e-12,
    );
}

#[test]
fn shared_prescribed_nodes_contribute_once_to_the_energy_balance() {
    let mut model = example();
    let case = heat_case(&mut model);
    let left = case.mesh.boundary_regions[0].faces.pop().unwrap();
    case.mesh.boundary_regions.push(caekit::FaceRegion {
        name: "x0_split".to_owned(),
        faces: vec![left],
    });
    case.thermal
        .boundary_conditions
        .push(temperature("x0_split", 400.0));
    let report = solve(&model).unwrap();
    for (name, expected) in [("x0", 100.0), ("x0_split", 100.0), ("x1", -200.0)] {
        close(
            region(&report, Output::BoundaryHeatRate, name),
            expected,
            1.0e-8,
        );
    }
    close(report.balances[0].imbalance, 0.0, 1.0e-8);
}

#[test]
fn uniform_source_converges_inside_elements_and_leaves_the_solid() {
    const LENGTH: f64 = 0.1;
    const SOURCE: f64 = 200_000.0;
    const K: f64 = 20.0;
    let mut previous = None;
    for divisions in [2, 4, 8] {
        let mut model = example();
        let case = heat_case(&mut model);
        case.mesh = structured_block([LENGTH; 3], divisions);
        case.thermal.volumetric_heat_source_w_m3 = SOURCE;
        case.thermal.boundary_conditions = vec![temperature("x0", 300.0), temperature("x1", 300.0)];
        let report = solve(&model).unwrap();
        assert_eq!(report.stop_reason, StopReason::Converged);
        let (nodes, values) = temperatures(&report);
        let (_, q) = fluxes(&report);
        let cells = &heat_case(&mut model).mesh.cells;
        let mut temperature_squared_error = 0.0;
        let mut flux_squared_error = 0.0;
        // This mesh has equal-volume tetrahedra. Sample inside every cell so the temperature
        // norm includes the interpolation error of the quadratic profile between vertices.
        for (&cell, &[qx, qy, qz]) in cells.iter().zip(q) {
            for weights in TETRA_SAMPLES {
                let x = interpolate(cell.map(|i| nodes[i][0]), weights);
                let temperature = interpolate(cell.map(|i| values[i]), weights);
                let analytical_t = 300.0 + SOURCE * x * (LENGTH - x) / (2.0 * K);
                let analytical_q = SOURCE * (x - LENGTH / 2.0);
                temperature_squared_error += (temperature - analytical_t).powi(2);
                flux_squared_error += (qx - analytical_q).powi(2) + qy.powi(2) + qz.powi(2);
            }
        }
        let samples = (TETRA_SAMPLES.len() * cells.len()) as f64;
        let temperature_error = (temperature_squared_error / samples).sqrt();
        let flux_error = (flux_squared_error / samples).sqrt();
        eprintln!(
            "Source case, {divisions} divisions per axis: {temperature_error:.6} K, {flux_error:.6} W/m^2 interior RMS error"
        );
        if let Some((coarse_t, coarse_q)) = previous {
            assert!(temperature_error < coarse_t / 2.7);
            assert!(flux_error < coarse_q / 1.65);
        }
        previous = Some((temperature_error, flux_error));
        let generation = SOURCE * LENGTH.powi(3);
        let outgoing = -["x0", "x1"]
            .map(|name| region(&report, Output::BoundaryHeatRate, name))
            .iter()
            .sum::<f64>();
        close(
            report.balances[0].terms["volumetric_generation"],
            generation,
            2.0e-6,
        );
        close(outgoing, generation, 2.0e-6);
        close(report.balances[0].imbalance, 0.0, 2.0e-6);
    }
    let (temperature_error, flux_error) = previous.unwrap();
    assert!(temperature_error < 0.3);
    assert!(flux_error < 800.0);
}

#[test]
fn validation_rejects_ambiguous_or_unphysical_thermal_inputs() {
    for (conditions, message) in [
        (vec![], "determine absolute temperature"),
        (
            vec![
                ThermalBoundary::Insulation {
                    region: "x0".to_owned(),
                },
                ThermalBoundary::HeatFlux {
                    region: "x1".to_owned(),
                    inward_heat_flux_w_m2: 5.0,
                },
            ],
            "determine absolute temperature",
        ),
        (
            vec![temperature("missing", 300.0)],
            "unknown exterior region",
        ),
        (
            vec![temperature("x0", 300.0), temperature("x0", 400.0)],
            "already has a thermal boundary condition",
        ),
        (
            vec![temperature("x0", 300.0), temperature("y0", 400.0)],
            "conflicting prescribed temperatures at vertex",
        ),
        (vec![temperature("x0", 0.0)], "temperature_k"),
        (
            vec![ThermalBoundary::Convection {
                region: "x1".to_owned(),
                ambient_temperature_k: 300.0,
                film_coefficient_w_m2_k: 0.0,
            }],
            "film_coefficient_w_m2_k",
        ),
    ] {
        let mut model = example();
        heat_case(&mut model).thermal.boundary_conditions = conditions;
        let data = serde_json::to_vec(&model).unwrap();
        for error in [
            Model::from_json(&data).unwrap_err(),
            model.validate().unwrap_err(),
            solve(&model).unwrap_err(),
        ] {
            assert!(error.to_string().contains(message), "{error}");
        }
    }
    let mut model = example();
    heat_case(&mut model)
        .thermal
        .material
        .thermal_conductivity_w_m_k = -1.0;
    assert!(
        model
            .validate()
            .unwrap_err()
            .to_string()
            .contains("thermal_conductivity_w_m_k")
    );
}
