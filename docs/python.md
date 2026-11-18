# Python package

The `caekit` package on PyPI runs the CAEKit solver from Python. It is a thin layer over
the Rust library: the model goes in as a JSON document, the report comes back as plain
dicts and lists, and the solve runs with the interpreter lock released.
[Model and result format](model.md)

## Install

```sh
pip install caekit
```

Wheels cover CPython 3.9 and newer on Linux (x86_64, aarch64), macOS (arm64, x86_64) and
Windows (x64). One wheel per platform serves every Python version (the stable ABI). On
other platforms pip builds the source distribution, which needs Rust 1.85 or newer.

## Solve a model

```python
import pathlib

import caekit

report = caekit.solve(pathlib.Path("examples/heat_conduction.json"))
assert report["stop_reason"] == "converged"

temperature = report["quantities"]["solid_temperature"]
print(temperature["unit"], min(temperature["values"]), max(temperature["values"]))

for region, power in report["quantities"]["boundary_heat_rate"]["values"].items():
    print(f"{region}: {power:+.1f} W")  # positive power enters the solid

for balance in report["balances"]:
    print(balance["name"], balance["imbalance"], balance["unit"])
```

| Function | Returns |
| --- | --- |
| `caekit.check(model)` | The validation result: `model_schema_version` and the metrics of every mesh |
| `caekit.solve(model)` | The run report: identity, meshes, execution, convergence, balances and quantities |

`model` is one of:

| Type | Read as |
| --- | --- |
| A mapping such as `dict` | The parsed model; it is written to JSON for the solver |
| `str` | JSON text |
| `bytes`, `bytearray`, `memoryview` | UTF-8 JSON |
| `pathlib.Path` or another `os.PathLike` | A file that holds the model |

A `str` is always JSON text, never a file name. Every form gives the same report and the
same `model_sha256`: the digest covers the typed model, so key order and indentation do
not change it.

`caekit.__version__` is the version of the solver, and `caekit.MODEL_SCHEMA_VERSION` and
`caekit.RESULT_SCHEMA_VERSION` are the document versions it reads and writes.

## Errors

```python
import json
import pathlib

import caekit

model = json.loads(pathlib.Path("examples/axial_bar.json").read_text(encoding="utf-8"))
model["physics"]["geometry"]["length_m"] = -1

try:
    caekit.check(model)
except caekit.InvalidModelError as error:
    print(error)  # names the path of the field, here physics.geometry.length_m
else:
    raise AssertionError("a negative length must be rejected")
```

| Exception | Raised when |
| --- | --- |
| `caekit.InvalidModelError` | The model is not valid JSON, uses an unsupported schema version or fails validation |
| `caekit.CalculationError` | The calculation leaves the finite binary64 range or cannot allocate its working memory |
| `TypeError` | The argument is not one of the accepted forms |

Both CAEKit exceptions derive from `caekit.CaekitError`.

## Runs that do not converge

A run that stops without convergence is returned, not raised. Read `stop_reason`. Such a
report keeps the traces of the stages that ran and withholds physical fields and balances.

```python
import json
import pathlib

import caekit

model = json.loads(pathlib.Path("examples/elastic_solid.json").read_text(encoding="utf-8"))
model["linear_solver"] = {
    "relative_residual_tolerance": 1e-10,
    "absolute_residual_tolerance": 0.0,
    "max_iterations": 1,
}

report = caekit.solve(model)
assert report["stop_reason"] == "iteration_limit"
assert report["quantities"] == {}
stage = report["convergence"][0]
print(stage["stage"], stage["final_true_residual_l2"], ">", stage["residual_threshold"], stage["residual_unit"])
```

## Threads

`solve` and `check` release the interpreter lock while the Rust code runs. Calls from
several Python threads therefore run in parallel; each solve itself uses one thread.

## Build and test from a checkout

The package lives in `crates/caekit-py` and is built with
[maturin](https://www.maturin.rs). In a virtual environment, from the repository root:

```sh
python -m pip install maturin pytest
maturin develop --release --locked --manifest-path crates/caekit-py/Cargo.toml
python -m pytest crates/caekit-py/tests -q
```

CI builds the wheel, installs it and runs the same tests. They solve every saved model,
compare the analytical references and execute the `python` examples of this page, of the
repository README and of the package README.
