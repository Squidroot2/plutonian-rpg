# The Plutonian

A third-person exploration RPG prototype built with Bevy 0.20 and Blender. You are a Plutonian returning to Nivalis, a small settlement whose crossing beacon has gone dark. Speak to Nemi, follow the blue waystones to the cavern, and bring its power core home.

## Run

Requires Rust 1.97.1 or newer, a Vulkan-capable graphics driver, and a Linux desktop. Install Bevy's [Linux development dependencies](https://bevy.org/learn/quick-start/getting-started/setup/#installing-os-dependencies) for your distribution, including ALSA/udev only if you enable audio/gamepad features later. This build uses X11 or Wayland and includes no audio or gamepad integration.

```sh
cargo run --locked
```

The game opens in borderless fullscreen. Run `cargo run --locked -- --windowed` to open it in a window.
Open **Settings** from the title screen or pause menu to switch display modes during the current session.

The first build compiles Bevy and its renderer and can take several minutes. Run from the repository root so Bevy finds `assets/`. `Cargo.lock` pins the dependency versions used for this application.

Choose **Start Game** on the title screen to load Nivalis. Click a button or select it with the arrow keys or Tab and activate it with Enter or Space.

| Control | Action |
| --- | --- |
| W / S | Walk forward / backpedal |
| A / D | Strafe left / right |
| Mouse | Orbit camera and turn to face its horizontal look direction |
| Scroll | Adjust camera distance |
| Left Shift | Sprint forward or on a forward diagonal |
| Left click | Thrust with an equipped spear, aiming with the camera through impact |
| Space | Jump (grounded only) |
| Left Ctrl | Dodge hop in the WASD direction, or backward with no movement input (grounded only) |
| E | Speak / collect a nearby item / dismiss dialogue |
| Enter | Quest interaction / dismiss dialogue |
| I | Open / close inventory |
| Escape | Close inventory / dismiss dialogue / pause or resume |

The pause menu also has Resume and Quit to desktop buttons. Losing window focus pauses the game. Progress lasts for the current session; relaunching starts a fresh quest. Falling beyond the playable area returns you to the outpost without removing quest progress.

## World and implementation

The playable route connects the outpost, an inclined ice shelf, and a shallow cavern. Follow the luminous waystones toward the cavern; the objective displays the distance to the current destination. The power core appears in inventory as a protected quest item and disappears from its plinth when collected. Returning it to Nemi consumes the core, lights the beacon, and completes the quest. You can continue exploring afterward.

`src/` separates loading, world assembly, player movement/animation, camera, shared interactions, inventory, equipment, quests, and UI into plugins. Avian's kinematic move-and-slide controller runs at a fixed timestep with capsule ground detection and tuned low gravity. The camera uses a sphere cast to avoid passing through geometry. Interaction requires a nearby target with a clear line of sight. There are no save files or migrations.

The game includes one beacon quest, the Old Relay dungeon, and one speaking NPC. Survival, crafting, levelling, controller support, and persistent saves are deferred. This is fictional Pluto: terrain, illumination, and gravity favor the experience over physical accuracy. Assets are original, generated locally with the included Blender script; no external models or textures are required.

## Inventory

You can carry 60 kg, including equipped gear. Above 60 kg you can still walk. Sprinting, jumping, and dodging require a load of 60 kg or less. Above 72 kg, drop items to move again. The smoke arena has two 40 kg Large Boulders.

E picks up the nearest eligible item within three meters and clear line of sight. E also speaks to Nemi and collects the quest core; one press performs one action. E or Enter dismisses an open conversation before any other interaction.

Batteries, scrap, and methane pouches are scattered around the outpost, trail, and cavern: two of each material in each region, for 18 initial pickups. Pickups transfer their whole stack. Inventory starts empty, stacks by item type, and has no capacity limit. Materials have no crafting or consumption effects yet.

Press I to open inventory. Gameplay, gravity, character animation, and camera controls pause while the panel is open, and the cursor becomes available. Select an item in the bag, click its quantity field to type an integer (or use the minus/plus buttons), then click **Drop selected quantity**. I, Escape, or Close returns to play. Scroll the bag list to find additional items; equipment slots and selected-item actions stay outside the scrolling list. Inventory cannot open during dialogue or from the pause menu.

Dropped quantities become one stationary world stack near the player and can be collected with E. Placement checks supporting ground, free space, and walls; a failed drop leaves the inventory unchanged. The beacon core is marked **Quest item** and cannot be dropped. All inventory, quest, and pickup changes last only for the current session; relaunching resets them.

