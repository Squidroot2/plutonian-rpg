# Blender workflow

Pass `-- --relay` to the asset command to regenerate the Old Relay source and collision files.
The paired GLBs are `old_relay` and `old_relay_collision`.

Pass `-- --worm` to the asset command to regenerate the ice worm source and its four animation clips.
Pass `-- --roach` to regenerate the relay roach source and animations.
Pass `-- --hushcap` to regenerate the Hushcap and Mycelium sources and exports.
Use `-- --export --hushcap` or `-- --export --relay` after editing those Blender sources by hand.
Pass `-- --south-basin` to regenerate the basin and cave sources and exports.
Use `-- --export --south-basin` to re-export their edited Blender sources.
Use `-- --build-crossing` to generate the Listening Flats, the moving gate, the vellik, and the crossing wagon.
Use `-- --export --crossing` to re-export their edited Blender sources.
Use `-- --build-wagon` to regenerate just the crossing wagon.
Use `-- --export --wagon` to re-export its edited Blender source.

The title screen uses a render of the outpost.
To refresh it after editing the scenery, run `blender --background -noaudio art/outpost.blend --python scripts/render_title.py`.

Blender 5.3 was used for the included editable sources in `art/`.
Runtime exports are checked in under `assets/models/`, so Blender is not needed to play.

| Source / export | Purpose |
| --- | --- |
| `plutonian` | Shared skinned character with locomotion, melee and musket actions, and `PlutonianDeath` |
| `outpost` | Ice shelf, furnished village huts, waystones, cavern, beacon tower, perimeter cliffs and gate, distant sky details |
| `collision` | Simplified static geometry, hidden at runtime |
| `core` | Collectible beacon power core |
| `battery`, `scrap`, `methane_pouch` | Ground-based material pickups |
| `large_boulder`, `practice_dummy` | Carrying-limit test pickup and weapon practice target |
| `ice_worm`, `relay_roach` | Animated enemies with idle, crawl, bite, and death clips |
| `hushcap` | Rooted enemy with idle, spit, and collapse clips |
| `old_relay`, `old_relay_collision` | Old Relay scenery and its hidden collision geometry |
| `south_basin`, `south_basin_collision` | South Basin and Hushcap Caverns scenery and collision geometry |
| `listening_flats`, `listening_flats_collision` | Crossing road, valley, and perimeter collision |
| `nivalis_gate`, `vellik`, `crossing_wagon` | Moving gate leaf and caravan models |
| `worm_meat`, `roach_meat`, `mycelium` | Corpse loot pickup models |
| `field_armor`, `reinforced_armor`, `field_helmet`, `reinforced_helmet` | Equipment sources, each exporting pickup and `_worn` GLBs |
| `field_spear`, `trident` | Weapons sharing the spear animations, each exporting pickup and `_worn` GLBs |
| `shortsword` | One-handed weapon with pickup and `_worn` GLBs for either hand |
| `methane_musket` | Barracks two-handed weapon with pickup and `_worn` GLBs |
| `methane_slug`, `methane_muzzle_flash` | Musket projectile and animated ignition/exhaust |

To re-export **your edited `.blend` sources** without regenerating them:

```sh
blender --background -noaudio --factory-startup --python-exit-code 1 --python scripts/assets.py -- --export
```

Run the headless validator after exporting.

```sh
cargo run --locked -p plutonian-assets
```

Append `--outpost` to export just the outpost and its collision scene.
Append `--shortsword` to export just the player rig and the two shortsword models.

Use `-- --build-perimeter` to regenerate the cliffs, walls, and gate frame in the existing outpost and collision sources and exports.
After hand-editing the perimeter, use `-- --export --outpost` to export both sources.
See [Nivalis perimeter](../design/nivalis-perimeter.md) for the design and collision details.

Use `-- --build-village` to regenerate the resident huts in both existing outpost sources and exports.
Hut footprints, entrance directions, furnishings, and cot counts are defined in `scripts/village.py`.
Run the crossing smoke after moving an entrance, since it also changes the walking route.

To rebuild only the procedural outpost and its collision scene, including their editable `.blend` sources and GLB exports:

