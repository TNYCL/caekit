//! Sparse linear methods and convergence reporting.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinearSettings {
    pub relative_residual_tolerance: f64,
    pub absolute_residual_tolerance: f64,
    pub max_iterations: usize,
}

impl Default for LinearSettings {
    fn default() -> Self {
        Self {
            relative_residual_tolerance: 1.0e-10,
            absolute_residual_tolerance: 0.0,
            max_iterations: 20_000,
        }
    }
}

#[derive(Debug)]
pub struct InvalidLinearSetting {
    pub field: &'static str,
    pub message: &'static str,
}

impl std::fmt::Display for InvalidLinearSetting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

impl std::error::Error for InvalidLinearSetting {}

impl LinearSettings {
    pub fn validate(self) -> Result<(), InvalidLinearSetting> {
        if !(0.0 < self.relative_residual_tolerance && self.relative_residual_tolerance < 1.0) {
            return Err(InvalidLinearSetting {
                field: "relative_residual_tolerance",
                message: "must be greater than zero and less than one",
            });
        }
        if !self.absolute_residual_tolerance.is_finite() || self.absolute_residual_tolerance < 0.0 {
            return Err(InvalidLinearSetting {
                field: "absolute_residual_tolerance",
                message: "must be nonnegative and finite",
            });
        }
        if self.max_iterations == 0 {
            return Err(InvalidLinearSetting {
                field: "max_iterations",
                message: "must be positive",
            });
        }
        Ok(())
    }
}

/// Stop reasons for a numerical stage. A failed stage retains its convergence evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    Converged,
    IterationLimit,
    NumericalBreakdown,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Convergence {
    pub stage: String,
    pub algorithm: String,
    pub stop_reason: StopReason,
    pub iterations: usize,
    pub residual_unit: String,
    pub residual_threshold: Option<f64>,
    pub residual_l2_history: Vec<f64>,
    pub final_true_residual_l2: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
}

/// Sparse triplet accumulation with deterministic row and column ordering.
pub struct SparseAssembly {
    rows: Vec<BTreeMap<usize, f64>>,
    invalid: bool,
}

impl SparseAssembly {
    pub fn new(size: usize) -> Self {
        Self {
            rows: vec![BTreeMap::new(); size],
            invalid: false,
        }
    }

    pub fn add(&mut self, row: usize, column: usize, entry: f64) {
        if row >= self.rows.len() || column >= self.rows.len() {
            self.invalid = true;
            return;
        }
        *self.rows[row].entry(column).or_default() += entry;
    }

    /// Invalid indices, dimensions or settings produce a numerical breakdown at iteration zero.
    pub fn solve(
        self,
        rhs: &[f64],
        settings: LinearSettings,
        stage: &str,
        residual_unit: &str,
    ) -> (Vec<f64>, Convergence) {
        let stats = Convergence {
            stage: stage.to_owned(),
            algorithm: "jacobi_preconditioned_conjugate_gradient".to_owned(),
            stop_reason: StopReason::NumericalBreakdown,
            iterations: 0,
            residual_unit: residual_unit.to_owned(),
            residual_threshold: None,
            residual_l2_history: Vec::new(),
            final_true_residual_l2: None,
            diagnostic: None,
        };
        if self.invalid || rhs.len() != self.rows.len() || settings.validate().is_err() {
            return (vec![0.0; self.rows.len()], stats);
        }
        pcg(&CsrMatrix::from_rows(self.rows), rhs, settings, stats)
    }
}

struct CsrMatrix {
    offsets: Vec<usize>,
    columns: Vec<usize>,
    entries: Vec<f64>,
    diagonal: Vec<f64>,
}

