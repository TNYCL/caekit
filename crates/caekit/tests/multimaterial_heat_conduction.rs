//! Multimaterial heat conduction: the series-resistance reference, interface heat rates and
//! regional energy balances, attribution of exterior power, refinement of a curved profile,
//! and rejected volume assignments.

#[path = "common/block.rs"]
mod block;
mod common;

use caekit::physics::heat_conduction::{
    HeatConduction, MultimaterialHeatConduction, SolidThermal, ThermalBoundary, ThermalVolumeRegion,
};
use caekit::{
    Balance, InterfaceValue, Model, Output, Physics, Quantity, RunReport, SampleLocation,
    StopReason, solve,
};

use block::{TETRA_SAMPLES, interpolate, structured_block};
use common::{close, region};

fn example() -> Model {
    Model::from_json(include_bytes!(
        "../../../examples/multimaterial_heat_conduction.json"
    ))
    .unwrap()
}

fn case(model: &mut Model) -> &mut MultimaterialHeatConduction {
    let Physics::MultimaterialHeatConduction(case) = &mut model.physics else {
        panic!("expected multimaterial heat conduction");
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

fn fluxes(report: &RunReport) -> &[[f64; 3]] {
    let Quantity::VectorField {
        unit,
        location: SampleLocation::Cell,
        values,
        ..
    } = &report.quantities[&Output::SolidHeatFlux]
    else {
        panic!("expected cell heat flux vectors");
    };
    assert_eq!(unit, "W/m^2");
    values
}

fn interfaces(report: &RunReport) -> &[InterfaceValue] {
    let Quantity::ByInterface { unit, values } = &report.quantities[&Output::InterfaceHeatRate]
    else {
        panic!("expected directed interface heat rates");
    };
    assert_eq!(unit, "W");
    values
}

fn energy<'a>(report: &'a RunReport, name: &str) -> &'a Balance {
    let balance = report.balances.iter().find(|balance| balance.name == name);
    let balance = balance.unwrap_or_else(|| panic!("missing balance {name}"));
    assert_eq!(balance.unit, "W");
    balance
}

fn check_region_energy(
    report: &RunReport,
    name: &str,
    generated: f64,
    exterior: f64,
    interface: f64,
    tolerance: f64,
) {
    let balance = energy(report, &format!("solid_energy:{name}"));
    close(balance.terms["volumetric_generation"], generated, tolerance);
    close(balance.terms["exterior_heat_input"], exterior, tolerance);
    close(balance.terms["interface_heat_input"], interface, tolerance);
    close(balance.imbalance, 0.0, tolerance);
}

fn check_energy_totals(report: &RunReport, tolerance: f64) {
    let global = energy(report, "solid_energy");
    let volumes = report
        .balances
        .iter()
        .filter(|balance| balance.name.starts_with("solid_energy:"));
    let sum = |term: &str| volumes.clone().map(|balance| balance.terms[term]).sum();
    close(
        sum("volumetric_generation"),
        global.terms["volumetric_generation"],
        tolerance,
    );
    close(
        sum("exterior_heat_input"),
        global.terms["prescribed_temperature_reaction"]
            + global.terms["prescribed_heat_flux"]
            + global.terms["convection"],
        tolerance,
    );
    close(sum("interface_heat_input"), 0.0, tolerance);
    close(
        volumes.map(|balance| balance.imbalance).sum(),
        global.imbalance,
        tolerance,
    );
    close(global.imbalance, 0.0, tolerance);
}