```sh
blender --background -noaudio --factory-startup --python-exit-code 1 --python scripts/assets.py -- --build-outpost
```

To **rebuild all source art and exports from the procedural designs**, overwriting the editable `.blend` sources:

```sh
blender --background -noaudio --factory-startup --python-exit-code 1 --python scripts/assets.py
```

To regenerate only the three material models without replacing existing world or character art, pass `-- --items` to the asset command.
This overwrites the material sources and exports.
Pickup models use a base origin at zero height; material models use a 0.62 m square placement footprint and 0.6 m height.
The validator reads per-item pickup dimensions from `data/items/catalog.item.ron`.
It checks the equipment and small loot exports against those dimensions.

Pass `-- --meat` to regenerate the two meat pickup models.

Pass `-- --locomotion` to regenerate the player's backpedal, strafe, and diagonal actions in `art/plutonian.blend` and re-export `assets/models/plutonian.glb`.

Pass `-- --list` to print the registered asset flags, builder status, and export names without building or opening a Blender scene.
An unrecognized asset flag exits with an error naming that flag instead of starting a full rebuild.

To regenerate only equipment, pass `-- --equipment`.
This overwrites equipment sources and exports while preserving character and map art.
Each equipment `.blend` stores geometry in its attachment frame.
Armor uses `spine`, helmets use `head`, polearms use `spear_grip`, and shortswords use matching wrist sockets.
The generator derives these frames from the exported character rig. `--export` produces the bone-local `_worn.glb` and automatically centers a second, base-origin pickup export from the same source geometry.
Recheck fit after changing the character rig.
Keep worn sources in their attachment frame.

Pass `-- --spear` to regenerate the field spear, trident, practice dummy, and the character's spear carry and `SpearThrust` actions.
Both weapons attach to `spear_grip` on the spine and use the same two grip points.
The spear pose generator bends the upper arm and forearm around each elbow, with blended weights at the joint.
Export the pickups flat on the ground using the same sources, and keep the action's contact at frame 6 and recovery at frame 16 (24 fps).

Pass `-- --shortsword` to regenerate the shortsword source, wrist sockets, and sword carry and slash actions.
The slashes contact at frame 5 and recover at frame 12 (24 fps).

Pass `-- --musket` to regenerate the musket actions on the existing character rig,
the musket equipment source, and the slug and flash sources.
Use `-- --export --musket` to re-export the edited sources.
Frame numbers below are zero-based at 24 fps.

| Action on `plutonian` | Frames | Art contract |
| --- | ---: | --- |
| `MusketIdle` | 0–48 (2 s) | Seamless drawn, low-ready hold with both hands on their grips. |
| `MusketAim` | 0–24 (1 s) | Seamless shouldered aim with the bore level. |
| `MusketFire` | 0–12 (0.5 s) | Shot and ignition at frame 1, peak recoil at frame 3, back to aim at frame 12. |
| `MusketReload` | 0–60 (2.5 s) | Left hand fetches the pouch at frame 20, seats it at frame 39, presses the breech at frame 44, returns to the fore-end at frame 54, and completes at frame 60. |

The musket attaches to `musket_grip` when drawn and `musket_holster` when slung across the back.
Both sockets follow `spine`.
The worn export uses the trigger grip as its origin and `+Z` downrange,
so it also fits the existing upright rack mount.
The flat pickup fits a `0.22 × 0.38 × 1.46 m` (X, Y, Z) clearance.
Spawn the shot and effect at the worn model's `MusketMuzzle` node
(`0, 0.165, 1.06 m` relative to the grip in glTF/Bevy coordinates).
The slug points along `+Z`;
its tapered wake is part of the model so the silhouette reads at the design's 60 m/s speed.
Play `MethaneMuzzleFlash` once, starting at the shot:
teal-blue ignition peaks at frame 1 and is gone by frame 2;
the cold exhaust expands, rises, and dissipates by frame 10 (0.417 s).

See the [pose comparisons](../design/images/musket-poses.jpg)
and [rack and shot comparisons](../design/images/musket-rack-and-shot.jpg).

Use `-- --export --plutonian-death` to rebuild the collapse action on the existing shared character rig.
This mode rewrites `art/plutonian.blend` and exports only `assets/models/plutonian.glb`.

