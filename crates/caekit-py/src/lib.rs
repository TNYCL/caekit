//! Python bindings for CAEKit (`caekit._caekit`).
//!
//! The model and the run report stay JSON documents at the boundary: Python passes the model
//! as UTF-8 JSON bytes and receives the report as JSON text, which `caekit/__init__.py`
//! decodes with the standard library. That keeps the binding independent of the shape of the
//! model: new physics kinds, fields and outputs appear without a change here. The solve runs
//! with the interpreter lock released.

use caekit::{Error, Model};
use pyo3::create_exception;
use pyo3::prelude::*;

create_exception!(
    _caekit,
    CaekitError,
    pyo3::exceptions::PyException,
    "Base class of all CAEKit errors."
);
create_exception!(
    _caekit,
    InvalidModelError,
    CaekitError,
    "The model is not valid JSON, uses an unsupported schema version, or fails validation."
);
create_exception!(
    _caekit,
    CalculationError,
    CaekitError,
    "The calculation left the finite binary64 range or could not allocate its working memory."
);

fn map_err(error: Error) -> PyErr {
    let message = error.to_string();
    match error {
        Error::Json(_) | Error::UnsupportedModelVersion(_) | Error::InvalidModel { .. } => {
            InvalidModelError::new_err(message)
        }
        Error::NonFiniteResult { .. } | Error::InsufficientMemory { .. } => {
            CalculationError::new_err(message)
        }
    }
}

/// Validates the model document and returns the validation report as JSON text.
#[pyfunction]
fn check_json(py: Python<'_>, model: &[u8]) -> PyResult<String> {
    py.detach(|| {
        let check = Model::from_json(model)?.validate()?;
        serde_json::to_string(&check).map_err(Error::from)
    })
    .map_err(map_err)
}

/// Validates and solves the model document and returns the run report as JSON text.
#[pyfunction]
fn solve_json(py: Python<'_>, model: &[u8]) -> PyResult<String> {
    py.detach(|| {
        let report = caekit::solve(&Model::from_json(model)?)?;
        serde_json::to_string(&report).map_err(Error::from)
    })
    .map_err(map_err)
}

#[pymodule]
fn _caekit(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = module.py();
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    module.add("MODEL_SCHEMA_VERSION", caekit::MODEL_SCHEMA_VERSION)?;
    module.add("RESULT_SCHEMA_VERSION", caekit::RESULT_SCHEMA_VERSION)?;
    module.add("CaekitError", py.get_type::<CaekitError>())?;
    module.add("InvalidModelError", py.get_type::<InvalidModelError>())?;
    module.add("CalculationError", py.get_type::<CalculationError>())?;
    module.add_function(wrap_pyfunction!(check_json, module)?)?;
    module.add_function(wrap_pyfunction!(solve_json, module)?)?;
    Ok(())
}
