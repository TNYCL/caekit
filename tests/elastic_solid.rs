mod common;

use caekit::physics::elastic_solid::{ElasticSolid, SolidBoundary};
use caekit::physics::laminar_channel::Pressure;
use caekit::{Model, Output, Physics, Quantity, RunReport, StopReason, solve};
use common::{close, scalar};

const INPUT: &[u8] = include_bytes!("../examples/elastic_solid.json");

fn elastic(model: &mut Model) -> &mut ElasticSolid {
    let Physics::ElasticSolid(case) = &mut model.physics else {
        panic!("expected the solid fixture");
    };
    case
}

fn balance<'a>(report: &'a RunReport, name: &str) -> &'a caekit::Balance {
    report
        .balances
        .iter()
        .find(|balance| balance.name == name)
        .unwrap()
}

fn check_affine(report: &RunReport, axial_stress: f64) {
    assert_eq!(report.stop_reason, StopReason::Converged);
    let Quantity::VectorField {
        positions_m,
        values,
        ..
    } = &report.quantities[&Output::DisplacementVector]
    else {
        panic!("expected a displacement field");
    };
    let strain = axial_stress / 200e9;
    for (&[x, y, z], &[ux, uy, uz]) in positions_m.iter().zip(values) {
        close(ux, strain * (x - 1.0), 1.0e-12);
        close(uy, -0.31 * strain * (y - 2.0), 1.0e-12);
        close(uz, -0.31 * strain * (z + 1.0), 1.0e-12);
    }
    let Quantity::TensorField { values, .. } = &report.quantities[&Output::CauchyStress] else {
        panic!("expected a tensor stress field");
    };
    for tensor in values {
        for (i, row) in tensor.iter().enumerate() {
            for (j, component) in row.iter().enumerate() {
                let expected = if (i, j) == (0, 0) { axial_stress } else { 0.0 };
                close(*component, expected, 0.01);
            }
        }
    }
    let Quantity::Field { values, .. } = &report.quantities[&Output::VonMisesStress] else {
        panic!("expected a von Mises stress field");
    };
    for &stress in values {
        close(stress, axial_stress.abs(), 0.01);
    }
    close(
        balance(report, "solid_force_x").terms["reaction"],
        -axial_stress * 0.2,
        1.0e-3,
    );
    for balance in &report.balances {
        close(balance.imbalance, 0.0, 1.0e-3);
    }
    // Strain-energy density is sigma_x * epsilon_x / 2, integrated over 0.4 m³.
    close(
        scalar(report, Output::ElasticEnergy),
        0.5 * axial_stress * strain * 0.4,
        1.0e-6,
    );
}

#[test]
fn completes_the_three_dimensional_patch_test() {
    let report = solve(&Model::from_json(INPUT).unwrap()).unwrap();
    assert_eq!(report.meshes[0].cell_count, 6);
    let quality = report.meshes[0].quality.as_ref().unwrap();
    assert!((0.1..=1.0).contains(&quality.minimum_mean_ratio));
    assert!(quality.cell_index < 6);
    check_affine(&report, 1.0e7);
}

#[test]
fn small_traction_is_resolved_under_an_imposed_rigid_translation() {
    let mut model = Model::from_json(INPUT).unwrap();
    let case = elastic(&mut model);
    let translation = [0.001, -0.002, 0.004];
    for boundary in &mut case.boundary_conditions {
        match boundary {
            SolidBoundary::Displacement { components_m, .. } => {
                for (component, shift) in components_m.iter_mut().zip(translation) {
                    if let Some(value) = component {
                        *value += shift;
                    }
                }
            }
            SolidBoundary::Traction { components_pa, .. } => *components_pa = [0.001, 0.0, 0.0],
            SolidBoundary::Pressure { .. } => unreachable!(),
        }
    }
    let report = solve(&model).unwrap();
    assert_eq!(report.stop_reason, StopReason::Converged);
    // The exact uniaxial strain is traction / E = 5e-15.
    close(scalar(&report, Output::ElasticEnergy), 1e-18, 1e-26);
    close(
        balance(&report, "solid_force_x").terms["reaction"],
        -2e-4,
        1e-12,
    );
    let Quantity::VectorField {
        positions_m,
        values,
        ..
    } = &report.quantities[&Output::DisplacementVector]
    else {
        panic!("expected a displacement field");
    };
    for (&[x, y, z], &u) in positions_m.iter().zip(values) {
        let strain = 5e-15;
        close(u[0], translation[0] + strain * (x - 1.0), 1e-18);
        close(u[1], translation[1] - 0.31 * strain * (y - 2.0), 1e-18);
        close(u[2], translation[2] - 0.31 * strain * (z + 1.0), 1e-18);
    }
}

