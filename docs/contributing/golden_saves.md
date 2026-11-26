# Working with golden saves

Golden saves let you start testing from a known game state.
Each scenario has a tracked JSON fixture at `golden-saves/<scenario>/<scenario>.json`.
The generator writes game-readable `.save` files into that same directory.
Git ignores the generated `.save` files.

Run the commands below from the repository root.

## Load an existing scenario

```sh
python3 scripts/golden_saves.py
cargo run --locked -- --save-dir golden-saves/after-old-relay
```

Choose **Load Game** and select the generated save.
The `after-old-relay` scenario starts from a bed save made after completing the Old Relay quest.
The game uses the scenario directory for listing, loading, saving, and deleting saves.
Rerun the generator to reset the scenario after testing.
It clears existing `.save` files in every generated scenario directory before recreating its fixture,
including any extra bed saves made during testing.

## Capture a new fixture

1. Launch the game with a separate directory for the capture.
   This keeps the capture available when you regenerate the scenarios.

   ```sh
   capture_dir=$(mktemp -d)
   cargo run --locked -- --save-dir "$capture_dir"
   ```

2. Play to the state you want to test,
   save at the bed in the starting tent,
   and quit.

3. Export the capture to JSON.
   Passing the capture directory selects its newest `.save` by modification time.
   Pass the individual save path if you want an earlier capture.

   ```sh
   cargo run --locked --bin plutonian-save -- inspect --json "$capture_dir" > "$capture_dir/fixture.json" &&
       mkdir -p golden-saves/before-caverns &&
       mv "$capture_dir/fixture.json" golden-saves/before-caverns/before-caverns.json
   ```

4. Regenerate the saves,
   launch with `--save-dir golden-saves/before-caverns`,
   and verify the scenario opens at the expected state.
   Commit the JSON fixture.

## Verify or debug a fixture

Run the same round-trip check as CI.

```sh
python3 scripts/golden_saves.py --check
```

This builds a save from each fixture in a temporary directory,
inspects it,
and compares the parsed JSON with the fixture.
To isolate one fixture,
run the two commands directly with an explicit output path.

```sh
check_dir=$(mktemp -d)
cargo run --locked --bin plutonian-save -- create golden-saves/after-old-relay/after-old-relay.json --output "$check_dir/scenario.save"
cargo run --locked --bin plutonian-save -- inspect --json "$check_dir/scenario.save"
```

`create` checks the current format version,
gameplay catalogs,
and saved state before writing.
When a schema or catalog change breaks a fixture,
update the fixture for the current game or recapture it.
Keep positions and rotations at the precision emitted by `inspect --json`
and preserve the order of arrays.

See the [save design](../design/saves.md) for snapshot contents and the save and load lifecycle.