impl CsrMatrix {
    fn from_rows(rows: Vec<BTreeMap<usize, f64>>) -> Self {
        let mut matrix = Self {
            offsets: Vec::with_capacity(rows.len() + 1),
            columns: Vec::new(),
            entries: Vec::new(),
            diagonal: vec![0.0; rows.len()],
        };
        matrix.offsets.push(0);
        for (i, row) in rows.into_iter().enumerate() {
            for (j, entry) in row {
                if i == j {
                    matrix.diagonal[i] = entry;
                }
                if entry != 0.0 {
                    matrix.columns.push(j);
                    matrix.entries.push(entry);
                }
            }
            matrix.offsets.push(matrix.entries.len());
        }
        matrix
    }

    fn multiply(&self, x: &[f64], output: &mut [f64]) {
        for (row, value) in output.iter_mut().enumerate() {
            *value = (self.offsets[row]..self.offsets[row + 1])
                .map(|i| self.entries[i] * x[self.columns[i]])
                .sum();
        }
    }

    /// Conservative binary64 rounding scale for computing b - A*x row by row.
    fn residual_roundoff_l2(&self, x: &[f64], rhs: &[f64]) -> f64 {
        rhs.iter().enumerate().fold(0.0_f64, |bound, (row, b)| {
            let indices = self.offsets[row]..self.offsets[row + 1];
            let operations = 2.0 * indices.len() as f64 + 1.0;
            let scale = indices.fold(b.abs(), |sum, i| {
                sum + (self.entries[i] * x[self.columns[i]]).abs()
            });
            bound.hypot(operations * f64::EPSILON * scale)
        })
    }
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}

pub fn norm(values: &[f64]) -> f64 {
    values.iter().fold(0.0_f64, |norm, x| norm.hypot(*x))
}

fn pcg(
    matrix: &CsrMatrix,
    rhs: &[f64],
    settings: LinearSettings,
    mut stats: Convergence,
) -> (Vec<f64>, Convergence) {
    let mut x = vec![0.0; rhs.len()];
    if !rhs.iter().all(|x| x.is_finite())
        || !matrix.entries.iter().all(|x| x.is_finite())
        || !matrix.diagonal.iter().all(|&d| d > 0.0)
    {
        return (x, stats);
    }

    let initial_norm = norm(rhs);
    if !initial_norm.is_finite() {
        return (x, stats);
    }
    let threshold = settings
        .absolute_residual_tolerance
        .max(settings.relative_residual_tolerance * initial_norm);
    stats.residual_threshold = Some(threshold);
    stats.residual_l2_history.push(initial_norm);
    if initial_norm <= threshold {
        stats.stop_reason = StopReason::Converged;
        stats.final_true_residual_l2 = Some(initial_norm);
        return (x, stats);
    }

    let mut residual = rhs.to_vec();
    let inverse_diagonal: Vec<_> = matrix.diagonal.iter().map(|d| d.recip()).collect();
    let mut preconditioned: Vec<_> = residual
        .iter()
        .zip(&inverse_diagonal)
        .map(|(r, d)| r * d)
        .collect();
    let mut direction = preconditioned.clone();
    let mut residual_dot = dot(&residual, &preconditioned);
    let mut product = vec![0.0; rhs.len()];
    let mut restart_baseline = f64::INFINITY;
    let mut unproductive_restarts = 0;

    for iteration in 1..=settings.max_iterations {
        matrix.multiply(&direction, &mut product);
        let denominator = dot(&direction, &product);
        if !residual_dot.is_finite()
            || residual_dot <= 0.0
            || !denominator.is_finite()
            || denominator <= 0.0
        {
            break;
        }
        let step = residual_dot / denominator;
        if !step.is_finite() {
            break;
        }
        for i in 0..x.len() {
            x[i] += step * direction[i];
            residual[i] -= step * product[i];
        }
        stats.iterations = iteration;
        let estimate = norm(&residual);
        if !estimate.is_finite() {
            break;
        }
        stats.residual_l2_history.push(estimate);

        // Recursive residuals drift from b - A*x. Verify convergence against the latter.
        let restart = estimate <= threshold;
        if restart {
            matrix.multiply(&x, &mut product);
            for i in 0..rhs.len() {
                residual[i] = rhs[i] - product[i];
            }
            let actual = norm(&residual);
            if actual.is_finite() && actual <= threshold {
                stats.stop_reason = StopReason::Converged;
                stats.final_true_residual_l2 = Some(actual);
                return (x, stats);
            }
            if !actual.is_finite() {
                break;
            }
            // Count complete correction attempts only after verifying the true residual.
            if actual < 0.75 * restart_baseline {
                restart_baseline = actual;
                unproductive_restarts = 0;
            } else {
                unproductive_restarts += 1;
                if unproductive_restarts >= 8 {
                    let roundoff = matrix.residual_roundoff_l2(&x, rhs);
                    if roundoff.is_finite() && actual <= roundoff {
                        stats.final_true_residual_l2 = Some(actual);
                        stats.diagnostic = Some(format!(
                            "True residual stagnated at {actual:.6e} {} above the requested threshold {threshold:.6e} {} after repeated corrections at binary64 precision.",
                            stats.residual_unit, stats.residual_unit
                        ));
                        return (x, stats);
                    }
                }
            }
        }
        for i in 0..rhs.len() {
            preconditioned[i] = residual[i] * inverse_diagonal[i];
        }
        let next_dot = dot(&residual, &preconditioned);
        let beta = if restart {
            0.0
        } else {
            next_dot / residual_dot
        };
        for i in 0..rhs.len() {
            direction[i] = preconditioned[i] + beta * direction[i];
        }
        residual_dot = next_dot;
    }

    matrix.multiply(&x, &mut product);
    for i in 0..rhs.len() {
        residual[i] = rhs[i] - product[i];
    }
    let actual = norm(&residual);
    if actual.is_finite() {
        stats.final_true_residual_l2 = Some(actual);
        if actual <= threshold {
            stats.stop_reason = StopReason::Converged;
        } else if stats.iterations == settings.max_iterations {
            stats.stop_reason = StopReason::IterationLimit;
        }
    }
    (x, stats)
}

