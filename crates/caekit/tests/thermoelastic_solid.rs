#[path = "common/block.rs"]
mod block;
mod common;

use caekit::physics::elastic_solid::{SolidBoundary, SolidMechanics};
use caekit::physics::heat_conduction::ThermalBoundary;
use caekit::physics::thermoelastic_solid::ThermoelasticSolid;
use caekit::{
    FaceRegion, Model, Output, Physics, Quantity, RunReport, SampleLocation, StopReason, TetraMesh,
    solve,
};
use caekit_types::Pressure;

use block::{TETRA_SAMPLES, interpolate, structured_block};
use common::{close, region, scalar};

const EXPANSION: f64 = 12.0e-6 * 50.0;

fn example() -> Model {
    Model::from_json(include_bytes!("../../../examples/thermoelastic_solid.json")).unwrap()
}

fn coupled_case(model: &mut Model) -> &mut ThermoelasticSolid {
    let Physics::ThermoelasticSolid(case) = &mut model.physics else {
        panic!("expected thermoelastic solid");
    };
    case
}

fn displacement(region: &str, components_m: [Option<f64>; 3]) -> SolidBoundary {
    SolidBoundary::Displacement {
        region: region.to_owned(),
        components_m,
    }
}

fn temperature(region: &str, temperature_k: f64) -> ThermalBoundary {
    ThermalBoundary::Temperature {
        region: region.to_owned(),
        temperature_k,
    }
}

fn nodal_vector<'a>(
    report: &'a RunReport,
    output: Output,
    expected_unit: &str,
) -> (&'a [[f64; 3]], &'a [[f64; 3]]) {
    let Quantity::VectorField {
        unit,
        location: SampleLocation::Node,
        positions_m,
        values,
    } = &report.quantities[&output]
    else {
        panic!("expected nodal vector");
    };
    assert_eq!(unit, expected_unit);
    (positions_m, values)
}

fn stresses(report: &RunReport) -> &[[[f64; 3]; 3]] {
    let Quantity::TensorField {
        unit,
        location: SampleLocation::Cell,
        values,
        ..
    } = &report.quantities[&Output::CauchyStress]
    else {
        panic!("expected centroid stress tensor");
    };
    assert_eq!(unit, "Pa");
    values
}

fn close_tensor(actual: &[[f64; 3]; 3], expected: [[f64; 3]; 3], tolerance: f64) {
    for (&actual, expected) in actual.iter().flatten().zip(expected.into_iter().flatten()) {
        close(actual, expected, tolerance);
    }
}

fn assert_converged(report: &RunReport) {
    assert_eq!(report.stop_reason, StopReason::Converged);
    let [thermal, mechanical] = report.convergence.as_slice() else {
        panic!("expected thermal and mechanical stages");
    };
    for (stage, name, unit) in [
        (thermal, "solid_heat_conduction", "W"),
        (mechanical, "solid_elastic_equilibrium", "N"),
    ] {
        assert_eq!(stage.stage, name);
        assert_eq!(stage.residual_unit, unit);
        assert_eq!(stage.stop_reason, StopReason::Converged);
        assert!(stage.final_true_residual_l2.unwrap() <= stage.residual_threshold.unwrap());
    }
}

#[test]
fn direct_mechanics_api_checks_absolute_temperature_before_any_shortcut() {
    let mut model = example();
    let case = coupled_case(&mut model);
    let checked = case.mesh.checked("mesh").unwrap();
    let solid = case.mechanical.prepare(&checked).unwrap();
    for alpha in [case.expansion.linear_expansion_coefficient_per_k, 0.0] {
        let mut expansion = case.expansion;
        expansion.linear_expansion_coefficient_per_k = alpha;
        for (datum, offset) in [
            (300.0, -301.0),
            (300.0, -300.0),
            (f64::MAX, f64::MAX),
            (300.0, f64::NAN),
        ] {
            let mut offsets = vec![0.0; case.mesh.vertices_m.len()];
            offsets[2] = offset;
            let err = solid
                .solve_thermoelastic(Default::default(), &[], expansion, datum, &offsets)
                .err()
                .expect("invalid absolute temperature must fail");
            assert!(err.to_string().contains("vertex 2"));
        }
    }
}

