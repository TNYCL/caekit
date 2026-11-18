use caekit::{Output, Quantity, RunReport};

pub fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual {actual:.16e}, expected {expected:.16e}, tolerance {tolerance:.3e}"
    );
}

#[allow(dead_code)]
pub fn scalar(report: &RunReport, output: Output) -> f64 {
    match &report.quantities[&output] {
        Quantity::Scalar { value, .. } => *value,
        quantity => panic!("expected a scalar, got {quantity:?}"),
    }
}

pub fn region(report: &RunReport, output: Output, region: &str) -> f64 {
    match &report.quantities[&output] {
        Quantity::ByRegion { values, .. } => values[region],
        quantity => panic!("expected region values, got {quantity:?}"),
    }
}
