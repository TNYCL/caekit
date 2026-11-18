use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use caekit::{Model, RunReport, StopReason, solve};
use serde_json::{Value, json};

#[test]
fn axial_bar() {
    check_and_run("axial_bar.json");
}

#[test]
fn elastic_solid() {
    check_and_run("elastic_solid.json");
}

#[test]
fn laminar_channel() {
    check_and_run("laminar_channel.json");
}

#[test]
fn cooling_channel() {
    check_and_run("cooling_channel.json");
}

#[test]
fn heat_conduction() {
    check_and_run("heat_conduction.json");
}

#[test]
fn thermoelastic_solid() {
    check_and_run("thermoelastic_solid.json");
}

#[test]
fn invalid_thermal_boundary_inputs_use_exit_one() {
    for (conditions, message) in [
        (
            json!([{"kind": "heat_flux", "region": "x0", "inward_heat_flux_w_m2": 1000}]),
            "determine absolute temperature",
        ),
        (
            json!([
                {"kind": "temperature", "region": "x0", "temperature_k": 300},
                {"kind": "temperature", "region": "y0", "temperature_k": 400}
            ]),
            "conflicting prescribed temperatures at vertex",
        ),
    ] {
        let mut model = fixture_json("heat_conduction.json");
        model["physics"]["thermal"]["boundary_conditions"] = conditions;
        let path = TempModel::new(&model);
        for command in ["check", "run"] {
            let output = run(command, &path.0);
            assert_exit(&output, 1, command);
            assert!(output.stdout.is_empty());
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(error.contains(message), "{error}");
        }
    }
}

#[test]
fn unphysical_calculated_temperatures_use_exit_one() {
    let mut model = fixture_json("heat_conduction.json");
    model["physics"]["thermal"]["volumetric_heat_source_w_m3"] = json!(-1e9);
    let path = TempModel::new(&model);
    assert_exit(&run("check", &path.0), 0, "check");
    let output = run("run", &path.0);
    assert_exit(&output, 1, "run");
    assert!(output.stdout.is_empty());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("solution must be above absolute zero"),
        "{error}"
    );
}

#[test]
fn a_stage_iteration_limit_uses_exit_two_and_preserves_the_trace() {
    let settings = json!({
        "relative_residual_tolerance": 1e-10,
        "absolute_residual_tolerance": 0,
        "max_iterations": 1
    });
    let mut thermal_limit = fixture_json("thermoelastic_solid.json");
    thermal_limit["physics"]["thermal_solver"] = settings.clone();
    thermal_limit["physics"]["thermal"]["boundary_conditions"] = json!([
        {"kind": "heat_flux", "region": "x0", "inward_heat_flux_w_m2": 1000},
        {"kind": "convection", "region": "x1", "ambient_temperature_k": 300,
         "film_coefficient_w_m2_k": 100}
    ]);
    let mut mechanical_limit = fixture_json("thermoelastic_solid.json");
    mechanical_limit["linear_solver"] = settings;
    for (model, units) in [
        (thermal_limit, &["W"][..]),
        (mechanical_limit, &["W", "N"][..]),
    ] {
        let path = TempModel::new(&model);
        assert_exit(&run("check", &path.0), 0, "check");
        let output = run("run", &path.0);
        assert_exit(&output, 2, "run");
        let report: RunReport = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report.stop_reason, StopReason::IterationLimit);
        assert!(report.quantities.is_empty());
        assert!(report.balances.is_empty());
        assert_eq!(report.convergence.len(), units.len());
        for (index, (stage, &unit)) in report.convergence.iter().zip(units).enumerate() {
            assert_eq!(stage.residual_unit, unit);
            let (reason, within_tolerance) = if index == units.len() - 1 {
                assert_eq!(stage.iterations, 1);
                (StopReason::IterationLimit, false)
            } else {
                (StopReason::Converged, true)
            };
            assert_eq!(stage.stop_reason, reason);
            assert_eq!(
                stage.final_true_residual_l2.unwrap() <= stage.residual_threshold.unwrap(),
                within_tolerance
            );
        }
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
}

fn fixture_json(name: &str) -> Value {
    serde_json::from_slice(&std::fs::read(fixture(name)).unwrap()).unwrap()
}

fn check_and_run(name: &str) {
    let path = fixture(name);
    let model = Model::from_json(&std::fs::read(&path).unwrap())
        .unwrap_or_else(|error| panic!("{name}: {error}"));
    let report = solve(&model).unwrap_or_else(|error| panic!("{name}: {error}"));
    assert_eq!(report.stop_reason, StopReason::Converged, "{name}");
    for (command, expected) in [
        ("check", serde_json::to_value(model.validate().unwrap())),
        ("run", serde_json::to_value(report)),
    ] {
        let output = run(command, &path);
        assert_exit(&output, 0, &format!("{command} {name}"));
        let actual: Value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|error| panic!("{command} {name}: invalid JSON: {error}"));
        assert_eq!(actual, expected.unwrap(), "{command} {name}");
    }
}

fn run(command: &str, path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_caekit"))
        .arg(command)
        .arg(path)
        .output()
        .unwrap()
}

fn assert_exit(output: &Output, expected: i32, command: &str) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "{command}: measured {}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}

struct TempModel(PathBuf);

impl TempModel {
    fn new(model: &Value) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "caekit-test-{}-{}.json",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let file = File::create_new(&path).unwrap();
        let fixture = Self(path);
        serde_json::to_writer(file, model).unwrap();
        fixture
    }
}

impl Drop for TempModel {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