#[test]
fn small_axial_load_is_resolved_after_removing_uniform_free_expansion() {
    let mut model = example();
    let case = coupled_case(&mut model);
    case.mechanical.boundary_conditions.pop();
    case.mechanical
        .boundary_conditions
        .push(SolidBoundary::Traction {
            region: "x1".into(),
            components_pa: [1.0, 0.0, 0.0],
        });
    let report = solve(&model).unwrap();
    assert_converged(&report);
    for tensor in stresses(&report) {
        close(tensor[0][0], 1.0, 1e-8);
    }

    // The fixture becomes a 1 m long bar with a 0.1 mm square section.
    for point in &mut coupled_case(&mut model).mesh.vertices_m {
        point[1] *= 0.01;
        point[2] *= 0.01;
    }
    let report = solve(&model).unwrap();
    if report.stop_reason == StopReason::Converged {
        for tensor in stresses(&report) {
            close(tensor[0][0], 1.0, 1e-7);
        }
    } else {
        assert!(report.quantities.is_empty());
        assert!(
            report.convergence[1]
                .diagnostic
                .as_ref()
                .unwrap()
                .contains("mean ratio")
        );
    }
    model.linear_solver.relative_residual_tolerance = 1e-7;
    let report = solve(&model).unwrap();
    assert_converged(&report);
    for tensor in stresses(&report) {
        close(tensor[0][0], 1.0, 1e-6);
    }
    let balance = report
        .balances
        .iter()
        .find(|b| b.name == "solid_force_x")
        .unwrap();
    close(balance.terms["reaction"], -1e-8, 1e-14);
}

#[test]
fn coupled_solve_checks_reactions_at_the_scale_of_the_external_load() {
    let mut model = example();
    let case = coupled_case(&mut model);
    case.mechanical.boundary_conditions.pop();
    case.mechanical
        .boundary_conditions
        .push(displacement("y1", [None, Some(0.0), None]));
    case.mechanical
        .boundary_conditions
        .push(SolidBoundary::Traction {
            region: "x1".into(),
            components_pa: [1.0, 0.0, 0.0],
        });
    for point in &mut case.mesh.vertices_m {
        point[1] *= 0.01;
        point[2] *= 0.01;
    }
    let report = solve(&model).unwrap();
    let trace = &report.convergence[1];
    assert_eq!(trace.stop_reason, StopReason::NumericalBreakdown);
    assert!(trace.final_true_residual_l2.unwrap() <= trace.residual_threshold.unwrap());
    let diagnostic = trace.diagnostic.as_ref().unwrap();
    assert!(
        diagnostic.contains("external-load balance tolerance") && diagnostic.contains("mean ratio")
    );
    assert!(report.quantities.is_empty() && report.balances.is_empty());
}

#[test]
fn free_expansion_uses_a_temperature_field_even_without_thermal_outputs() {
    let mut model = example();
    coupled_case(&mut model)
        .mechanical
        .boundary_conditions
        .pop();
    model.outputs.retain(|output| {
        !matches!(
            output,
            Output::SolidTemperature | Output::SolidHeatFlux | Output::BoundaryHeatRate
        )
    });
    let report = solve(&model).unwrap();
    assert_converged(&report);
    let (nodes, values) = nodal_vector(&report, Output::DisplacementVector, "m");
    for (&point, u) in nodes.iter().zip(values) {
        for (&measured, expected) in u.iter().zip(point.map(|x| EXPANSION * x)) {
            close(measured, expected, 5.0e-12);
        }
    }
    for stress in stresses(&report) {
        close_tensor(stress, [[0.0; 3]; 3], 0.05);
    }
    for &value in nodal_vector(&report, Output::NodalReaction, "N")
        .1
        .iter()
        .flatten()
    {
        close(value, 0.0, 5.0e-6);
    }
    close(scalar(&report, Output::ElasticEnergy), 0.0, 1.0e-10);
}

#[test]
fn axial_restraint_gives_uniaxial_compression_and_opposite_reactions() {
    let mut model = example();
    model.linear_solver.absolute_residual_tolerance = 1.0e-8;
    coupled_case(&mut model)
        .thermal_solver
        .absolute_residual_tolerance = 1.0e-12;
    let report = solve(&model).unwrap();
    assert_converged(&report);
    assert_eq!(report.convergence[0].residual_threshold, Some(1.0e-12));
    assert!(report.convergence[1].residual_threshold.unwrap() >= 1.0e-8);
    assert_eq!(report.geometry.len(), 1);
    assert_eq!(report.geometry[0].id, "restrained-thermal-bar");
    assert_eq!(report.meshes.len(), 1);
    let (nodes, values) = nodal_vector(&report, Output::DisplacementVector, "m");
    for (&[_, y, z], u) in nodes.iter().zip(values) {
        for (&measured, expected) in u
            .iter()
            .zip([0.0, 1.3 * EXPANSION * y, 1.3 * EXPANSION * z])
        {
            close(measured, expected, 5.0e-12);
        }
    }
    for stress in stresses(&report) {
        close_tensor(stress, [[-120.0e6, 0.0, 0.0], [0.0; 3], [0.0; 3]], 0.05);
    }
    let (nodes, values) = nodal_vector(&report, Output::NodalReaction, "N");
    for (x, expected) in [(0.0, 12_000.0), (1.0, -12_000.0)] {
        let end_force: f64 = nodes
            .iter()
            .zip(values)
            .filter(|(node, _)| node[0] == x)
            .map(|(_, reaction)| reaction[0])
            .sum();
        close(end_force, expected, 5.0e-6);
    }
    close(scalar(&report, Output::ElasticEnergy), 3.6, 1.0e-8);
    for balance in &report.balances {
        close(
            balance.imbalance,
            0.0,
            if balance.unit == "W" { 1.0e-8 } else { 5.0e-6 },
        );
    }
}

