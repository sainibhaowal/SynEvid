import json
import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
VALIDATOR_PATH = ROOT / "scripts" / "validate-schemas.py"
VALIDATOR_SPEC = importlib.util.spec_from_file_location(
    "validate_schemas", VALIDATOR_PATH
)
assert VALIDATOR_SPEC is not None and VALIDATOR_SPEC.loader is not None
validate_schemas = importlib.util.module_from_spec(VALIDATOR_SPEC)
VALIDATOR_SPEC.loader.exec_module(validate_schemas)


def test_phase6_schemas_are_valid_draft_2020_12():
    for name in (
        "analytics-report.schema.json",
        "analytics-bundle.schema.json",
        "runtime-observation-window.schema.json",
    ):
        valid, error = validate_schemas.validate_schema(ROOT / "schemas" / name)
        assert valid, f"{name}: {error}"


def test_phase6_schema_instances_validate():
    pairs = (
        ("analytics-report-sample.json", "analytics-report.schema.json"),
        ("analytics-bundle-sample.json", "analytics-bundle.schema.json"),
        (
            "runtime-observation-window-sample.json",
            "runtime-observation-window.schema.json",
        ),
    )
    for instance_name, schema_name in pairs:
        valid, error = validate_schemas.validate_instance(
            ROOT / "tests" / "testdata" / instance_name,
            ROOT / "schemas" / schema_name,
        )
        assert valid, f"{instance_name}: {error}"


def test_phase6_report_schema_rejects_short_digests(tmp_path):
    instance = json.loads(
        (ROOT / "tests" / "testdata" / "analytics-report-sample.json").read_text()
    )
    instance["report_id"] = "blake3:not-a-64-character-digest"
    path = tmp_path / "invalid-report.json"
    path.write_text(json.dumps(instance))
    valid, _ = validate_schemas.validate_instance(
        path, ROOT / "schemas" / "analytics-report.schema.json"
    )
    assert not valid
