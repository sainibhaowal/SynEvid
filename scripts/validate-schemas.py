#!/usr/bin/env python3
"""
Formal JSON Schema validation for SynEvid public schemas.
Validates that all schema files are valid JSON Schema Draft 2020-12
and that instance documents conform to their schemas.
"""

import json
import sys
from pathlib import Path
from jsonschema import Draft202012Validator, validators
from jsonschema.exceptions import SchemaError, ValidationError
from referencing import Registry, Resource

SCHEMAS_DIR = Path(__file__).parent.parent / "schemas"
TEST_DATA_DIR = Path(__file__).parent.parent / "tests" / "testdata"

def load_schema(schema_path):
    """Load a JSON schema file."""
    with open(schema_path, 'r') as f:
        return json.load(f)

def validate_schema(schema_path):
    """Validate that a schema is a valid JSON Schema Draft 2020-12."""
    try:
        schema = load_schema(schema_path)
        Draft202012Validator.check_schema(schema)
        return True, None
    except SchemaError as e:
        return False, str(e)
    except json.JSONDecodeError as e:
        return False, f"Invalid JSON: {e}"

def validate_instance(instance_path, schema_path):
    """Validate an instance document against its schema."""
    try:
        schema = load_schema(schema_path)
        with open(instance_path, 'r') as f:
            instance = json.load(f)
        
        # Register every local schema by its declared $id so cross-schema
        # references are validated offline and cannot trigger network fetches.
        registry = Registry()
        for local_path in sorted(SCHEMAS_DIR.glob("*.json")):
            local_schema = load_schema(local_path)
            schema_id = local_schema.get("$id")
            if isinstance(schema_id, str) and schema_id:
                registry = registry.with_resource(
                    schema_id, Resource.from_contents(local_schema)
                )
        
        validator = Draft202012Validator(schema, registry=registry)
        errors = list(validator.iter_errors(instance))
        
        if errors:
            error_msgs = []
            for error in errors:
                path = " -> ".join(str(p) for p in error.path)
                error_msgs.append(f"  {path}: {error.message}")
            return False, "\n".join(error_msgs)
        return True, None
    except ValidationError as e:
        return False, str(e)
    except json.JSONDecodeError as e:
        return False, f"Invalid JSON: {e}"
    except Exception as e:  # fail closed on unresolved or invalid references
        return False, f"Schema reference validation failed: {e}"

def main():
    print("=" * 60)
    print("SynEvid Schema Validation")
    print("=" * 60)
    
    # Find all schema files
    schema_files = list(SCHEMAS_DIR.glob("*.json"))
    if not schema_files:
        print("ERROR: No schema files found in schemas/")
        return 1
    
    print(f"Found {len(schema_files)} schema files")
    
    # Validate each schema
    all_valid = True
    for schema_file in sorted(schema_files):
        print(f"\nValidating schema: {schema_file.name}")
        valid, error = validate_schema(schema_file)
        if valid:
            print("  [OK] Schema is valid JSON Schema Draft 2020-12")
        else:
            print(f"  [FAIL] Schema validation failed: {error}")
            all_valid = False
    
    # Validate instance documents if they exist
    print("\n" + "=" * 60)
    print("Instance Document Validation")
    print("=" * 60)
    
    # Map schema names to test data patterns
    schema_to_testdata = {
        "analytics-report.schema.json": "analytics-report-*.json",
        "analytics-bundle.schema.json": "analytics-bundle-*.json",
        "autopsy-result.schema.json": "autopsy-result-*.json",
        "baseline.schema.json": "baseline-*.json",
        "blast-radius.schema.json": "blast-radius-*.json",
        "catalog-report.schema.json": "catalog-report-*.json",
        "cochange-report.schema.json": "cochange-report-*.json",
        "distributed-contract.schema.json": "distributed-contract-*.json",
        "evidence-envelope.schema.json": "evidence-envelope-*.json",
        "invariant.schema.json": "invariant-*.json",
        "lineage.schema.json": "lineage-*.json",
        "runtime-envelope.schema.json": "runtime-envelope-*.json",
        "runtime-observation-window.schema.json": "runtime-observation-window-*.json",
        "telemetry-fusion.schema.json": "telemetry-fusion-*.json",
        "test-selection.schema.json": "test-selection-*.json",
        "temporal.schema.json": "temporal-*.json",
        "topology-report.schema.json": "topology-report-*.json",
    }
    required_instance_schemas = {
        "analytics-report.schema.json",
        "analytics-bundle.schema.json",
        "runtime-observation-window.schema.json",
    }
    
    for schema_name, test_pattern in schema_to_testdata.items():
        schema_path = SCHEMAS_DIR / schema_name
        if not schema_path.exists():
            continue
            
        test_files = list(TEST_DATA_DIR.glob(test_pattern))
        if not test_files:
            print(f"\n{schema_name}: No test data found (pattern: {test_pattern})")
            if schema_name in required_instance_schemas:
                all_valid = False
            continue
            
        print(f"\nValidating instances for {schema_name}:")
        for test_file in test_files:
            print(f"  Testing {test_file.name}...")
            valid, error = validate_instance(test_file, schema_path)
            if valid:
                print(f"    [OK] {test_file.name} conforms to schema")
            else:
                print(f"    [FAIL] {test_file.name} validation failed:")
                for line in error.split('\n'):
                    print(f"      {line}")
                all_valid = False
    
    print("\n" + "=" * 60)
    if all_valid:
        print("[OK] ALL SCHEMAS AND INSTANCES VALID")
        return 0
    else:
        print("[FAIL] VALIDATION FAILED")
        return 1

if __name__ == "__main__":
    sys.exit(main())
