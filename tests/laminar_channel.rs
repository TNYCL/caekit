mod common;

use caekit::physics::laminar_channel::{FlowDrive, LaminarChannel, Pressure, PressureAtPort};
use caekit::{Model, Output, Physics, Quantity, StopReason, solve};
use common::{close, region, scalar};

const INPUT: &[u8] = include_bytes!("../examples/laminar_channel.json");

fn channel(model: &mut Model) -> &mut LaminarChannel {
    let Physics::LaminarChannel(case) = &mut model.physics else {
        panic!("expected the channel fixture");
    };
    case
}

fn uniform_gap(case: &mut LaminarChannel, n: usize) {
    case.mesh.vertices_m = (0..=n)
        .map(|i| case.geometry.gap_m * (i as f64 / n as f64))
        .collect();
}

fn pressure_drive(case: &mut LaminarChannel, outlet: Pressure, drop: f64) {
    case.drive = FlowDrive::Pressure {
        inlet: PressureAtPort {
            region: "inlet".to_owned(),
            pressure: Pressure::Gauge {
                value_pa: -5.0 + drop,
                reference_absolute_pa: 100_000.0,
            },
        },
        outlet: PressureAtPort {
            region: "outlet".to_owned(),
            pressure: outlet,
        },
    };
}

#[test]
fn solves_analytic_channel_pressure_loss_and_balances() {
    let report = solve(&Model::from_json(INPUT).unwrap()).unwrap();
    assert_eq!(report.stop_reason, StopReason::Converged);
    // Plane Poiseuille relation, independent of the finite-volume system.
    let analytical = 12.0 * 1.0e-3 * 2.0 * 1.0e-5 / (0.1 * 0.01_f64.powi(3));
    close(scalar(&report, Output::PressureLoss), analytical, 0.005);
    close(scalar(&report, Output::VolumeFlowRate), 1.0e-5, 1.0e-18);
    close(scalar(&report, Output::MassFlowRate), 0.01, 1.0e-15);
    close(scalar(&report, Output::ChannelReynolds), 200.0, 1.0e-10);
    close(
        region(&report, Output::AbsolutePressure, "outlet"),
        101_325.0,
        0.0,
    );
    close(report.balances[0].imbalance, 0.0, 1.0e-15);
    close(report.balances[1].imbalance, 0.0, 1.0e-12);
}

#[test]
fn grid_refinement_recovers_second_order_pressure_loss() {
    let mut model = Model::from_json(INPUT).unwrap();
    let errors: Vec<_> = [8, 16, 32, 64]
        .iter()
        .map(|&n| {
            uniform_gap(channel(&mut model), n);
            let report = solve(&model).unwrap();
            (2.4 - scalar(&report, Output::PressureLoss)).abs()
        })
        .collect();
    for pair in errors.windows(2) {
        assert!((3.8..4.1).contains(&(pair[0] / pair[1])), "{errors:?}");
    }
}

#[test]
fn pressure_drive_checks_parabolic_profile_and_gauge_reference() {
    let mut model = Model::from_json(INPUT).unwrap();
    let case = channel(&mut model);
    let outlet = Pressure::Gauge {
        value_pa: -5.0,
        reference_absolute_pa: 100_000.0,
    };
    pressure_drive(case, outlet, 2.4);
    uniform_gap(case, 64);
    let report = solve(&model).unwrap();
    close(scalar(&report, Output::PressureLoss), 2.4, 1.0e-12);
    close(scalar(&report, Output::VolumeFlowRate), 1.0e-5, 5.0e-9);
    close(
        region(&report, Output::AbsolutePressure, "outlet"),
        99_995.0,
        0.0,
    );

    let Quantity::Field {
        positions_m,
        values,
        ..
    } = &report.quantities[&Output::AxialVelocity]
    else {
        panic!("expected a velocity field");
    };
    for (&[x, y, z], &velocity) in positions_m.iter().zip(values) {
        assert_eq!((x, z), (1.0, 0.0));
        let expected = (2.4 / 2.0) / (2.0 * 1.0e-3) * y * (0.01 - y);
        close(velocity, expected, 4.0e-6);
    }
}

#[test]
fn mixed_pressure_references_preserve_a_small_driving_gradient() {
    let mut model = Model::from_json(INPUT).unwrap();
    let case = channel(&mut model);
    case.drive = FlowDrive::Pressure {
        inlet: PressureAtPort {
            region: "inlet".into(),
            pressure: Pressure::Gauge {
                value_pa: 1e-12,
                reference_absolute_pa: 101325.0,
            },
        },
        outlet: PressureAtPort {
            region: "outlet".into(),
            pressure: Pressure::Absolute { value_pa: 101325.0 },
        },
    };
    let report = solve(&model).unwrap();
    assert_eq!(report.stop_reason, StopReason::Converged);
    close(scalar(&report, Output::PressureLoss), 1e-12, 1e-26);
    let exact_flow = 1e-12 * 0.1 * 0.01_f64.powi(3) / (12.0 * 1e-3 * 2.0);
    close(
        scalar(&report, Output::VolumeFlowRate),
        exact_flow,
        exact_flow * 0.003,
    );
}

#[test]
fn pressure_reference_and_wall_assignments_are_checked() {
    let mut model = Model::from_json(INPUT).unwrap();
    let case = channel(&mut model);
    pressure_drive(
        case,
        Pressure::Gauge {
            value_pa: -100_001.0,
            reference_absolute_pa: 100_000.0,
        },
        2.4,
    );
    let failure = solve(&model).unwrap_err().to_string();
    assert!(
        failure.contains("outlet.pressure.absolute_pressure_pa"),
        "{failure}"
    );

    let mut model = Model::from_json(INPUT).unwrap();
    channel(&mut model).walls[0].region = "old_wall".to_owned();
    assert!(
        solve(&model)
            .unwrap_err()
            .to_string()
            .contains("each transverse mesh endpoint")
    );
}
