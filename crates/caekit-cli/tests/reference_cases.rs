use std::path::Path;
use std::process::Command;

use caekit::{Model, StopReason, solve};
use serde::Serialize;

#[test]
fn cli_checks_and_solves_the_saved_reference_models() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    for name in [
        "axial_bar.json",
        "laminar_channel.json",
        "cooling_channel.json",
        "elastic_solid.json",
    ] {
        let path = examples.join(name);
        let model = Model::from_json(&std::fs::read(&path).unwrap()).unwrap();
        let report = solve(&model).unwrap();
        assert_eq!(report.stop_reason, StopReason::Converged, "{name}");
        for (command, expected) in [
            ("check", json_line(&model.validate().unwrap())),
            ("run", json_line(&report)),
        ] {
            let output = Command::new(env!("CARGO_BIN_EXE_caekit"))
                .arg(command)
                .arg(&path)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{command} {name}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout, expected, "{command} {name}");
        }
    }
}

fn json_line(report: &impl Serialize) -> Vec<u8> {
    let mut json = serde_json::to_vec_pretty(report).unwrap();
    json.push(b'\n');
    json
}