#[test]
fn gauge_pressure_uses_outward_normals_and_ambient_reference() {
    let mut model = Model::from_json(INPUT).unwrap();
    let case = elastic(&mut model);
    *case.boundary_conditions.last_mut().unwrap() = SolidBoundary::Pressure {
        region: "x1".to_owned(),
        applied: Pressure::Gauge {
            value_pa: 1.0e7,
            reference_absolute_pa: 123_000.0,
        },
        ambient: Pressure::Absolute {
            value_pa: 123_000.0,
        },
    };
    check_affine(&solve(&model).unwrap(), -1.0e7);
}

#[test]
fn body_load_integrates_over_the_actual_volume() {
    let mut model = Model::from_json(INPUT).unwrap();
    let case = elastic(&mut model);
    case.boundary_conditions.pop();
    case.body_force_n_m3 = [0.0, -600.0, 0.0];
    let report = solve(&model).unwrap();
    assert_eq!(report.stop_reason, StopReason::Converged);
    close(
        balance(&report, "solid_force_y").terms["applied"],
        -240.0,
        1.0e-9,
    );
    close(
        balance(&report, "solid_force_y").terms["reaction"],
        240.0,
        1.0e-6,
    );
    // The body's centroid is one metre from the origin along x.
    close(
        balance(&report, "solid_moment_z").terms["applied"],
        -240.0,
        1.0e-9,
    );
    close(balance(&report, "solid_moment_z").imbalance, 0.0, 1.0e-6);
}

#[test]
fn diagnoses_inverted_cells_duplicate_faces_and_underconstrained_modes() {
    let mut model = Model::from_json(INPUT).unwrap();
    elastic(&mut model).mesh.cells[1].swap(0, 1);
    let error = model.validate().unwrap_err().to_string();
    assert!(
        error.contains("physics.mesh.cells[1]") && error.contains("inverted"),
        "{error}"
    );

    let mut model = Model::from_json(INPUT).unwrap();
    let case = elastic(&mut model);
    let duplicate = case.mesh.boundary_regions[0].faces[0];
    case.mesh.boundary_regions[1].faces.push(duplicate);
    assert!(
        model
            .validate()
            .unwrap_err()
            .to_string()
            .contains("already assigned face")
    );

    let mut model = Model::from_json(INPUT).unwrap();
    elastic(&mut model).boundary_conditions.truncate(1);
    assert!(
        model
            .validate()
            .unwrap_err()
            .to_string()
            .contains("six rigid-body modes")
    );
}

#[test]
fn rejects_stale_and_conflicting_region_constraints() {
    let mut model = Model::from_json(INPUT).unwrap();
    let case = elastic(&mut model);
    case.mesh.boundary_regions[0].name = "revised_x".to_owned();
    assert!(
        model
            .validate()
            .unwrap_err()
            .to_string()
            .contains("unknown exterior region")
    );

    let mut model = Model::from_json(INPUT).unwrap();
    elastic(&mut model)
        .boundary_conditions
        .push(SolidBoundary::Displacement {
            region: "y0".to_owned(),
            components_m: [Some(1.0e-6), None, None],
        });
    assert!(
        model
            .validate()
            .unwrap_err()
            .to_string()
            .contains("conflicting displacement at vertex")
    );
}
