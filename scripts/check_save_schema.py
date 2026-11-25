"""Validate committed golden save fixtures against their JSON Schema."""

import argparse
import json
import subprocess
from pathlib import Path

from jsonschema.validators import validator_for


ROOT = Path(__file__).resolve().parents[1]
SCHEMA_PATH = ROOT / "golden-saves" / "save.schema.json"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()

    schema = json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))
    validator_type = validator_for(schema)
    validator_type.check_schema(schema)
    validator = validator_type(schema)
    fixtures = sorted(
        path
        for path in (ROOT / "golden-saves").glob("*/*.json")
    )

    for fixture in fixtures:
        if fixture.parent.name != fixture.stem:
            raise ValueError(f"{fixture.name} must be stored in its matching scenario directory")
        document = json.loads(fixture.read_text(encoding="utf-8"))
        errors = sorted(validator.iter_errors(document), key=lambda error: list(error.path))
        if errors:
            details = "\n".join(f"  {error.message}" for error in errors)
            raise ValueError(f"{fixture.name} does not match save.schema.json:\n{details}")
        print(f"Schema validation passed for {fixture.name}")

    if fixtures:
        document = json.loads(fixtures[0].read_text(encoding="utf-8"))
        misspelled = {**document, "format_vesion": document["format_version"]}
        if validator.is_valid(misspelled):
            raise ValueError("schema accepted a misspelled top-level property")

        wrong_type = {**document, "format_version": str(document["format_version"])}
        if validator.is_valid(wrong_type):
            raise ValueError("schema accepted a string format_version")

        nemi_with_unknown_field = {
            **document,
            "nemi": {**document["nemi"], "unexpected": True},
        }
        if validator.is_valid(nemi_with_unknown_field):
            raise ValueError("schema accepted an unknown nested actor property")
        progress_with_unknown_field = {
            **document,
            "progression": {
                **document["progression"],
                "progress": {**document["progression"]["progress"], "unexpected": True},
            },
        }
        if validator.is_valid(progress_with_unknown_field):
            raise ValueError("schema accepted an unknown nested progression property")
        print("Schema rejects unknown top-level and nested properties and incorrect field types")

        saved_game = ROOT / "tests" / "fixtures" / "inspector-v12.save"
        inspected = subprocess.run(
            [args.binary, "inspect", "--json", saved_game],
            check=True,
            capture_output=True,
            text=True,
        )
        validator.validate(json.loads(inspected.stdout))
        print("Schema validation passed for inspect --json output")
    else:
        print("No golden save fixtures are present")


if __name__ == "__main__":
    main()
