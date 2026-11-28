---
name: blender-assets
description: Work on Blender art for plutonian-rpg — edit or re-export .blend sources in art/, regenerate procedural art, check exported GLBs, or fix animation names and collision alignment. Use when the user asks about Blender models, animations, GLB exports, art sources, or anything in art/, assets/models/, or scripts/assets.py.
---

# Blender Assets

Do Blender/asset work for plutonian-rpg in the current repository.
Two repo docs govern this work — read both before touching art:

- `docs/design/art-style.md` is the source of truth for art direction: shape language, palette, lighting accents, and per-location character.
  This skill covers process and hard constraints; it does not restate or override the style doc.
  If a request conflicts with the style doc, flag the conflict instead of silently deviating.
- `docs/contributing/blender_workflow.md` is the full asset workflow — follow it over any general Blender habit.

## Layout

- Blender sources: `art/*.blend` (editable, checked in; Blender 5.3 was used for them).
- Runtime exports: `assets/models/*.glb` (checked in, so Blender is not needed to play).
- Asset tooling: `scripts/assets.py` is the registry and dispatcher (see below); the builders live in modules beside it — `scripts/village.py` (the outpost village homes, including the Barracks), `scripts/crossing.py`, and `scripts/perimeter.py` — with shared primitives in `scripts/asset_helpers.py` (`material`, `finish`, `box`, `cone`, `ico`, `save_export`, `export`).
  Prefer the helpers over hand-rolling materials or geometry.
- Validation is Rust, not Python: `crates/plutonian-assets`, run as `cargo run --locked -p plutonian-assets` (see Verify).
  There is no `scripts/check_assets.py` anymore.
- Other scripts: `scripts/grounding.py` (foundation-contact check), `scripts/render_title.py` (title-screen render).

## The registry

`ASSET_REGISTRY` at the end of `scripts/assets.py` is the single source of truth for the asset pipeline: each entry declares a flag, its export names, and its builder (or an export-only marker).
Dispatch, the `--export` name list, and rebuild-all are all derived from it, in the registry's declared order — the order is load-bearing (the character rig builds before equipment, which reads its attachment frames).
Run `blender --background -noaudio --factory-startup --python-exit-code 1 --python scripts/assets.py -- --list` to print every flag, whether it is built or export-only, and its export names.
An unknown flag is an error, not a no-op — if a flag you expect is rejected, check `--list` before inventing one.
Adding an asset means adding a registry entry (plus its builder, in the module where its neighbours live); do not wire new assets into dispatch or export lists by hand.

## The one rule that matters most

**Never run `scripts/assets.py` without a flag.**
Bare `scripts/assets.py` rebuilds all source art from the procedural designs and **overwrites the editable `.blend` sources**.
Re-exporting hand-edited sources always takes `--export`:

```sh
blender --background -noaudio --factory-startup --python-exit-code 1 --python scripts/assets.py -- --export
```

Append a per-asset flag to export just that source, e.g. `-- --export --hushcap`, `-- --export --relay`, `-- --export --south-basin`, `-- --export --crossing`, `-- --export --wagon`, `-- --export --outpost`, `-- --export --shortsword`.
Rebuild flags (which regenerate sources) are a different, deliberate act.
They are the entries `--list` reports as built rather than export-only — do not work from a remembered list of them, in this skill or anywhere else; the registry is the only maintained list, and any copied list will drift.
Each rebuild overwrites the sources it names.
Say which mode you are in (re-export vs. rebuild) before running anything, and never rebuild when the user asked for a re-export.

## The seam with Rust — check it before exporting

Clip and model names are a contract with the game code, not just Blender labels.
Enemy clips are declared in the `ENEMY_MODELS` registry in `src/enemy.rs` (each `EnemyModel` carries its `EnemyClip` names and durations; per-enemy files such as `src/worm.rs` hold the behaviour).
Before exporting an enemy or the character rig, check those declarations (e.g. `WormIdle`, `WormBite`, `WormDeath`) and the locomotion/carry sets the asset validator enforces for `plutonian` (`Idle`, `Walk`, `Run`, `Jump`, directional walks, `SwordMainSlash`, `SwordOffSlash`, `SpearThrust`, `PlutonianDeath`).
Keep animation names unchanged unless the issue explicitly asks to rename one — and if it does, update the Rust side in the same change.
Equipment geometry must fit the per-item bounds declared on the item definitions in the catalog (`src/content/items.rs`); the asset validator verifies that.

