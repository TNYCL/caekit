mod common;

use caekit::physics::axial_bar::BarBoundary;
use caekit::{Error, Model, Output, Physics, Quantity, StopReason, solve};
use common::{close, region};

const INPUT: &[u8] = include_bytes!("../examples/axial_bar.json");

#[test]
fn solves_analytic_tension_case() {
    let report = solve(&Model::from_json(INPUT).unwrap()).unwrap();

    assert_eq!(report.stop_reason, StopReason::Converged);
    close(
        region(&report, Output::EndDisplacement, "tip"),
        5.0e-5,
        1.0e-13,
    );
    close(
        region(&report, Output::ReactionForce, "clamp"),
        -1000.0,
        1.0e-7,
    );
    close(report.balances[0].imbalance, 0.0, 1.0e-7);

    let Quantity::Field { values, .. } = &report.quantities[&Output::AxialStress] else {
        panic!("expected a stress field");
    };
    for &stress in values {
        close(stress, 1.0e7, 1.0e-3);
    }
    let stage = &report.convergence[0];
    assert!(stage.final_true_residual_l2.unwrap() <= stage.residual_threshold.unwrap());
}

#[test]
fn loaded_start_has_negative_projection_and_geometry_is_preserved() {
    let mut model = Model::from_json(INPUT).unwrap();
    let Physics::AxialBar(case) = &mut model.physics else {
        panic!("expected the axial bar fixture");
    };
    case.geometry.frame.origin_m = [3.0, 0.0, -2.0];
    case.geometry.frame.direction = [0.0, 1.0, 0.0];
    case.boundary_conditions = vec![
        BarBoundary::Force {
            region: "clamp".to_owned(),
            value_n: -1000.0,
        },
        BarBoundary::Displacement {
            region: "tip".to_owned(),
            value_m: 0.0,
        },
    ];

    let report = solve(&model).unwrap();
    close(
        region(&report, Output::EndDisplacement, "clamp"),
        -5.0e-5,
        1.0e-13,
    );
    close(
        region(&report, Output::ReactionForce, "tip"),
        1000.0,
        1.0e-7,
    );
    let Quantity::Field { positions_m, .. } = &report.quantities[&Output::AxialDisplacement] else {
        panic!("expected a displacement field");
    };
    assert_eq!(positions_m[0], [3.0, 0.0, -2.0]);
    assert_eq!(positions_m[5], [3.0, 1.0, -2.0]);
}

#[test]
fn supports_nonzero_displacement_conditions_at_both_ends() {
    let mut model = Model::from_json(INPUT).unwrap();
    let Physics::AxialBar(case) = &mut model.physics else {
        panic!("expected the axial bar fixture");
    };
    case.boundary_conditions = vec![
        BarBoundary::Displacement {
            region: "clamp".to_owned(),
            value_m: 1.0e-5,
        },
        BarBoundary::Displacement {
            region: "tip".to_owned(),
            value_m: 6.0e-5,
        },
    ];
    let report = solve(&model).unwrap();
    close(
        region(&report, Output::ReactionForce, "tip"),
        1000.0,
        1.0e-6,
    );
    close(
        region(&report, Output::ReactionForce, "clamp"),
        -1000.0,
        1.0e-6,
    );
}

#[test]
fn separates_model_identity_and_execution_outcome() {
    let mut model = Model::from_json(INPUT).unwrap();
    let before = solve(&model).unwrap();
    let normalized = serde_json::to_vec_pretty(&model).unwrap();
    let after = solve(&Model::from_json(&normalized).unwrap()).unwrap();
    assert_eq!(before.model_sha256, after.model_sha256);

    model.linear_solver.max_iterations = 1;
    let stopped = solve(&model).unwrap();
    assert_ne!(before.model_sha256, stopped.model_sha256);
    assert_eq!(stopped.stop_reason, StopReason::IterationLimit);
    assert!(stopped.quantities.is_empty());
    assert_eq!(stopped.convergence[0].residual_l2_history.len(), 2);
}

#[test]
fn rejects_future_schemas_unknown_fields_and_invalid_conditions() {
    let mut json: serde_json::Value = serde_json::from_slice(INPUT).unwrap();
    json["schema_version"] = 2.into();
    assert!(matches!(
        Model::from_json(&serde_json::to_vec(&json).unwrap()),
        Err(Error::UnsupportedModelVersion(2))
    ));

    json["schema_version"] = 1.into();
    json["physics"]["boundary_conditions"][0]["forcce"] = 1.into();
    assert!(
        Model::from_json(&serde_json::to_vec(&json).unwrap())
            .unwrap_err()
            .to_string()
            .contains("unknown field")
    );

    let mut model = Model::from_json(INPUT).unwrap();
    let Physics::AxialBar(case) = &mut model.physics else {
        panic!("expected the axial bar fixture");
    };
    case.boundary_conditions.pop();
    assert!(
        solve(&model)
            .unwrap_err()
            .to_string()
            .contains("each endpoint requires")
    );

    let mut model = Model::from_json(INPUT).unwrap();
    let Physics::AxialBar(case) = &mut model.physics else {
        panic!("expected the axial bar fixture");
    };
    case.mesh.vertices_m[1] = 0.0;
    assert!(
        solve(&model)
            .unwrap_err()
            .to_string()
            .contains("physics.mesh.vertices_m[1]")
    );
}
