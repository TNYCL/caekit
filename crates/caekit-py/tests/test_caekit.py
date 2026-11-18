"""Tests of the installed `caekit` wheel against the saved reference models."""

import json
import pathlib

import pytest

import caekit

REPO = pathlib.Path(__file__).resolve().parents[3]
EXAMPLES = sorted((REPO / "examples").glob("*.json"))


def example(name: str) -> pathlib.Path:
    return REPO / "examples" / f"{name}.json"


def test_examples_are_present():
    assert len(EXAMPLES) >= 7


@pytest.mark.parametrize("path", EXAMPLES, ids=lambda path: path.stem)
def test_reference_model_converges(path):
    model = json.loads(path.read_text(encoding="utf-8"))
    check = caekit.check(path)
    assert check["model_schema_version"] == caekit.MODEL_SCHEMA_VERSION
    assert check["meshes"]

    report = caekit.solve(path)
    assert report["schema_version"] == caekit.RESULT_SCHEMA_VERSION
    assert report["model_name"] == model["name"]
    assert report["stop_reason"] == "converged"
    assert report["execution"]["backend"] == "native_cpu"
    assert report["execution"]["solver_version"] == caekit.__version__
    assert set(report["quantities"]) == set(model["outputs"])
    assert report["meshes"] == check["meshes"]
    for stage in report["convergence"]:
        assert stage["stop_reason"] == "converged"
        assert stage["final_true_residual_l2"] <= stage["residual_threshold"]


def test_analytical_references():
    bar = caekit.solve(example("axial_bar"))
    tip = bar["quantities"]["end_displacement"]["values"]["tip"]
    assert tip == pytest.approx(5.0e-5, abs=1e-13)

    heat = caekit.solve(example("heat_conduction"))
    rates = heat["quantities"]["boundary_heat_rate"]["values"]
    assert rates["x0"] == pytest.approx(200.0, abs=1e-8)
    assert rates["x1"] == pytest.approx(-200.0, abs=1e-8)

    layers = caekit.solve(example("multimaterial_heat_conduction"))
    (transfer,) = layers["quantities"]["interface_heat_rate"]["values"]
    assert (transfer["from_region"], transfer["to_region"]) == ("left", "right")
    assert transfer["value"] == pytest.approx(10.0, abs=1e-8)

    heated = caekit.solve(example("thermoelastic_solid"))
    assert heated["quantities"]["elastic_energy"]["value"] == pytest.approx(3.6, abs=1e-8)


def test_every_input_form_gives_the_same_report():
    path = example("laminar_channel")
    text = path.read_text(encoding="utf-8")
    forms = [path, text, text.encode("utf-8"), bytearray(text.encode("utf-8")), json.loads(text)]
    reports = [caekit.solve(form) for form in forms]
    for report in reports[1:]:
        assert report == reports[0]
    # The digest covers the typed model, so key order and indentation do not change it.
    reordered = dict(reversed(list(json.loads(text).items())))
    assert caekit.solve(reordered)["model_sha256"] == reports[0]["model_sha256"]


def test_invalid_models_raise_invalid_model_error():
    model = json.loads(example("axial_bar").read_text(encoding="utf-8"))

    with pytest.raises(caekit.InvalidModelError, match="unsupported model schema version 2"):
        caekit.solve({**model, "schema_version": 2})
    with pytest.raises(caekit.InvalidModelError, match="unknown field"):
        caekit.check({**model, "surprise": 1})
    with pytest.raises(caekit.InvalidModelError, match="outputs"):
        caekit.check({**model, "outputs": []})
    with pytest.raises(caekit.InvalidModelError, match="invalid model JSON"):
        caekit.solve("{")
    with pytest.raises(caekit.InvalidModelError, match="cannot be written as JSON"):
        caekit.solve({**model, "name": float("nan")})
    assert issubclass(caekit.InvalidModelError, caekit.CaekitError)
    assert issubclass(caekit.CalculationError, caekit.CaekitError)


def test_unsupported_argument_types_raise_type_error():
    for value in (None, 1, 1.5, ["schema_version"]):
        with pytest.raises(TypeError, match="model must be"):
            caekit.solve(value)


def test_nonconverged_run_is_reported_not_raised():
    model = json.loads(example("elastic_solid").read_text(encoding="utf-8"))
    model["linear_solver"] = {
        "relative_residual_tolerance": 1e-10,
        "absolute_residual_tolerance": 0.0,
        "max_iterations": 1,
    }
    report = caekit.solve(model)
    assert report["stop_reason"] == "iteration_limit"
    assert report["convergence"][0]["iterations"] == 1
    assert report["quantities"] == {}
    assert report["balances"] == []