The scripts run in a separate background Blender process; a live MCP connection is not required.
One Blender unit equals one meter.
Author with Z up and the character facing -Y; glTF export converts to Bevy's Y-up coordinates, with the character facing +Z.
Keep scale applied and keep the animation names unchanged.
Materials use Principled BSDF base color, roughness, metallic, and emission.
Animation is exported as sampled actions.
Keep visible environment edits and `collision.blend` aligned; collision does not regenerate from edited outpost sources during `--export`.

Trail pillar bodies (the `Waystone` meshes) are included in the hidden collision scene; their glowing lamps remain decorative.
To add missing pillar bodies to an existing collision source without replacing other collision meshes, run the asset command with `-- --pillars`.
Full procedural generation includes them automatically.

The authored environment contains the tower; Bevy owns its switchable lens and light, NPC/player placement, and collectible placement.
At the end of loading, the world plugin aligns the player and keeper to the collision surface and rests the core on its plinth.
The beacon lens follows the authored spindle.
Add collision meshes to `collision.blend` for solid additions.
The loader waits for scene instantiation and collider construction, and reports missing assets, animation names, or load timeout on screen.

Foundations follow the inclined ice shelf, with a 4 cm overlap to avoid gaps along sloping ground.
To repair placements in existing outpost sources while preserving meshes and materials, run the asset command above with `-- --ground` instead of `-- --export`.
This moves attached details and matching collision objects together.
Full procedural generation applies the same grounding automatically.

To check foundation contact and matching visual/collision transforms without changing sources:

```sh
blender --background -noaudio --factory-startup --python-exit-code 1 --python scripts/grounding.py
```

## In-engine equipment previews

Use `plutonian-art-preview` for an art issue's in-engine screenshots.
It displays the worn export on the player rig or the Barracks weapon rack
using the shipped outpost and game lighting.
It reads GLBs directly,
so the art can be reviewed before the catalog entry or gameplay integration exists.

Run from the repository root on a desktop:

```sh
cargo run --locked --features smoke --bin plutonian-art-preview -- \
  docs/contributing/previews/musket.ron drawn
```

The preview opens a window,
saves `/tmp/plutonian-art-drawn.png`,
and exits.
Pass a third argument to choose another PNG path with an existing parent directory.
If the local display needs software Vulkan,
use the environment and `xvfb-run` prefix from [desktop smoke setups](verification.md#desktop-smoke-setups)
with the preview command in place of `scripts/smoke.sh`.

The [musket recipe](previews/musket.ron) recreates the review views for #163.
To capture the comparisons and detail shots:

```sh
for shot in before-drawn drawn before-aim aim before-holstered holstered \
  before-rack rack before-shot shot smoke reload; do
  cargo run --locked --features smoke --bin plutonian-art-preview -- \
    docs/contributing/previews/musket.ron "$shot" || break
done
```

For a new model,
write a small RON recipe such as:

```ron
(
    model: "shortsword_worn",
    shots: {
        "in-hand": (mount: Socket("sword_grip_right")),
        "racked": (mount: Rack),
    },
)
```

- Use the exported registry stem without `.glb`,
  an asset path such as `models/shortsword_worn.glb` or `assets/models/shortsword_worn.glb`,
  or a local GLB path relative to the working directory or absolute.
- `Socket("…")` looks up the named node on `plutonian.glb`.
  Use the rig's hand, body, helmet, or holster socket that matches the worn export.
  `Rack` uses the existing first rack slot.
- Omit `pose` for the rig's rest pose,
  or use `pose: Some((clip: "SwordMainSlash", frame: 5))` to sample a named clip at a zero-based 24 fps frame.
  As in the musket recipe,
  `#![enable(implicit_some)]` lets you omit the `Some(…)` wrapper.
- The default view is `Hand` for a socket and `Rack` for the rack.
  Override it with `view: Some(Front)`, `Shoulder`, `Back`, `Wide`, `Detail`, or `Muzzle`.
- `hide_model: true` captures the same view without the gear for before/after comparisons.
  Optional `effects` mount other GLBs on named nodes of the worn export,
  with a socket-local offset in Bevy coordinates and an optional sampled clip.
