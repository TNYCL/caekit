use std::path::Path;
use std::process::Command;

use caekit::{Model, StopReason, solve};
use serde_json::Value;

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

fn check_and_run(name: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name);
    let model = Model::from_json(&std::fs::read(&path).unwrap())
        .unwrap_or_else(|error| panic!("{name}: {error}"));
    let report = solve(&model).unwrap_or_else(|error| panic!("{name}: {error}"));
    assert_eq!(report.stop_reason, StopReason::Converged, "{name}");
    for (command, expected) in [
        ("check", serde_json::to_value(model.validate().unwrap())),
        ("run", serde_json::to_value(report)),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_caekit"))
            .arg(command)
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{command} {name}: expected exit code 0, measured {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        let actual: Value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|error| panic!("{command} {name}: invalid JSON: {error}"));
        assert_eq!(actual, expected.unwrap(), "{command} {name}");
    }
}
