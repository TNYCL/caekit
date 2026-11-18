//! WebAssembly bindings for CAEKit (the npm package `caekit-wasm`).
//!
//! The model and the run report stay JSON documents at the boundary: JavaScript passes the
//! model as JSON text, UTF-8 JSON bytes or a plain object, and receives the report as a
//! plain object decoded by `JSON.parse`. That keeps the binding independent of the shape of
//! the model: new physics kinds, fields and outputs appear without a change here. The
//! TypeScript declarations below describe version 1 of the result document and are checked
//! against real reports by `tests/typings_test.mjs`.

use caekit::{Error, Model};
use js_sys::{JSON, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TYPESCRIPT: &'static str = r#"
/** A model document: JSON text, UTF-8 JSON bytes, or the parsed object. */
export type ModelInput = string | Uint8Array | object;

/** Why a numerical stage, or the run as a whole, stopped. */
export type StopReason = "converged" | "iteration_limit" | "numerical_breakdown";

/** Where the values of a field are sampled. */
export type SampleLocation = "node" | "cell" | "interface_face" | "sample";

/** Identity of the geometry that the mesh and assignments belong to. */
export interface GeometryRevision {
  id: string;
  revision: string;
}

/** Size and quality metrics of one validated mesh. */
export interface MeshMetrics {
  name: string;
  cell_count: number;
  smallest_cell_m: number;
  smallest_cell_index: number;
  largest_cell_m: number;
  quality?: { minimum_mean_ratio: number; cell_index: number };
}

/** Result of `check`. */
export interface ModelCheck {
  model_schema_version: number;
  meshes: MeshMetrics[];
}

/** Where and how the run was executed. */
export interface Execution {
  backend: string;
  precision: string;
  architecture: string;
  operating_system: string;
  threads: number;
  solver_version: string;
}

/** Convergence evidence of one numerical stage. */
export interface Convergence {
  stage: string;
  algorithm: string;
  stop_reason: StopReason;
  iterations: number;
  residual_unit: string;
  residual_threshold: number | null;
  residual_l2_history: number[];
  final_true_residual_l2: number | null;
  diagnostic?: string;
}

/** Signed terms of a conservation balance and their signed sum. */
export interface Balance {
  name: string;
  unit: string;
  terms: Record<string, number>;
  imbalance: number;
}

/** Transfer between two volume regions; positive from `from_region` to `to_region`. */
export interface InterfaceValue {
  from_region: string;
  to_region: string;
  value: number;
}

type Vector = [number, number, number];

/** A requested output with its unit and, for fields, its sample positions in metres. */
export type Quantity =
  | { kind: "scalar"; unit: string; value: number }
  | { kind: "field"; unit: string; location: SampleLocation; positions_m: Vector[]; values: number[] }
  | { kind: "vector_field"; unit: string; location: SampleLocation; positions_m: Vector[]; values: Vector[] }
  | { kind: "tensor_field"; unit: string; location: SampleLocation; positions_m: Vector[]; values: [Vector, Vector, Vector][] }
  | { kind: "by_region"; unit: string; values: Record<string, number> }
  | { kind: "by_interface"; unit: string; values: InterfaceValue[] };

/** Result of `solve`: version 1 of the result document. */
export interface RunReport {
  schema_version: number;
  model_schema_version: number;
  model_name: string;
  model_sha256: string;
  geometry: GeometryRevision[];
  meshes: MeshMetrics[];
  execution: Execution;
  method: string;
  stop_reason: StopReason;
  convergence: Convergence[];
  balances: Balance[];
  quantities: Record<string, Quantity>;
}

/**
 * Thrown by `check` and `solve`. `kind` is `"invalid_model"` for a model that is not valid
 * JSON, uses an unsupported schema version or fails validation, `"calculation"` when the
 * calculation leaves the finite binary64 range or cannot allocate its working memory, and
 * `"bad_argument"` for an argument that is not a model document.
 */
export interface CaekitError extends Error {
  name: "CaekitError";
  kind: "invalid_model" | "calculation" | "bad_argument";
}

/** Validates a model and returns its schema version and mesh metrics. */
export function check(model: ModelInput): ModelCheck;

/**
 * Validates and solves a model on the reference double-precision CPU path.
 *
 * A run that stops without convergence is returned, not thrown: read `stop_reason`. Such a
 * report keeps the convergence traces and withholds the physical fields and balances.
 */
export function solve(model: ModelInput): RunReport;

/** Version of the CAEKit solver compiled into this module. */
export function version(): string;
"#;

fn failure(kind: &str, message: &str) -> JsValue {
    let error = js_sys::Error::new(message);
    error.set_name("CaekitError");
    // Setting a property on a fresh Error object cannot fail.
    let _ = Reflect::set(&error, &JsValue::from_str("kind"), &JsValue::from_str(kind));
    error.into()
}

fn map_err(error: Error) -> JsValue {
    let kind = match error {
        Error::Json(_) | Error::UnsupportedModelVersion(_) | Error::InvalidModel { .. } => {
            "invalid_model"
        }
        Error::NonFiniteResult { .. } | Error::InsufficientMemory { .. } => "calculation",
    };
    failure(kind, &error.to_string())
}

/// The model as UTF-8 JSON bytes.
fn document(model: &JsValue) -> Result<Vec<u8>, JsValue> {
    if let Some(text) = model.as_string() {
        return Ok(text.into_bytes());
    }
    if let Some(bytes) = model.dyn_ref::<Uint8Array>() {
        return Ok(bytes.to_vec());
    }
    if model.is_object() && !model.is_function() {
        let text = JSON::stringify(model)
            .map_err(|_| failure("bad_argument", "model cannot be written as JSON"))?;
        if let Some(text) = text.as_string() {
            return Ok(text.into_bytes());
        }
    }
    Err(failure(
        "bad_argument",
        "model must be JSON text, a Uint8Array of UTF-8 JSON or an object",
    ))
}

/// Validates a model and returns its schema version and mesh metrics.
#[wasm_bindgen(skip_typescript)]
pub fn check(model: JsValue) -> Result<JsValue, JsValue> {
    let bytes = document(&model)?;
    let check = Model::from_json(&bytes)
        .and_then(|model| model.validate())
        .and_then(|check| serde_json::to_string(&check).map_err(Error::from))
        .map_err(map_err)?;
    JSON::parse(&check)
}

/// Validates and solves a model on the reference double-precision CPU path.
#[wasm_bindgen(skip_typescript)]
pub fn solve(model: JsValue) -> Result<JsValue, JsValue> {
    let bytes = document(&model)?;
    let report = Model::from_json(&bytes)
        .and_then(|model| caekit::solve(&model))
        .and_then(|report| serde_json::to_string(&report).map_err(Error::from))
        .map_err(map_err)?;
    JSON::parse(&report)
}

/// Version of the CAEKit solver compiled into this module.
#[wasm_bindgen(skip_typescript)]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}
