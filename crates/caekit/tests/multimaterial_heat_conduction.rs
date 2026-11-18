mod common;

use caekit::physics::heat_conduction::{HeatConduction, MultimaterialHeatConduction, SolidThermal};
use caekit::{Model, Output, Physics, Quantity, RunReport, SampleLocation, StopReason, solve};

use common::close;

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
}

#[test]
fn two_regions_can_share_a_material_and_recover_the_uniform_solution() {
    let mut model = example();
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
    for output in &model.outputs {
        assert_eq!(
            serde_json::to_value(&report.quantities[output]).unwrap(),
            serde_json::to_value(&reference.quantities[output]).unwrap()
        );
    }
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