#[test]
fn a_temperature_gradient_drives_the_local_displacement_and_converges_with_refinement() {
    let nu = 0.3;
    let scale = (1.0 + nu) / (1.0 - nu) * EXPANSION / 2.0;
    let exact = |x: f64| scale * x.powi(2);
    let mut previous = None;
    for divisions in [2, 4, 8] {
        let mut model = example();
        let case = coupled_case(&mut model);
        case.mesh = structured_block([1.0, 0.5, 0.5], divisions);
        case.thermal.boundary_conditions = vec![temperature("x0", 300.0), temperature("x1", 350.0)];
        case.mechanical.boundary_conditions = vec![
            displacement("x0", [Some(0.0), None, None]),
            displacement("y0", [None, Some(0.0), None]),
            displacement("y1", [None, Some(0.0), None]),
            displacement("z0", [None, None, Some(0.0)]),
            displacement("z1", [None, None, Some(0.0)]),
        ];
        let report = solve(&model).unwrap();
        assert_converged(&report);
        let (points, displacement) = nodal_vector(&report, Output::DisplacementVector, "m");
        let Quantity::Field {
            values: temperature,
            ..
        } = &report.quantities[&Output::SolidTemperature]
        else {
            panic!("expected nodal temperatures");
        };
        for (&[x, _, _], &t) in points.iter().zip(temperature) {
            close(t, 300.0 + 50.0 * x, 2.0e-8);
        }
        let cells = &coupled_case(&mut model).mesh.cells;
        let mut squared_error = 0.0;
        for &cell in cells {
            for weights in TETRA_SAMPLES {
                let x = interpolate(cell.map(|i| points[i][0]), weights);
                let ux = interpolate(cell.map(|i| displacement[i][0]), weights);
                squared_error += (ux - exact(x)).powi(2);
            }
        }
        let rms = (squared_error / (cells.len() * TETRA_SAMPLES.len()) as f64).sqrt();
        eprintln!(
            "Temperature gradient, {divisions} divisions per axis: {rms:.8e} m interior displacement RMS error"
        );
        if let Some(coarse) = previous {
            assert!(rms < coarse / 2.7);
        }
        previous = Some(rms);
        if divisions == 8 {
            for x in [0.25, 0.5, 0.75] {
                let node = points
                    .iter()
                    .position(|point| *point == [x, 0.25, 0.25])
                    .unwrap();
                close(displacement[node][0], exact(x), 5.0e-6);
            }
        }
    }
    assert!(previous.unwrap() < 5.0e-6);
}

#[test]
fn fully_restrained_tetrahedron_integrates_the_quadratic_thermal_energy() {
    let mut model = example();
    let case = coupled_case(&mut model);
    case.mesh = TetraMesh {
        vertices_m: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        cells: vec![[0, 1, 2, 3]],
        boundary_regions: [
            ("x0", [0, 2, 3]),
            ("y0", [0, 1, 3]),
            ("z0", [0, 1, 2]),
            ("slant", [1, 2, 3]),
        ]
        .into_iter()
        .map(|(region, face)| FaceRegion {
            name: region.to_owned(),
            faces: vec![face],
        })
        .collect(),
    };
    // For T = 300 + 50x and k = 20, the inward flux on the slant is 1000/sqrt(3).
    case.thermal.boundary_conditions = vec![
        temperature("x0", 300.0),
        ThermalBoundary::HeatFlux {
            region: "slant".to_owned(),
            inward_heat_flux_w_m2: 1_000.0 / 3.0_f64.sqrt(),
        },
    ];
    case.mechanical.boundary_conditions = vec![
        displacement("x0", [Some(0.0); 3]),
        displacement("slant", [Some(0.0); 3]),
    ];
    let report = solve(&model).unwrap();
    assert_converged(&report);
    close(
        region(&report, Output::BoundaryHeatRate, "slant"),
        500.0,
        1.0e-8,
    );
    close(
        region(&report, Output::BoundaryHeatRate, "x0"),
        -500.0,
        1.0e-8,
    );
    close(report.balances[0].imbalance, 0.0, 1.0e-8);
    // Integral of x^2 over the unit reference tetrahedron is 1/60.
    let hydrostatic_modulus = 200.0e9 / (1.0 - 2.0 * 0.3);
    let energy = 1.5 * hydrostatic_modulus * EXPANSION.powi(2) / 60.0;
    close(scalar(&report, Output::ElasticEnergy), energy, 1.0e-8);
    let s = -hydrostatic_modulus * EXPANSION / 4.0;
    close_tensor(
        &stresses(&report)[0],
        [[s, 0.0, 0.0], [0.0, s, 0.0], [0.0, 0.0, s]],
        0.05,
    );
}

