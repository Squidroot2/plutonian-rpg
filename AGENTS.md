# Repository Guidelines

## Project Structure & Module Organization

This Linux desktop RPG uses Rust 2024, Bevy 0.20, and Avian 0.8. `src/main.rs` assembles plugins; `loading.rs`, `world.rs`, `player.rs`, `camera.rs`, `quest.rs`, and `ui.rs` own their respective systems. Keep movement physics in `FixedUpdate` and respect the loading, playing, paused, and failed states.

Unit tests live beside quest logic in `src/quest.rs`. `src/smoke.rs` provides the opt-in desktop playthrough. Editable Blender sources belong in `art/`; runtime GLBs belong in `assets/models/`. Asset generation and validation scripts live in `scripts/`.

## Build, Test, and Development Commands

Run from the repository root with Rust 1.97.1 or newer:

- `cargo run --locked`: launch the game with checked-in assets.
- `cargo build --locked`: build the executable.
- `cargo check --locked`: check compilation without linking.
- `cargo fmt --check`: verify Rust formatting; use `cargo fmt` to apply it.
- `cargo test --locked`: run Rust unit tests.
- `python3 scripts/check_assets.py`: validate GLB structure, embedded resources, skin, and animation names.
- `cargo run --locked -- --smoke-test`: run the automated quest on a graphical desktop; expect `SMOKE PASS` and screenshots in `/tmp/plutonian-*.png`.

## Coding Style & Naming Conventions

Use rustfmt, four-space indentation, `snake_case` functions/modules, and `PascalCase` types/plugins. Prefer focused Bevy systems, typed components/resources, and explicit quest transitions. Follow existing Python formatting and use descriptive asset names. Keep `Cargo.lock` tracked; avoid unrelated dependency upgrades.

## Testing Guidelines

Use Rust's built-in `#[test]` framework with descriptive behavior names. Cover valid progression, premature interactions, and duplicate actions when changing quests. No numeric coverage threshold is configured. For movement, camera, or UI changes, run the desktop smoke test and manually check affected interactions; distinguish compilation checks from runtime verification.

## Blender Asset Workflow

Preserve `.blend` sources and update corresponding GLBs. Use the README's `--export` command to export edited sources. Running `scripts/assets.py` without `--export` overwrites source art. Keep collision geometry aligned separately and retain `Idle`, `Walk`, `Run`, and `Jump` animation names.

## Commit & Pull Request Guidelines

There are no commits yet, so no established commit-message convention exists. Use concise imperative subjects describing one coherent change. PRs should explain behavior changes, reference relevant issues, report checks and limitations, and include screenshots for visual changes. Include updated source art and exports together.