## Authoring conventions

- One Blender unit equals one meter; author with Z up and the character facing -Y (glTF export converts to Bevy's Y-up, character facing +Z).
- Keep scale applied. Materials use Principled BSDF base color, roughness, metallic, and emission. Animation is exported as sampled actions.
- Equipment sources store geometry in their attachment frame: armor `spine`, helmets `head`, polearms `spear_grip`, shortswords matching wrist sockets. `--export` produces the bone-local `_worn.glb` plus a centered base-origin pickup export from the same source. Recheck fit after changing the character rig.
- Slash contact/recovery frames are load-bearing: sword slashes contact frame 5, recover frame 12; spear thrust contacts frame 6, recovers frame 16 (24 fps). Do not retime them casually.

## Collision

- Keep visible environment edits and `collision.blend` aligned. `--export` does **not** regenerate collision from edited outpost sources.
- Add collision meshes to `collision.blend` for solid additions. Trail pillar (`Waystone`) bodies belong in the hidden collision scene.
- To repair placements while preserving meshes and materials, use `-- --ground` instead of `-- --export`; it moves attached details and matching collision objects together.
- To check foundation contact without changing sources: `blender --background -noaudio --factory-startup --python-exit-code 1 --python scripts/grounding.py`.

## Style gates

Check finished work against `docs/design/art-style.md` before reporting done.
The quick version of its checks:

- Silhouette reads at normal third-person gameplay distance, against its surroundings.
- Forms are broad planes and simple volumes with deliberate facets; no fine surface noise or photorealistic ornament.
- Palette stays in the cold blues, blue-gray, near-black, and muted stone family.
  Warm amber is reserved for inhabited spaces and machinery; cyan is reserved for waystones and luminous features.
  Accents stay concentrated so they keep their meaning.
- Materials are mostly matte with readable value changes across facets.
  Wear comes from construction and selective color variation, not scratch textures everywhere.
- No ordinary wooden construction; equipment reads as plates, frames, fasteners, fabric-like coverings, stone, or metal.
- Creatures read as threats before small details; characters and gear stay legible from behind and in motion.
- Colors are judged under the actual in-game lighting, not only in a neutral modeling viewport.
- New work is compared in its intended scene against the references named in the style doc: the outpost, the Old Relay, the Plutonian, the crossing wagon, and nearby creatures.

## Verify

After every export, run the repo's asset check (no Blender needed):

```sh
cargo run --locked -p plutonian-assets
```

It validates GLB structure, resources, skin, animation names and durations, and equipment bounds, and asserts every exported GLB has a matching `.blend` source in `art/`.
If it fails, fix the cause and rerun — do not hand the user an export that fails it.
For in-engine equipment screenshots,
use `cargo run --locked --features smoke --bin plutonian-art-preview -- <recipe.ron> <shot>`.
It uses the game lighting and can show worn GLBs on the player rig or Barracks rack before catalog or gameplay integration.
Follow the [preview instructions and musket example](../../../docs/contributing/blender_workflow.md#in-engine-equipment-previews) for before/after shots.
If the change also touches Rust, run the repo's Rust verification from `AGENTS.md` (`cargo fmt --check`, `cargo clippy --locked --all-targets`, `cargo test --locked`) as well.
Do not claim a check passed that did not run.

## Title screen

To refresh the title render after editing the scenery: `blender --background -noaudio art/outpost.blend --python scripts/render_title.py`.

## Out of scope

- Gameplay, tuning, or save-format changes. This skill is art only; a task that also needs code changes says so explicitly.
- Live MCP connections: the scripts run in a separate background Blender process; none is required.
