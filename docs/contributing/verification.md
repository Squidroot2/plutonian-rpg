# Verification

```sh
cargo fmt --check
cargo check --locked
cargo clippy --locked --all-targets
cargo test --locked
cargo run --locked --features smoke -- --smoke-test
cargo run --locked --features smoke -- --smoke-world
cargo run --locked --features smoke -- --smoke-nemi
cargo run --locked --features smoke -- --smoke-cave
cargo run --locked --features smoke -- --smoke-crossing
cargo run --locked -p plutonian-assets
```

Clippy enforces a 150-line function limit, including smoke drivers.
GitHub Actions builds, tests, and runs Clippy for pushes and pull requests to `master`.
The separate desktop smoke workflow runs nightly at 07:00 UTC, when a release
is published, and on manual dispatch. It builds once and runs all five
scenarios sequentially under Xvfb with lavapipe: arena, world, Nemi, cave, and
crossing. The run summary reports each result.
CI passes `--no-screenshots`, so workflow runs produce no
screenshots. To run the same scenarios locally with screenshots enabled, use
`scripts/smoke.sh test`, `scripts/smoke.sh world`, `scripts/smoke.sh nemi`,
`scripts/smoke.sh cave`, and `scripts/smoke.sh crossing`. The workflow keeps
the spike's Ubuntu 24.04 dependency set, Vulkan llvmpipe probe,
`WGPU_BACKEND=vulkan`, reduced Cargo debug information, and disabled
incremental compilation. Smoke scenarios use fixed 20 ms virtual-time steps.
See the [repository guidelines](../../AGENTS.md) for development and verification standards.

## Coverage

Install `cargo-llvm-cov` with `cargo install cargo-llvm-cov`
and add the required Rust component with `rustup component add llvm-tools-preview`.
Use a separate target directory to keep instrumented artifacts out of the everyday `target/`:

```sh
export CARGO_TARGET_DIR=/tmp/plutonian-coverage
cargo llvm-cov --workspace
cargo llvm-cov --workspace --html
```

The first command prints a per-file summary;
the second generates a browsable report at `/tmp/plutonian-coverage/llvm-cov/html/index.html`.
On pushes to `master`,
The Coverage workflow runs on pushes to `master` or by manual dispatch.
It publishes the line coverage and per-file report in the run summary
and uploads LCOV and HTML reports as the `coverage-report` artifact.
Coverage is informational and has no threshold gate.

## Headless and desktop checks

Write new scenario logic checks headless-first in the crate's test modules.
Use `src/headless.rs` to boot the needed game plugins without a window or renderer,
and advance the app with its fixed 20 ms step and bounded condition helper.
The harness loads shipped GLB world assets and builds Avian colliders on the CPU.
It omits audio playback and does not exercise rendering or real window input.
Reserve desktop and CI smoke scenarios for checks that need pixels,
such as rendered geometry, real picking, and window or focus behavior.

Run the asset checker from the repository root.
It validates model references, Blender source coverage, embedded resources, collision companions,
pickup dimensions, and the animated rigs without starting the game.
Enemy clips and durations come from the runtime model registry.
New area collision companions pair by the `_collision.glb` suffix.
Declare bodies that must exist in both halves of a pair in `src/asset_check/expectations.rs`.

