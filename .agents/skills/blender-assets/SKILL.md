---
name: blender-assets
description: Work on Blender art for plutonian-rpg — edit or re-export .blend sources in art/, regenerate procedural art, check exported GLBs, or fix animation names and collision alignment. Use when the user asks about Blender models, animations, GLB exports, art sources, or anything in art/, assets/models/, or scripts/assets.py.
---

# Blender Assets

Do Blender/asset work for plutonian-rpg in the current repository.
Two repo docs govern this work — read both before touching art:

- `../../../docs/design/art-style.md` is the source of truth for art direction: shape language, palette, lighting accents, and per-location character.
  This skill covers process and hard constraints; it does not restate or override the style doc.
  If a request conflicts with the style doc, flag the conflict instead of silently deviating.
- `../../../docs/contributing/blender_workflow.md` is the full asset workflow — follow it over any general Blender habit.

## Layout

- Blender sources: `art/*.blend` (editable, checked in; Blender 5.3 was used for them).
- Runtime exports: `assets/models/*.glb` (checked in, so Blender is not needed to play).
- Asset tooling: `../../../scripts/assets.py` (generation + export), `scripts/check_assets.py` (GLB validation), `scripts/grounding.py` (foundation-contact check), `scripts/render_title.py` (title-screen render).

## The one rule that matters most

**Never run `../../../scripts/assets.py` without a flag.**
Bare `../../../scripts/assets.py` rebuilds all source art from the procedural designs and **overwrites the editable `.blend` sources**.
Re-exporting hand-edited sources always takes `--export`:

```sh
blender --background -noaudio --factory-startup --python-exit-code 1 --python scripts/assets.py -- --export
```

Append a per-asset flag to export just that source, e.g. `-- --export --hushcap`, `-- --export --relay`, `-- --export --south-basin`, `-- --export --crossing`, `-- --export --wagon`, `-- --export --outpost`, `-- --export --shortsword`.
Rebuild flags (which regenerate sources) are a different, deliberate act — `-- --build-outpost`, `-- --build-crossing`, `-- --build-wagon`, `-- --build-perimeter`, `-- --worm`, `-- --roach`, `-- --items`, `-- --meat`, `-- --locomotion`, `-- --equipment`, `-- --spear`, `-- --nemi-death`, `-- --ground`, `-- --pillars` — and each overwrites the sources it names.
Say which mode you are in (re-export vs. rebuild) before running anything, and never rebuild when the user asked for a re-export.

## The seam with Rust — check it before exporting

Clip and model names are a contract with the game code, not just Blender labels.
Before exporting an enemy or the character rig, check the `EnemyModel` clips in `../../../src` (e.g. `src/worm.rs`: `WormIdle`, `WormBite`, `WormDeath`) and the locomotion/carry sets `check_assets.py` enforces for `plutonian` (`Idle`, `Walk`, `Run`, `Jump`, directional walks, `SwordMainSlash`, `SwordOffSlash`, `SpearThrust`, `NemiDeath`).
Keep animation names unchanged unless the issue explicitly asks to rename one — and if it does, update the Rust side in the same change.
Equipment geometry must fit the per-item bounds from `ItemKind::gear()` in `src/equipment.rs`; `check_assets.py` verifies that.

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

Check finished work against `../../../docs/design/art-style.md` before reporting done.
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
python3 scripts/check_assets.py
```

It validates GLB structure, resources, skin, animation names, and equipment bounds, and asserts every exported GLB has a matching `.blend` source in `../../../art`.
If it fails, fix the cause and rerun — do not hand the user an export that fails it.
If the change also touches Rust, run the repo's Rust verification from `AGENTS.md` (`cargo fmt --check`, `cargo clippy --locked --all-targets`, `cargo test --locked`) as well.
Do not claim a check passed that did not run.

## Title screen

To refresh the title render after editing the scenery: `blender --background -noaudio art/outpost.blend --python scripts/render_title.py`.

## Out of scope

- Live MCP connections: the scripts run in a separate background Blender process; none is required.
