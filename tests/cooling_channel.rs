mod common;

use std::process::Command;

use caekit::physics::cooling_channel::CoolingChannel;
use caekit::{Model, Output, Physics, Quantity, RunReport, StopReason, solve};
use common::{close, scalar};

const INPUT: &[u8] = include_bytes!("../examples/cooling_channel.json");

fn channel(model: &mut Model) -> &mut CoolingChannel {
    let Physics::CoolingChannel(case) = &mut model.physics else {
        panic!("expected the cooling channel fixture");
    };
    case
}

fn continuum_outlet() -> f64 {
    let per_area_resistance = 0.004 / 15.0 + 1.0 / 400.0;
    let ntu: f64 = (2.0 * 0.1 / per_area_resistance) / (0.01 * 4180.0);
    360.0 - (360.0 - 298.0) * (-ntu).exp()
}

#[test]
fn cli_solves_nonmatching_meshes_and_closes_energy_balances() {
    let run = Command::new(env!("CARGO_BIN_EXE_caekit"))
        .args(["run", "examples/cooling_channel.json"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let report: RunReport = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(report.stop_reason, StopReason::Converged);
    assert_eq!(report.geometry.len(), 2);
    assert_eq!(
        report
            .meshes
            .iter()
            .map(|m| m.cell_count)
            .collect::<Vec<_>>(),
        [32, 31, 3, 48]
    );
    assert_eq!(report.convergence.len(), 2);
    assert!(report.convergence[1].iterations > 1);
    let outlet = scalar(&report, Output::OutletTemperature);
    close(outlet, continuum_outlet(), 0.5);
    close(
        scalar(&report, Output::HeatTransferRate),
        0.01 * 4180.0 * (outlet - 298.0),
        1.0e-6,
    );

    for name in [
        "wall_energy",
        "interface_energy",
        "coolant_energy",
        "system_energy",
    ] {
        let balance = report.balances.iter().find(|b| b.name == name).unwrap();
        close(balance.imbalance, 0.0, 1.0e-6);
    }

    let Quantity::Field {
        positions_m,
        values,
        ..
    } = &report.quantities[&Output::WallTemperature]
    else {
        panic!("expected world-space wall samples");
    };
    assert_eq!(positions_m.len(), 31 * 4);
    assert_eq!(values.len(), positions_m.len());
    for (samples, temperatures) in positions_m.chunks_exact(4).zip(values.chunks_exact(4)) {
        assert_eq!(samples[0][1], -0.004);
        assert_eq!(samples[3][1], 0.0);
        close(temperatures[0], 360.0, 1.0e-12);
        assert!(temperatures.windows(2).all(|p| p[0] > p[1]));
    }

    let mut model = Model::from_json(INPUT).unwrap();
    let geometry = channel(&mut model);
    let wall_edges = &geometry.wall.axial_mesh.vertices_m;
    let Quantity::Field { values: fluxes, .. } = &report.quantities[&Output::InterfaceHeatFlux]
    else {
        panic!("expected interface flux field");
    };
    let integrated: f64 = wall_edges
        .windows(2)
        .zip(fluxes)
        .map(|(edge, flux)| (edge[1] - edge[0]) * 0.1 * flux)
        .sum();
    close(
        integrated,
        scalar(&report, Output::HeatTransferRate),
        1.0e-9,
    );
}

#[test]
fn thermal_mesh_refinement_recovers_first_order_enthalpy_advection() {
    let mut model = Model::from_json(INPUT).unwrap();
    let exact = continuum_outlet();
    let mut errors = Vec::new();
    for n in [12, 24, 48, 96] {
        let case = channel(&mut model);
        let vertices: Vec<_> = (0..=n).map(|i| 2.0 * i as f64 / n as f64).collect();
        case.wall.axial_mesh.vertices_m.clone_from(&vertices);
        case.coolant.axial_mesh.vertices_m = vertices;

        let report = solve(&model).unwrap();
        assert_eq!(report.stop_reason, StopReason::Converged);
        errors.push(exact - scalar(&report, Output::OutletTemperature));
    }
    for pair in errors.windows(2) {
        assert!((1.9..2.1).contains(&(pair[0] / pair[1])), "{errors:?}");
    }
}

#[test]
fn transfers_heat_in_either_direction_and_has_zero_isothermal_flux() {
    let mut model = Model::from_json(INPUT).unwrap();
    channel(&mut model).wall.hot_boundary.temperature_k = 280.0;
    let report = solve(&model).unwrap();
    assert!(scalar(&report, Output::HeatTransferRate) < 0.0);
    assert!((280.0..298.0).contains(&scalar(&report, Output::OutletTemperature)));

    channel(&mut model).wall.hot_boundary.temperature_k = 298.0;
    let report = solve(&model).unwrap();
    close(scalar(&report, Output::OutletTemperature), 298.0, 0.0);
    close(scalar(&report, Output::HeatTransferRate), 0.0, 0.0);
    assert_eq!(report.convergence[1].iterations, 0);
}

#[test]
fn stops_with_trace_if_coupling_fails_to_converge() {
    let mut model = Model::from_json(INPUT).unwrap();
    channel(&mut model).coupling_solver.max_iterations = 1;
    let report = solve(&model).unwrap();
    assert_eq!(report.stop_reason, StopReason::IterationLimit);
    assert_eq!(report.convergence[0].stop_reason, StopReason::Converged);
    assert_eq!(
        report.convergence[1].stop_reason,
        StopReason::IterationLimit
    );
    assert!(report.quantities.is_empty());
    assert!(
        report.convergence[1].final_true_residual_l2.unwrap()
            > report.convergence[1].residual_threshold.unwrap()
    );
}

#[test]
fn rejects_a_stale_surface_assignment_and_wrong_axial_ports() {
    let mut model = Model::from_json(INPUT).unwrap();
    channel(&mut model).interface.fluid_region = "deleted_wall".to_owned();
    assert!(
        model
            .validate()
            .unwrap_err()
            .to_string()
            .contains("physics.interface.fluid_region")
    );

    let mut model = Model::from_json(INPUT).unwrap();
    channel(&mut model).coolant.axial_mesh.start_region = "outlet".to_owned();
    assert!(model.validate().is_err());
}
