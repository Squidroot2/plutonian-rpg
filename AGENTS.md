# Repository Guidelines

## Project Structure & Module Organization

This Linux desktop RPG uses Rust 2024, Bevy 0.20, and Avian 0.8. `src/main.rs` registers feature plugins.
  Unit tests live beside their systems, and `src/smoke/` holds desktop scenarios.
  Keep Blender sources in `art/`, runtime GLBs in `assets/models/`, and asset tooling in `scripts/`.

## Build, Test, and Development Commands

Run these commands from the repository root with Rust 1.97.1 or newer.

- `cargo run --locked` launches the game.
- `cargo build --locked` builds the executable. `cargo check --locked` checks compilation without linking.
- `cargo fmt --check` checks formatting. `cargo fmt` applies it.
- `cargo test --locked` runs Rust tests. `cargo clippy --locked --all-targets` runs lint checks.
- `python3 scripts/check_assets.py` validates GLB structure, resources, skin, and animation names.
- `cargo run --locked -- --smoke-test` exercises the gameplay arena.
- `cargo run --locked -- --smoke-world` exercises the shipped map, tent storage, Old Relay, and bed-save rollback.
- `cargo run --locked -- --smoke-nemi` exercises Nemi's combat reactions and the solo quest route.

Run `cargo test --locked` after every change.
  Before handing off Rust changes, run `cargo fmt --check` and `cargo clippy --locked --all-targets`.
  Fix any failures and rerun the relevant checks.

## Coding Style & Naming Conventions

Use rustfmt, four-space indentation, `snake_case` functions and modules, and `PascalCase` types.
  Follow existing Python formatting, name assets descriptively, and keep `Cargo.lock` tracked.

- Keep the crate's Clippy denies in force. `clippy::too_many_lines` uses the 150-line threshold in `clippy.toml`, including for desktop smoke drivers.
  Split long functions at a responsibility boundary.
  Give smoke assertions and state transitions a named helper for the phase they exercise.
- Look for an existing helper, component, bundle, or system before adding a second implementation.
  Share behavior where the rules and lifecycle match, such as scene readiness, terrain probes, and modal layout.
  Keep feature-specific decisions with the feature that owns them.
- Name gameplay tuning values and thresholds with constants or existing configuration fields.
  Include units in names when they clarify a value, such as seconds, meters, or animation frames.
  Keep related tuning values together.
- Use `///` comments where callers need to understand a contract, invariant, or lifecycle.
  Explain why ordering, thresholds, or relationships matter when the code alone does not make that clear.
  Handle expected failures explicitly and use `expect` to describe a violated invariant.
- Preserve gameplay behavior, tuning values, and save compatibility during refactors.
  Preserve frame and schedule ordering when extracting systems or helpers.

## ECS Design

- Put state on the entity that owns it, including per-actor attack, movement, and playback state.
  Use resources for session-wide state and shared configuration.
  Query capabilities such as `Health` or `HitReaction` and keep species-specific tuning in enemy definitions.
- Give each system a focused responsibility and query only the data it needs.
  Split work into systems when the stages have clear data dependencies.
  Use a helper for steps that need to run together or share a single query pass.
- Emit `Hit` from attack systems.
  Shared systems resolve damage, emit `ConfirmedHit`, and mark zero-health entities `Dead`.
  Opt in to death animation, collider removal, and corpse expiry through components.
- Separate AI decisions, `FixedUpdate` movement, and presentation.
  Schedule frame attacks in `CombatSystems::Detect` and reaction systems after `CombatSystems::Resolve`.
  Gate gameplay by state and make dependencies between systems explicit in the schedule.
- Bind animation players and equipment sockets when the scene instance becomes ready.
  Resolve playback through the owning entity.
  Use change detection to synchronize equipment and UI, and skip component writes when the value is already current.

## Documentation

Design documents are stored in `./docs/design`.
  Design documents should include the high level decisions, the reasoning behind those decisions, and how those features are implemented 

Documentation for players is put in markdown files inside of `./docs/guide`

Instruction for developers is put in markdown files inside of `./docs/contributing`

Make sure all documentation is kept up to date as changes are made.

Use semantic line breaks (one clause per line) when writing markdown files

## Testing Guidelines

Use `#[test]` with descriptive behavior names.
  Cover quest progression, premature actions, and duplicates.
  Test gameplay systems without a renderer.

Run the relevant desktop smoke scenarios for movement, combat, UI, loading, and changes to the smoke drivers themselves.
  They require a desktop and save screenshots in `/tmp/plutonian-*.png`.
  For frame-driven checks, separate input, state assertions, and screenshot capture when they depend on different frames.
  Report which scenarios passed, which needed retries, and any that could not run.

## Blender Asset Workflow

Export edited `.blend` sources with the README's `--export` command.
  Running `scripts/assets.py` without `--export` overwrites source art.
  Align collision geometry separately and preserve animation names.

