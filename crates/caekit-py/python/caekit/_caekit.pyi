"""Type stubs of the compiled extension module."""

__version__: str
MODEL_SCHEMA_VERSION: int
RESULT_SCHEMA_VERSION: int

class CaekitError(Exception):
    """Base class of all CAEKit errors."""

class InvalidModelError(CaekitError):
    """The model is not valid JSON, uses an unsupported schema version, or fails validation."""

class CalculationError(CaekitError):
    """The calculation left the finite binary64 range or could not allocate its working memory."""

def check_json(model: bytes) -> str:
    """Validates the model document and returns the validation report as JSON text."""

def solve_json(model: bytes) -> str:
    """Validates and solves the model document and returns the run report as JSON text."""