#[test]
fn pressure_and_body_force_add_to_thermal_expansion() {
    let mut model = example();
    let case = coupled_case(&mut model);
    case.mesh = structured_block([1.0, 0.1, 0.1], 4);
    case.mechanical.material.poisson_ratio = 0.0;
    case.mechanical.body_force_n_m3 = [2.0e6, 0.0, 0.0];
    case.mechanical.boundary_conditions.pop();
    case.mechanical
        .boundary_conditions
        .push(SolidBoundary::Pressure {
            region: "x1".to_owned(),
            applied: Pressure::Absolute {
                value_pa: 1_101_325.0,
            },
            ambient: Pressure::Absolute {
                value_pa: 101_325.0,
            },
        });
    let report = solve(&model).unwrap();
    assert_converged(&report);
    let (nodes, values) = nodal_vector(&report, Output::DisplacementVector, "m");
    // For nu=0, sigma_xx = -p + b(L-x), so the load-induced tip extension is zero.
    for (&[x, _, _], u) in nodes.iter().zip(values) {
        let mechanical = (1.0e6 * x - 1.0e6 * x.powi(2)) / 200.0e9;
        close(u[0], EXPANSION * x + mechanical, 5.0e-8);
    }
    let balance = report
        .balances
        .iter()
        .find(|balance| balance.name == "solid_force_x")
        .unwrap();
    close(balance.terms["applied"], 1.0e4, 1.0e-6);
    close(balance.terms["reaction"], -1.0e4, 1.0e-4);
}

#[test]
fn zero_expansion_or_reference_temperature_recovers_legacy_mechanics() {
    let mut elastic =
        Model::from_json(include_bytes!("../../../examples/elastic_solid.json")).unwrap();
    let Physics::ElasticSolid(case) = &mut elastic.physics else {
        panic!("expected elastic solid");
    };
    case.body_force_n_m3 = [500.0, -1_000.0, 1_500.0];
    let mechanical = SolidMechanics {
        material: case.material.clone(),
        boundary_conditions: case.boundary_conditions.clone(),
        body_force_n_m3: case.body_force_n_m3,
    };
    let mut coupled = example();
    let combined = coupled_case(&mut coupled);
    combined.mesh = case.mesh.clone();
    combined.geometry = case.geometry.clone();
    combined.mechanical = mechanical;
    let elastic_report = solve(&elastic).unwrap();

    for zero_alpha in [false, true] {
        let combined = coupled_case(&mut coupled);
        combined.expansion.linear_expansion_coefficient_per_k =
            if zero_alpha { 0.0 } else { 12.0e-6 };
        combined.thermal.boundary_conditions = vec![temperature("x0", 300.0)];
        if zero_alpha {
            combined
                .thermal
                .boundary_conditions
                .push(temperature("x1", 350.0));
        }
        let report = solve(&coupled).unwrap();
        assert_converged(&report);
        for &output in &elastic.outputs {
            assert_eq!(
                serde_json::to_value(&report.quantities[&output]).unwrap(),
                serde_json::to_value(&elastic_report.quantities[&output]).unwrap()
            );
        }
        assert_eq!(
            serde_json::to_value(&report.balances[1..]).unwrap(),
            serde_json::to_value(&elastic_report.balances).unwrap()
        );
    }
}

#[test]
fn iteration_limits_preserve_only_the_executed_stage_traces() {
    let mut thermal_limit = example();
    let case = coupled_case(&mut thermal_limit);
    case.thermal_solver.max_iterations = 1;
    case.thermal.boundary_conditions = vec![
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
    let mut mechanical_limit = example();
    mechanical_limit.linear_solver.max_iterations = 1;
    for (model, stages, unit) in [(thermal_limit, 1, "W"), (mechanical_limit, 2, "N")] {
        let report = solve(&model).unwrap();
        assert_eq!(report.stop_reason, StopReason::IterationLimit);
        assert!(report.quantities.is_empty());
        assert!(report.balances.is_empty());
        assert_eq!(report.convergence.len(), stages);
        for stage in &report.convergence[..stages - 1] {
            assert_eq!(stage.stop_reason, StopReason::Converged);
        }
        let stopped = report.convergence.last().unwrap();
        assert_eq!(stopped.stop_reason, StopReason::IterationLimit);
        assert_eq!(stopped.residual_unit, unit);
        assert_eq!(stopped.iterations, 1);
        assert!(stopped.final_true_residual_l2.unwrap() > stopped.residual_threshold.unwrap());
    }
}