#[test]
fn layered_conduction_matches_the_series_resistance_solution() {
    let mut model = example();
    let report = solve(&model).unwrap();
    assert_eq!(report.stop_reason, StopReason::Converged);
    let stage = &report.convergence[0];
    assert_eq!(stage.residual_unit, "W");
    assert!(stage.final_true_residual_l2.unwrap() <= stage.residual_threshold.unwrap());
    let mesh = &case(&mut model).mesh;
    let (nodes, temperature) = temperatures(&report);
    assert_eq!(nodes, mesh.vertices_m);
    assert_eq!(temperature.len(), nodes.len());
    let mut interface_nodes = 0;
    for (&[x, _, _], &value) in nodes.iter().zip(temperature) {
        let expected = if x <= 0.01 {
            311.0 - 100.0 * x
        } else {
            310.0 - 1_000.0 * (x - 0.01)
        };
        close(value, expected, 1.0e-8);
        if x == 0.01 {
            interface_nodes += 1;
            close(value, 310.0, 1.0e-8);
        }
    }
    assert_eq!(interface_nodes, 4);
    let flux = fluxes(&report);
    assert_eq!(flux.len(), mesh.cells.len());
    for vector in flux {
        for (&value, expected) in vector.iter().zip([1_000.0, 0.0, 0.0]) {
            close(value, expected, 1.0e-6);
        }
    }
    let [contact] = interfaces(&report) else {
        panic!("expected one interface between the two layers");
    };
    assert_eq!(contact.from_region, "left");
    assert_eq!(contact.to_region, "right");
    close(contact.value, 10.0, 1.0e-8);
    let Quantity::ByRegion { unit, values } = &report.quantities[&Output::BoundaryHeatRate] else {
        panic!("expected exterior heat rates");
    };
    assert_eq!(unit, "W");
    for (name, expected) in [
        ("x0", 10.0),
        ("x1", -10.0),
        ("y0", 0.0),
        ("y1", 0.0),
        ("z0", 0.0),
        ("z1", 0.0),
    ] {
        close(values[name], expected, 1.0e-8);
    }
    assert_eq!(report.balances.len(), 3);
    check_region_energy(&report, "left", 0.0, 10.0, -10.0, 1.0e-8);
    check_region_energy(&report, "right", 0.0, -10.0, 10.0, 1.0e-8);
    check_energy_totals(&report, 1.0e-8);
    let projection = &report.convergence[1];
    assert_eq!(projection.stage, "solid_interface_equilibration");
    assert_eq!(projection.residual_unit, "W");
    assert!(projection.final_true_residual_l2.unwrap() <= projection.residual_threshold.unwrap());
}

#[test]
fn two_regions_can_share_a_material_and_recover_the_uniform_solution() {
    let mut model = example();
    model
        .outputs
        .retain(|output| *output != Output::InterfaceHeatRate);
    let multi = case(&mut model);
    multi.thermal.materials.pop();
    multi.thermal.volume_regions[1].material_id = multi.thermal.materials[0].id.clone();
    for region in &mut multi.thermal.volume_regions {
        region.volumetric_heat_source_w_m3 = 200_000.0;
    }
    let uniform = HeatConduction {
        geometry: multi.geometry.clone(),
        mesh: multi.mesh.clone(),
        thermal: SolidThermal {
            boundary_conditions: multi.thermal.boundary_conditions.clone(),
            material: multi.thermal.materials[0].clone(),
            volumetric_heat_source_w_m3: 200_000.0,
        },
    };
    let report = solve(&model).unwrap();
    model.physics = Physics::HeatConduction(Box::new(uniform));
    let reference = solve(&model).unwrap();
    assert_eq!(report.stop_reason, StopReason::Converged);
    assert_eq!(reference.stop_reason, StopReason::Converged);
    for output in [Output::SolidTemperature, Output::SolidHeatFlux] {
        assert_eq!(
            serde_json::to_value(&report.quantities[&output]).unwrap(),
            serde_json::to_value(&reference.quantities[&output]).unwrap()
        );
    }
    for name in ["x0", "x1", "y0", "y1", "z0", "z1"] {
        close(
            region(&report, Output::BoundaryHeatRate, name),
            region(&reference, Output::BoundaryHeatRate, name),
            1.0e-8,
        );
    }
    assert_eq!(
        serde_json::to_value(&report.balances[0]).unwrap(),
        serde_json::to_value(&reference.balances[0]).unwrap()
    );
    check_energy_totals(&report, 1.0e-8);
}

