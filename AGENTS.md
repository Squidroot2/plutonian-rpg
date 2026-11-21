# Repository Guidelines

## Project Structure & Module Organization

This Linux desktop RPG uses Rust 2024, Bevy 0.20, and Avian 0.8. `src/main.rs` registers feature plugins. Unit tests live beside their systems, and `src/smoke/` holds desktop scenarios. Keep Blender sources in `art/`, runtime GLBs in `assets/models/`, and asset tooling in `scripts/`.

## Build, Test, and Development Commands

Run from the repository root with Rust 1.97.1 or newer:

- `cargo run --locked` launches the game.
- `cargo build --locked` builds the executable. `cargo check --locked` checks compilation without linking.
- `cargo fmt --check` checks formatting. `cargo fmt` applies it.
- `cargo test --locked` runs Rust tests. `cargo clippy --locked --all-targets` runs lint checks.
- `python3 scripts/check_assets.py` validates GLB structure, resources, skin, and animation names.
- `cargo run --locked -- --smoke-test` exercises the gameplay arena.
- `cargo run --locked -- --smoke-world` exercises the shipped map and Old Relay. Both smoke modes require a desktop and save screenshots in `/tmp/plutonian-*.png`.

## Coding Style & Naming Conventions

Use rustfmt, four-space indentation, `snake_case` functions and modules, and `PascalCase` types. Follow existing Python formatting, name assets descriptively, and keep `Cargo.lock` tracked.

## ECS Design

- Put actor state in components and session-wide state in resources. Query capabilities such as `Health` or `HitReaction`.
- Emit `Hit` from attack systems. Shared systems resolve damage, emit `ConfirmedHit`, and mark zero-health entities `Dead`. Opt in to death animation, collider removal, and corpse expiry through components.
- Separate AI decisions, `FixedUpdate` movement, and presentation. Schedule frame attacks in `CombatSystems::Detect` and reaction systems after `CombatSystems::Resolve`. Gate gameplay by state.
- Bind animation players and equipment sockets when the scene instance becomes ready. Use entity relationships and change detection to synchronize equipment and UI.

## Testing Guidelines

Use `#[test]` with descriptive behavior names. Cover quest progression, premature actions, and duplicates. Test gameplay systems without a renderer. Run the desktop smoke tests for movement, combat, or UI changes.

## Blender Asset Workflow

Export edited `.blend` sources with the README's `--export` command. Running `scripts/assets.py` without `--export` overwrites source art. Align collision geometry separately and preserve animation names.

## Commit & Pull Request Guidelines

Use concise imperative commit subjects. Describe behavior changes, link issues, and report verification in PRs. Include screenshots for visual changes and commit source art with its exports.