## Equipment

Armor and helmets are world pickups collected with E. Start with empty equipment slots and find the field set near the outpost and the reinforced set inside the cavern.

| Item | Slot | Defense |
| --- | --- | ---: |
| Field armor | Body armor | 2 |
| Reinforced armor | Body armor | 5 |
| Field helmet | Helmet | 1 |
| Reinforced helmet | Helmet | 3 |

Open inventory with I, select a gear item from the bag, and click **Equip**. Equipping moves one copy into its matching slot; swapping returns the previous piece to the bag. Click **Unequip** beside a slot to return the worn item before dropping it. Additional bag copies can be dropped without affecting the worn copy. Identical gear stacks in the bag; equipping the same type again is a no-op.

Slots show their equipped piece and defense, with a combined total above them. Each point of defense reduces bite damage by one, to a minimum of one. Equipment survives fall recovery and resets on application restart.

Equipped models follow the player's animated spine and head bones; Nemi's appearance stays unchanged. Helmets have open faces and clearance for the alien's eyes and sensory horns.

## Spear practice

Find the field spear and trident near the outpost spawn, pick either up with E, and equip it in the **Weapon** slot in inventory. Aim the camera at the practice dummy on the other side of the spawn and left click while grounded. Both weapons use the same two-handed grip at the right hip and thrust up to 2.5 m. A hit makes the dummy flash and rock. The thrust briefly locks movement, and a wall stops it before it can hit the dummy. Unequip the weapon before dropping it.

## Ice worms

Two ice worms roam beside the trail and a third waits near the cavern entrance. Watch for the worm to rear up, move out of its bite, then thrust during its recovery. Three hits with either weapon defeat a worm. A bite deals 20 damage before armor, and defeat returns you to the outpost at full health with your quest, inventory, and equipment intact. After five seconds without taking damage, you recover 10 health per second. The health bar fades away when you reach full health. Defeated worms stay gone for the session.

## The Old Relay

After you restore the beacon, look for the round hatch southwest of Nemi. Use E to enter. Follow the amber lamps to the maintenance switch, then go through the raised gate to discover the archive survey log. Two ice worms patrol the relay. Use the hatch in the vestibule to return to Nivalis. Gate progress and the discovery stay with you for the session.

## Blender workflow

Pass `-- --relay` to the asset command to regenerate the Old Relay source and collision files. The paired GLBs are `old_relay` and `old_relay_collision`.

Pass `-- --worm` to the asset command to regenerate only the ice worm source and its four animation clips.

The title screen uses a render of the outpost. To refresh it after editing the scenery, run `blender --background -noaudio art/outpost.blend --python scripts/render_title.py`.

Blender 5.3 was used for the included editable sources in `art/`. Runtime exports are checked in under `assets/models/`, so Blender is not needed to play.

| Source / export | Purpose |
| --- | --- |
| `plutonian` | Shared skinned character, with base locomotion, spear carry, and `SpearThrust` actions |
| `outpost` | Ice shelf, habitats, waystones, cavern, beacon tower, distant sky details |
| `collision` | Simplified static geometry, hidden at runtime |
| `core` | Collectible beacon power core |
| `battery`, `scrap`, `methane_pouch` | Ground-based material pickups |
| `field_armor`, `reinforced_armor`, `field_helmet`, `reinforced_helmet` | Equipment sources, each exporting pickup and `_worn` GLBs |
| `field_spear`, `trident` | Weapons sharing the spear animations, each exporting pickup and `_worn` GLBs |

To re-export **your edited `.blend` sources** without regenerating them:

```sh
blender --background -noaudio --factory-startup --python-exit-code 1 --python scripts/assets.py -- --export
```

To **rebuild all source art and exports from the procedural designs**, overwriting the editable `.blend` sources:

```sh
blender --background -noaudio --factory-startup --python-exit-code 1 --python scripts/assets.py
```

To regenerate only the three material models without replacing existing world or character art, pass `-- --items` to the asset command. This overwrites the material sources and exports. Pickup models use a base origin at zero height; material models use a 0.62 m square placement footprint and 0.6 m height. Equipment uses its larger per-item bounds from `ItemKind::gear()` in `src/equipment.rs`; `scripts/check_assets.py` verifies the exported geometry fits those bounds.

Pass `-- --locomotion` to regenerate the player's backpedal, strafe, and diagonal actions in `art/plutonian.blend` and re-export `assets/models/plutonian.glb`.