#[test]
fn a_cycle_of_region_pairs_keeps_the_flux_from_the_element_gradients() {
    let mut model = example();
    model.outputs = vec![Output::InterfaceHeatRate];
    let multi = case(&mut model);
    multi.mesh = structured_block([0.02, 0.1, 0.1], 4);
    multi.thermal.boundary_conditions = vec![temperature("x0", 306.5), temperature("x1", 300.0)];
    // Four slabs ordered a, b, c, a along x. The same 10 W crosses each of the three
    // interfaces. The two parts of a touch opposite ends, so its exterior net is zero.
    let mut regions: Vec<_> = [("c", "high-k"), ("a", "high-k"), ("b", "low-k")]
        .into_iter()
        .map(|(name, material_id)| ThermalVolumeRegion {
            name: name.into(),
            material_id: material_id.into(),
            cells: Vec::new(),
            volumetric_heat_source_w_m3: 0.0,
        })
        .collect();
    for (index, cell) in multi.mesh.cells.iter().enumerate() {
        let x = interpolate(cell.map(|node| multi.mesh.vertices_m[node][0]), [0.25; 4]);
        let slab = (x / 0.005) as usize;
        let region = [1, 2, 0, 1][slab];
        regions[region].cells.push(index);
    }
    multi.thermal.volume_regions = regions;
    let report = solve(&model).unwrap();
    assert_eq!(report.stop_reason, StopReason::Converged);
    let expected = [("a", "b", 10.0), ("a", "c", -10.0), ("b", "c", 10.0)];
    let rates = interfaces(&report);
    assert_eq!(rates.len(), expected.len());
    for (rate, (from, to, power)) in rates.iter().zip(expected) {
        assert_eq!(rate.from_region, from);
        assert_eq!(rate.to_region, to);
        close(rate.value, power, 1.0e-7);
    }
    for name in ["a", "b", "c"] {
        check_region_energy(&report, name, 0.0, 0.0, 0.0, 1.0e-7);
    }
    check_energy_totals(&report, 1.0e-7);
}

#[test]
fn flux_and_weak_convection_are_attributed_to_the_owning_volume() {
    let mut model = example();
    case(&mut model).thermal.boundary_conditions = vec![
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
        let right_drop = 0.1 * (0.02 - x.max(0.01));
        let left_drop = 0.1 * (0.01 - x).max(0.0) / 10.0;
        close(value, 310.0 + right_drop + left_drop, 1.0e-8);
    }
    close(interfaces(&report)[0].value, 0.001, 1.0e-12);
    check_region_energy(&report, "left", 0.0, 0.001, -0.001, 1.0e-12);
    check_region_energy(&report, "right", 0.0, -0.001, 0.001, 1.0e-12);
    check_energy_totals(&report, 1.0e-12);
}

#[test]
fn shared_fixed_vertices_do_not_create_transfer_between_parallel_heat_paths() {
    let mut model = example();
    let multi = case(&mut model);
    multi.mesh = structured_block([0.02, 0.1, 0.1], 2);
    for volume in &mut multi.thermal.volume_regions {
        volume.cells.clear();
    }
    for (i, &cell) in multi.mesh.cells.iter().enumerate() {
        let y = interpolate(cell.map(|node| multi.mesh.vertices_m[node][1]), [0.25; 4]);
        multi.thermal.volume_regions[usize::from(y > 0.05)]
            .cells
            .push(i);
    }
    let cold = multi.mesh.boundary_regions.remove(1);
    for (name, upper) in [("out_high_k", false), ("out_low_k", true)] {
        multi.mesh.boundary_regions.push(caekit::FaceRegion {
            name: name.to_owned(),
            faces: cold
                .faces
                .iter()
                .copied()
                .filter(|face| {
                    let y = face
                        .iter()
                        .map(|&node| multi.mesh.vertices_m[node][1] / 3.0)
                        .sum::<f64>();
                    (y > 0.05) == upper
                })
                .collect(),
        });
    }
    multi.thermal.boundary_conditions = vec![
        temperature("x0", 310.0),
        ThermalBoundary::HeatFlux {
            region: "out_high_k".into(),
            inward_heat_flux_w_m2: -5_000.0,
        },
        ThermalBoundary::HeatFlux {
            region: "out_low_k".into(),
            inward_heat_flux_w_m2: -500.0,
        },
    ];
    let report = solve(&model).unwrap();
    assert_eq!(report.stop_reason, StopReason::Converged);
    let (nodes, values) = temperatures(&report);
    for (&[x, _, _], &value) in nodes.iter().zip(values) {
        close(value, 310.0 - 500.0 * x, 1.0e-8);
    }
    close(
        region(&report, Output::BoundaryHeatRate, "x0"),
        27.5,
        1.0e-8,
    );
    close(interfaces(&report)[0].value, 0.0, 1.0e-8);
    for name in ["left", "right"] {
        check_region_energy(&report, name, 0.0, 0.0, 0.0, 1.0e-8);
    }
    check_energy_totals(&report, 1.0e-8);
}

