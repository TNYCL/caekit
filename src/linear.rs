use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Error;

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

impl LinearSettings {
    pub(crate) fn validate(self) -> Result<(), Error> {
        if !(0.0 < self.relative_residual_tolerance && self.relative_residual_tolerance < 1.0) {
            return Err(Error::invalid(
                "linear_solver.relative_residual_tolerance",
                "must be greater than zero and less than one",
            ));
        }
        if !self.absolute_residual_tolerance.is_finite() || self.absolute_residual_tolerance < 0.0 {
            return Err(Error::invalid(
                "linear_solver.absolute_residual_tolerance",
                "must be nonnegative and finite",
            ));
        }
        if self.max_iterations == 0 {
            return Err(Error::invalid(
                "linear_solver.max_iterations",
                "must be positive",
            ));
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
}

/// Sparse triplet accumulation with deterministic row and column ordering.
pub(crate) struct SparseAssembly {
    rows: Vec<BTreeMap<usize, f64>>,
}

impl SparseAssembly {
    pub(crate) fn new(size: usize) -> Self {
        Self {
            rows: vec![BTreeMap::new(); size],
        }
    }

    pub(crate) fn add(&mut self, row: usize, column: usize, entry: f64) {
        *self.rows[row].entry(column).or_default() += entry;
    }

    pub(crate) fn solve(
        self,
        rhs: &[f64],
        settings: LinearSettings,
        stage: &str,
        residual_unit: &str,
    ) -> (Vec<f64>, Convergence) {
        let matrix = CsrMatrix::from_rows(self.rows);
        pcg(&matrix, rhs, settings, stage, residual_unit)
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
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}

pub(crate) fn norm(values: &[f64]) -> f64 {
    values.iter().fold(0.0_f64, |norm, x| norm.hypot(*x))
}

fn pcg(
    matrix: &CsrMatrix,
    rhs: &[f64],
    settings: LinearSettings,
    stage: &str,
    residual_unit: &str,
) -> (Vec<f64>, Convergence) {
    let mut x = vec![0.0; rhs.len()];
    let mut stats = Convergence {
        stage: stage.to_owned(),
        algorithm: "jacobi_preconditioned_conjugate_gradient".to_owned(),
        stop_reason: StopReason::NumericalBreakdown,
        iterations: 0,
        residual_unit: residual_unit.to_owned(),
        residual_threshold: None,
        residual_l2_history: Vec::new(),
        final_true_residual_l2: None,
    };
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
pub(crate) struct ConstrainedSystem {
    reduced: SparseAssembly,
    rhs: Vec<f64>,
    prescribed: Vec<Option<f64>>,
    reduced_index: Vec<Option<usize>>,
}

impl ConstrainedSystem {
    pub(crate) fn new(prescribed: Vec<Option<f64>>, loads: &[f64]) -> Self {
        let mut rhs = Vec::new();
        let reduced_index = prescribed
            .iter()
            .zip(loads)
            .map(|(value, &load)| {
                if value.is_some() {
                    None
                } else {
                    let index = rhs.len();
                    rhs.push(load);
                    Some(index)
                }
            })
            .collect();
        Self {
            reduced: SparseAssembly::new(rhs.len()),
            rhs,
            prescribed,
            reduced_index,
        }
    }

    pub(crate) fn add(&mut self, row: usize, column: usize, entry: f64) {
        if let Some(i) = self.reduced_index[row] {
            match self.reduced_index[column] {
                Some(j) => self.reduced.add(i, j, entry),
                None => self.rhs[i] -= entry * self.prescribed[column].unwrap(),
            }
        }
    }

    pub(crate) fn solve(
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