To regenerate only equipment, pass `-- --equipment`. This overwrites equipment sources and exports while preserving character and map art. Each equipment `.blend` stores geometry in its attachment frame (`spine` for armor, `head` for helmets, `spear_grip` for weapons). The generator derives that frame from the exported character rig. `--export` produces the bone-local `_worn.glb` and automatically centers a second, base-origin pickup export from the same source geometry. Recheck fit after changing the character rig. Keep worn sources in their attachment frame.

Pass `-- --spear` to regenerate the field spear, trident, practice dummy, and the character's spear carry and `SpearThrust` actions. Both weapons attach to `spear_grip` on the spine and use the same two grip points. The spear pose generator bends the upper arm and forearm around each elbow, with blended weights at the joint. Export the pickups flat on the ground using the same sources, and keep the action's contact at frame 6 and recovery at frame 16 (24 fps).

The scripts run in a separate background Blender process; a live MCP connection is not required. One Blender unit equals one meter. Author with Z up and the character facing -Y; glTF export converts to Bevy's Y-up coordinates, with the character facing +Z. Keep scale applied and keep the animation names unchanged. Materials use Principled BSDF base color, roughness, metallic, and emission. Animation is exported as sampled actions. Keep visible environment edits and `collision.blend` aligned; collision does not regenerate from edited outpost sources during `--export`.

Trail pillar bodies (the `Waystone` meshes) are included in the hidden collision scene; their glowing lamps remain decorative. To add missing pillar bodies to an existing collision source without replacing other collision meshes, run the asset command with `-- --pillars`. Full procedural generation includes them automatically.

The authored environment contains the tower; Bevy owns its switchable lens and light, NPC/player placement, and collectible placement. At the end of loading, the world plugin aligns the player and keeper to the collision surface and rests the core on its plinth. The beacon lens follows the authored spindle. Add collision meshes to `collision.blend` for solid additions. The loader waits for scene instantiation and collider construction, and reports missing assets, animation names, or load timeout on screen.

Foundations follow the inclined ice shelf, with a 4 cm overlap to avoid gaps along sloping ground. To repair placements in existing outpost sources while preserving meshes and materials, run the asset command above with `-- --ground` instead of `-- --export`. This moves attached details and matching collision objects together. Full procedural generation applies the same grounding automatically.

To check foundation contact and matching visual/collision transforms without changing sources:

```sh
blender --background -noaudio --factory-startup --python-exit-code 1 --python scripts/grounding.py
```

## Verification

```sh
cargo fmt --check
cargo check --locked
cargo clippy --locked --all-targets
cargo test --locked
cargo run --locked -- --smoke-test
cargo run --locked -- --smoke-world
python3 scripts/check_assets.py
```

Run the desktop checks on an idle graphical desktop without supplying mouse or keyboard input. Add `--windowed` to either command to run it in a window.

- `--smoke-test` starts a standalone arena with nearby materials, equipment, a practice dummy, and the keeper and core. It checks inventory quantities and drops, both weapons' hits and misses, a worm pursuit, dodge, bite, and defeat, armor swaps and attachment during movement, the 640×704 inventory layout, wall collision, pause/resume, and fall recovery. The short beacon interaction verifies that the core is protected in the bag, consumed on turn-in, and lights the beacon. The run prints `SMOKE PASS` and its duration, or fails within 60 seconds with the current phase.
- `--smoke-world` loads the shipped outpost, verifies the player and worms start grounded, checks the route past the worms, and checks that every authored pillar blocks the movement capsule from four directions. It then checks the locked hatch, beacon turn-in, gate, archive discovery, return and reentry, and fall recovery. It prints `WORLD SMOKE PASS` on success.

The checks save screenshots in `/tmp/plutonian-*.png`, including the arena, outpost, inventory, beacon, and weapon and armor poses.

Manual acceptance: walk and sprint, jump and land, traverse the trail rise, push against habitat/cavern walls, orbit the camera near obstructions, try interacting out of range or through a wall, read/dismiss each dialogue, collect the core once, return to Nemi, confirm the lit beacon, pause/resume using keys and buttons, fall off the ice shelf, pick up materials on the trail and in the cavern, use the inventory quantity field and buttons, drop and recollect stacks near slopes and walls, equip and swap both gear sets, unequip/drop/recollect gear, inspect its fit during animation, and restart to confirm fresh quest, inventory, and equipment state. Automated checks cannot establish how comfortable the movement or composition feels.

## License

Licensed under the [MIT License](LICENSE).