fn temperature(region: &str, value: f64) -> ThermalBoundary {
    ThermalBoundary::Temperature {
        region: region.to_owned(),
        temperature_k: value,
    }
}

struct RefinementErrors {
    temperature_k: f64,
    flux_w_m2: [f64; 2],
}

fn sourced_layers(divisions: usize) -> RefinementErrors {
    const HALF_LENGTH: f64 = 0.01;
    const SIDE: f64 = 0.02;
    const AREA: f64 = SIDE * SIDE;
    const HOT: f64 = 311.0;
    const COLD: f64 = 300.0;
    const K: [f64; 2] = [10.0, 1.0];
    const Q: [f64; 2] = [200_000.0, 50_000.0];
    let mut model = example();
    let multi = case(&mut model);
    multi.mesh = structured_block([SIDE; 3], divisions);
    for (i, volume) in multi.thermal.volume_regions.iter_mut().enumerate() {
        volume.cells.clear();
        volume.volumetric_heat_source_w_m3 = Q[i];
    }
    let side = |cell: [usize; 4]| {
        usize::from(
            interpolate(cell.map(|node| multi.mesh.vertices_m[node][0]), [0.25; 4]) > HALF_LENGTH,
        )
    };
    for (i, &cell) in multi.mesh.cells.iter().enumerate() {
        multi.thermal.volume_regions[side(cell)].cells.push(i);
    }
    let started = std::time::Instant::now();
    let report = solve(&model).unwrap();
    let seconds = started.elapsed().as_secs_f64();
    assert_eq!(report.stop_reason, StopReason::Converged);

    // Solve q' = Q and T' = -q/k separately in each layer, with continuous
    // temperature and flux at the interface and the two prescribed end temperatures.
    let source_drop = K.map(|k| HALF_LENGTH.powi(2) / (2.0 * k));
    let q_interface = (HOT - COLD + Q[0] * source_drop[0] - Q[1] * source_drop[1])
        / (HALF_LENGTH / K[0] + HALF_LENGTH / K[1]);
    let t_interface = HOT - q_interface * HALF_LENGTH / K[0] + Q[0] * source_drop[0];
    let expected_transfer = q_interface * AREA;
    let left_exterior = (q_interface - Q[0] * HALF_LENGTH) * AREA;
    let right_exterior = -(q_interface + Q[1] * HALF_LENGTH) * AREA;
    close(interfaces(&report)[0].value, expected_transfer, 2.0e-7);
    close(
        region(&report, Output::BoundaryHeatRate, "x0"),
        left_exterior,
        2.0e-7,
    );
    close(
        region(&report, Output::BoundaryHeatRate, "x1"),
        right_exterior,
        2.0e-7,
    );
    check_region_energy(
        &report,
        "left",
        Q[0] * HALF_LENGTH * AREA,
        left_exterior,
        -expected_transfer,
        2.0e-7,
    );
    check_region_energy(
        &report,
        "right",
        Q[1] * HALF_LENGTH * AREA,
        right_exterior,
        expected_transfer,
        2.0e-7,
    );
    check_energy_totals(&report, 2.0e-7);

    let (nodes, nodal_t) = temperatures(&report);
    let cells = &case(&mut model).mesh.cells;
    let mut temperature_error = 0.0;
    let mut flux_error = [0.0; 2];
    let mut samples = [0; 2];
    for (&cell, &[qx, qy, qz]) in cells.iter().zip(fluxes(&report)) {
        let xs = cell.map(|node| nodes[node][0]);
        let ts = cell.map(|node| nodal_t[node]);
        let layer = usize::from(interpolate(xs, [0.25; 4]) > HALF_LENGTH);
        for weights in TETRA_SAMPLES {
            let x = interpolate(xs, weights) - HALF_LENGTH;
            let t = interpolate(ts, weights);
            let expected_t = t_interface - (q_interface * x + 0.5 * Q[layer] * x * x) / K[layer];
            let expected_q = q_interface + Q[layer] * x;
            temperature_error += (t - expected_t).powi(2);
            flux_error[layer] += (qx - expected_q).powi(2) + qy.powi(2) + qz.powi(2);
            samples[layer] += 1;
        }
    }
    let errors = RefinementErrors {
        temperature_k: (temperature_error / samples.iter().sum::<usize>() as f64).sqrt(),
        flux_w_m2: std::array::from_fn(|i| (flux_error[i] / samples[i] as f64).sqrt()),
    };
    eprintln!(
        "{divisions} divisions, {} cells: {:.8} K, {:.6} and {:.6} W/m^2 RMS, \
         interface {:.9} W, {} + {} iterations, {seconds:.6} s",
        cells.len(),
        errors.temperature_k,
        errors.flux_w_m2[0],
        errors.flux_w_m2[1],
        interfaces(&report)[0].value,
        report.convergence[0].iterations,
        report.convergence[1].iterations,
    );
    errors
}

