"""Generate or round-trip-check the current-format golden save fixtures."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "golden-saves"


def fixture_data(path):
    with path.open(encoding="utf-8") as source:
        data = json.load(source)
    name = data.get("player_name") if isinstance(data, dict) else None
    if not isinstance(name, str) or not (1 <= len(name) <= 16):
        raise ValueError(f"{path.name} has an invalid player_name")
    if name != name.strip() or not all(char.isalnum() or char == " " for char in name):
        raise ValueError(f"{path.name} has an invalid player_name")
    return data


def regenerate(directory, fixtures, command, check):
    environment = os.environ.copy()
    environment.setdefault("BEVY_ASSET_ROOT", str(ROOT))
    for fixture in fixtures:
        data = fixture_data(fixture)
        scenario = directory / fixture.stem
        if scenario.is_symlink():
            raise ValueError(f"Scenario directory is a symlink: {scenario}")
        scenario.mkdir(exist_ok=True)
        for old_save in scenario.glob("*.save"):
            if old_save.is_symlink() or old_save.is_file():
                old_save.unlink()
        output = scenario / f"{data['player_name']}-{int(time.time())}.save"
        subprocess.run(
            [*command, "create", fixture, "--output", output],
            check=True, cwd=ROOT, env=environment,
        )
        if check:
            inspected = subprocess.check_output(
                [*command, "inspect", "--json", output], cwd=ROOT, env=environment,
            )
            if json.loads(inspected) != data:
                raise ValueError(f"{fixture.name} changed during the save round trip")
            print(f"Round trip passed for {fixture.name}")
        else:
            print(f"Generated {output.relative_to(ROOT)}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Round-trip in a temporary directory")
    parser.add_argument("--binary", type=Path, help="Use an already built plutonian-save")
    args = parser.parse_args()
    command = [args.binary.resolve()] if args.binary else [
        "cargo", "run", "--locked", "--quiet", "--bin", "plutonian-save", "--",
    ]
    fixtures = sorted(FIXTURES.glob("*.json"))
    try:
        if not fixtures:
            raise ValueError(f"No JSON fixtures in {FIXTURES}")
        if args.check:
            with tempfile.TemporaryDirectory(prefix="plutonian-golden-") as temp:
                regenerate(Path(temp), fixtures, command, check=True)
        else:
            regenerate(FIXTURES, fixtures, command, check=False)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