Run one smoke scenario at a time on an idle graphical desktop without supplying mouse or keyboard input.
Add `--windowed` to a smoke command to run it in a window.
Add `--no-screenshots` to a smoke command to skip PNG captures and their wait gates.
The scheduled desktop smoke workflow uses this flag.
Scenarios are run through `scripts/smoke.sh <scenario>`, which checks for GPU devices before launching.
Agent sandboxes (Codex's, for example) present a synthetic `/dev` without `/dev/dri`, so a sandboxed run fails with "could not detect a GPU" even on a host with a working GPU.
The script must run outside the sandbox (the execpolicy rules allow it in exactly this form), and that error from inside a sandbox must never be reported as the host being unable to run the scenarios.
Each scenario uses a temporary save slot.
Smoke scenarios advance virtual time in 20 ms steps so input and waits use the same pacing on desktop and in CI.
The virtual time budget counts these steps.
The wall clock watchdog defaults to 10 minutes.
CI sets `PLUTONIAN_SMOKE_WALL_TIMEOUT_SECS` to give world, cave, and crossing 30, 40, and 60 minutes respectively for software rendering.
Their workflow steps allow five extra minutes so the watchdog can report the current phase before GitHub cancels the step.
A launch without a smoke scenario flag uses the real-time clock.
Reaction checks in smoke drivers must be tick-based: arm the check when triggering the action,
then evaluate it on a later tick after the responsible system can react.
Plain waits for elapsed time remain appropriate when no reaction deadline is asserted.

- `--smoke-test` starts a standalone arena with nearby materials, equipment, a practice dummy, and the keeper and core.
  It checks tab switching, live physics while the menu is open, inventory quantities, equipped rows, the drop prompt, both polearms' hits and misses, and shortsword attachment and hits, including simultaneous strikes.
  It also checks a worm pursuit, dodge, bite, and defeat, the Character tab's XP total, armor swaps and attachment during movement, the 640×704 player-menu layout, wall collision, pause/resume, and fall recovery.
  After the equipment checks, it verifies that the unarmed player faces the direction of travel
  when moving right or backward at walking pace.
  After corpse looting, it checks that an ineligible equip shows a dismissible Strength warning and leaves the gear in the bag.
  The short beacon interaction verifies that the core is protected in the bag, consumed on turn-in, and lights the beacon.
  The run prints `SMOKE PASS` and its duration, or reports the current phase if it reaches 60 virtual seconds.
- `--smoke-world` loads the shipped outpost, checks transfers in all three tent chests, and walks out of and back into the tent to verify stored contents.
  It also checks the Barracks weapon rack: non-weapon and full-rack refusals, two displayed weapons, withdrawal removing the model, and bed-save restoration of the racked weapons.
  It checks grounded worm spawns, traversable routes, and pillar collision from four directions.
  It sweeps the perimeter with the player capsule,
  then sprints and jumps into the sealed gate to verify the player stays on the shelf.
  It also checks hatch unlocking, the relay encounters, roach loot and XP, archive discovery, return and reentry, fall recovery, and combat defeat with and without a bed save.
  In the archive it checks projectile cover, reads the log with the Hushcap alive, and uses a spear to defeat and loot it.
  It verifies journal reward text, completion XP for The Last Light and Old Relay, and restoration of saved XP after combat defeat.
  After leaving Old Relay it also reports the archive Hushcap to Nemi and checks the new quest's journal counters.
  It saves at the cot, provokes the guards with a spear hit on Nemi, and verifies that defeat restores the checkpoint.
  It prints `WORLD SMOKE PASS` on success.
- `--smoke-nemi` isolates Nemi from the guards and checks spear damage, fleeing and returning, the failed quest entry, Nemi’s collapse and XP reward, and XP for opening the relay by completing Relight the Beacon.
  It prints `NEMI SMOKE PASS` on success.
- `--smoke-cave` walks from the starting tent to Hushcap Caverns and back to the outpost.
  It probes the hidden entrance, both middle-chamber passages, encounter grounding, and projectile cover.
  It also observes Hushcap spit and collects Mycelium through the corpse loot panel.
  It prints `CAVE SMOKE PASS` on success.
- `--smoke-crossing` checks the Listening Flats perimeter, fifth-kill gate trigger, caravan arrival, Tovan’s dialogue, the outer trail loop, and bed-save rollback.
  It prints `CROSSING SMOKE PASS` on success.

The checks save screenshots in `/tmp/plutonian-*.png`, including the arena, outpost, inventory, beacon, and weapon and armor poses.

Manual acceptance: walk and sprint, jump and land, traverse the trail rise, push against habitat/cavern walls, orbit the camera near obstructions, try interacting out of range or through a wall, read/dismiss each dialogue, collect the core once, return to Nemi, confirm the lit beacon, pause/resume using keys and buttons, fall off the ice shelf, pick up materials on the trail and in the cavern, use the inventory quantity field and buttons, drop and recollect stacks near slopes and walls, equip and swap both gear sets, unequip/drop/recollect gear, inspect its fit during animation, and choose **Start Game** after relaunching to confirm fresh quest, inventory, and equipment state.
Automated checks cannot establish how comfortable the movement or composition feels.