#[test]
fn a_curved_profile_converges_in_temperature_and_in_each_layer_flux() {
    let errors = [4, 8, 16].map(sourced_layers);
    for coarse_fine in errors.windows(2) {
        let [coarse, fine] = coarse_fine else {
            unreachable!();
        };
        assert!(coarse.temperature_k / fine.temperature_k > 2.7);
        for i in 0..2 {
            assert!(coarse.flux_w_m2[i] / fine.flux_w_m2[i] > 1.65);
        }
    }
    assert!(errors[2].temperature_k < 0.02);
    assert!(errors[2].flux_w_m2[0] < 100.0);
    assert!(errors[2].flux_w_m2[1] < 35.0);
}

#[test]
#[ignore = "run in a separate native process to measure peak RSS"]
fn measure_sourced_layers() {
    let divisions = std::env::var("CAEKIT_REFINEMENT_DIVISIONS")
        .expect("set CAEKIT_REFINEMENT_DIVISIONS to 4, 8, or 16")
        .parse()
        .unwrap();
    assert!([4, 8, 16].contains(&divisions));
    sourced_layers(divisions);
}

#[test]
fn invalid_volume_assignments_fail_validation_and_solving() {
    type Change = fn(&mut MultimaterialHeatConduction);
    let changes: [(Change, &str); 8] = [
        (
            |case| {
                case.thermal.volume_regions[0].cells.pop();
            },
            "cell 5 has no volume region assignment",
        ),
        (
            |case| case.thermal.volume_regions[1].cells.push(0),
            "volume region \"right\" assigns cell 0, already assigned to \"left\"",
        ),
        (
            |case| case.thermal.volume_regions[0].cells.push(60),
            "volume region \"left\" references invalid cell 60",
        ),
        (
            |case| case.thermal.volume_regions[1].name = "left".into(),
            "duplicate volume region name \"left\"",
        ),
        (
            |case| case.thermal.volume_regions[1].name = " ".into(),
            "volume_regions[1].name",
        ),
        (
            |case| case.thermal.volume_regions[1].material_id = "missing".into(),
            "volume region \"right\" references unknown material \"missing\"",
        ),
        (
            |case| case.thermal.materials[1].id = case.thermal.materials[0].id.clone(),
            "duplicate material ID \"high-k\"",
        ),
        (
            |case| case.thermal.materials[0].thermal_conductivity_w_m_k = 0.0,
            "materials[0].thermal_conductivity_w_m_k",
        ),
    ];
    for (change, message) in changes {
        let mut model = example();
        change(case(&mut model));
        let data = serde_json::to_vec(&model).unwrap();
        for error in [
            model.validate().unwrap_err(),
            Model::from_json(&data).unwrap_err(),
            solve(&model).unwrap_err(),
        ] {
            assert!(error.to_string().contains(message), "{error}");
        }
    }
    for conductivity in [f64::INFINITY, f64::NAN] {
        let mut model = example();
        case(&mut model).thermal.materials[0].thermal_conductivity_w_m_k = conductivity;
        let error = model.validate().unwrap_err().to_string();
        assert!(
            error.contains("materials[0].thermal_conductivity_w_m_k"),
            "{error}"
        );
    }
}
