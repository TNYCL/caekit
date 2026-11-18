"""Multiphysics engineering simulation: solve versioned CAEKit models and read run reports.

A model is the JSON document described in docs/model.md of the CAEKit repository. `solve`
and `check` accept it as a mapping, as JSON text (`str`), as UTF-8 JSON bytes, or as a
path object naming a file. They return the report as plain dicts and lists.
"""

from __future__ import annotations

import json
import os
from collections.abc import Mapping
from typing import Any, Union

from caekit._caekit import (
    MODEL_SCHEMA_VERSION,
    RESULT_SCHEMA_VERSION,
    CaekitError,
    CalculationError,
    InvalidModelError,
    __version__,
    check_json,
    solve_json,
)

__all__ = [
    "MODEL_SCHEMA_VERSION",
    "RESULT_SCHEMA_VERSION",
    "CaekitError",
    "CalculationError",
    "InvalidModelError",
    "ModelInput",
    "__version__",
    "check",
    "solve",
]

ModelInput = Union[Mapping[str, Any], str, bytes, bytearray, memoryview, "os.PathLike[str]"]


def _document(model: ModelInput) -> bytes:
    """The model as UTF-8 JSON bytes."""
    if isinstance(model, str):
        return model.encode("utf-8")
    if isinstance(model, (bytes, bytearray, memoryview)):
        return bytes(model)
    if isinstance(model, os.PathLike):
        with open(model, "rb") as file:
            return file.read()
    if isinstance(model, Mapping):
        try:
            return json.dumps(model, allow_nan=False).encode("utf-8")
        except ValueError as error:
            # NaN and infinities have no JSON spelling; circular references end here too.
            raise InvalidModelError(f"model cannot be written as JSON: {error}") from error
    raise TypeError(
        "model must be a mapping, JSON text, UTF-8 JSON bytes or a path object, "
        f"not {type(model).__name__}"
    )


def check(model: ModelInput) -> dict[str, Any]:
    """Validate a model and return its schema version and mesh metrics.

    Raises `InvalidModelError` when the model is not valid JSON, uses an unsupported
    schema version or fails validation; the message names the offending field.
    """
    return json.loads(check_json(_document(model)))


def solve(model: ModelInput) -> dict[str, Any]:
    """Validate and solve a model on the reference double-precision CPU path.

    Returns the run report. A run that stops without convergence is returned, not raised:
    read `report["stop_reason"]` (`"converged"`, `"iteration_limit"` or
    `"numerical_breakdown"`). Such a report keeps the convergence traces and withholds
    the physical fields and balances.

    Raises `InvalidModelError` for an invalid model and `CalculationError` when the
    calculation leaves the finite binary64 range or cannot allocate its working memory.
    """
    return json.loads(solve_json(_document(model)))