/// Symmetric elimination of prescribed degrees of freedom.
pub struct ConstrainedSystem {
    reduced: SparseAssembly,
    rhs: Vec<f64>,
    prescribed: Vec<Option<f64>>,
    reduced_index: Vec<Option<usize>>,
}

impl ConstrainedSystem {
    pub fn new(prescribed: Vec<Option<f64>>, loads: &[f64]) -> Self {
        let mut rhs = Vec::new();
        let reduced_index = prescribed
            .iter()
            .enumerate()
            .map(|(i, value)| {
                if value.is_some() {
                    None
                } else {
                    let index = rhs.len();
                    rhs.push(loads.get(i).copied().unwrap_or(0.0));
                    Some(index)
                }
            })
            .collect();
        let mut reduced = SparseAssembly::new(rhs.len());
        reduced.invalid = loads.len() != prescribed.len()
            || !prescribed
                .iter()
                .flatten()
                .chain(loads)
                .all(|v| v.is_finite());
        Self {
            reduced,
            rhs,
            prescribed,
            reduced_index,
        }
    }

    pub fn add(&mut self, row: usize, column: usize, entry: f64) {
        if row >= self.reduced_index.len()
            || column >= self.reduced_index.len()
            || !entry.is_finite()
        {
            self.reduced.invalid = true;
            return;
        }
        if let Some(i) = self.reduced_index[row] {
            match self.reduced_index[column] {
                Some(j) => self.reduced.add(i, j, entry),
                None => self.rhs[i] -= entry * self.prescribed[column].unwrap(),
            }
        }
    }

    pub fn solve(
        self,
        settings: LinearSettings,
        stage: &str,
        residual_unit: &str,
    ) -> (Vec<f64>, Convergence) {
        let (free, convergence) = self
            .reduced
            .solve(&self.rhs, settings, stage, residual_unit);
        let full = self
            .prescribed
            .into_iter()
            .zip(self.reduced_index)
            .map(|(value, index)| value.unwrap_or_else(|| free[index.unwrap()]))
            .collect();
        (full, convergence)
    }
}
