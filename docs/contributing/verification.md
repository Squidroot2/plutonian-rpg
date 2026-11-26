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
See the [repository guidelines](../../AGENTS.md) for development and verification standards.

Run the asset checker from the repository root.
It validates model references, Blender source coverage, embedded resources, collision companions,
pickup dimensions, and the animated rigs without starting the game.
Enemy clips and durations come from the runtime model registry.
New area collision companions pair by the `_collision.glb` suffix.
Declare bodies that must exist in both halves of a pair in `src/asset_check/expectations.rs`.

Run one smoke scenario at a time on an idle graphical desktop without supplying mouse or keyboard input.
Add `--windowed` to a smoke command to run it in a window.
Each scenario uses a temporary save slot.

- `--smoke-test` starts a standalone arena with nearby materials, equipment, a practice dummy, and the keeper and core.
  It checks tab switching, live physics while the menu is open, inventory quantities, equipped rows, the drop prompt, both polearms' hits and misses, and shortsword attachment and hits, including simultaneous strikes.
  It also checks a worm pursuit, dodge, bite, and defeat, the Character tab's XP total, armor swaps and attachment during movement, the 640×704 player-menu layout, wall collision, pause/resume, and fall recovery.
  After corpse looting, it checks that an ineligible equip shows a dismissible Strength warning and leaves the gear in the bag.
  The short beacon interaction verifies that the core is protected in the bag, consumed on turn-in, and lights the beacon.
  The run prints `SMOKE PASS` and its duration, or fails within 60 seconds with the current phase.
- `--smoke-world` loads the shipped outpost, checks transfers in all three tent chests, and walks out of and back into the tent to verify stored contents.
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
