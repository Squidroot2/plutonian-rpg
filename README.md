# The Plutonian

A third-person exploration RPG prototype built with Bevy 0.20 and Blender.
  You wake in a tent in Nivalis, a small settlement whose crossing beacon has gone dark.
  Speak to Nemi, follow the blue waystones to the cavern, and bring its power core home.

## Run

Requires Rust 1.97.1 or newer, a Vulkan-capable graphics driver, and a Linux desktop running X11 or Wayland.
  Install your distribution's development packages for the window backends and ALSA audio.
  For the package list used on Ubuntu 24.04, see the [CI workflow](.github/workflows/ci.yml).
  For other distributions, see [Bevy's Linux setup guide](https://bevy.org/learn/quick-start/getting-started/setup/#installing-os-dependencies).

```sh
cargo run --locked
```

The game opens in borderless fullscreen.
  Run `cargo run --locked -- --windowed` to open it in a window.
Open **Settings** from the title screen or pause menu to switch display modes during the current session.
The **FPS counter** setting cycles through the four screen corners and Off.
  It starts in the top left.
Under **Keybinds**, select a gameplay action, menu control, or menu shortcut and press its new key.
  Both attack actions also accept mouse buttons.
  Press Esc to cancel, or choose **Restore defaults** to reset all bindings.
  Changes apply immediately for the current session.

The first build compiles Bevy and its renderer and can take several minutes.
  Run from the repository root so the game can find `assets/` and the quest catalog in `data/`. `Cargo.lock` pins the dependency versions used for this application.

Choose **Start Game** on the title screen to begin a fresh run in Nivalis, or **Load Game** to resume your last bed save.
  Click a button or select it with Up, Down, or Tab and activate it with Enter or Space.

| Control | Action |
| --- | --- |
| W / S | Move forward / backward in play. Move selection up / down in Inventory. |
| A / D | Strafe left / right |
| Mouse | Orbit camera and turn to face its horizontal look direction |
| Scroll | Adjust camera distance |
| Caps Lock | Toggle between walking and running |
| Left Shift | Sprint while moving straight forward |
| Left click | Attack with the main-hand weapon |
| Right click | Attack with the off-hand weapon |
| X | Holster or draw all equipped weapons |
| Space | Jump (grounded only) |
| Left Ctrl | Dodge hop in the WASD direction, or backward with no movement input (grounded only) |
| E | Interact with a nearby target or continue past a dialogue reply or notice |
| Enter | Quest interaction, select a dialogue topic, or continue past a reply or notice |
| W / S | Move between dialogue topics (Menu Up / Menu Down) |
| I | Open or switch to Inventory / close the player menu from Inventory |
| L | Open or switch to Quest / close the player menu from Quest |
| Q | Open the drop quantity prompt for the selected item in Inventory |
| Tab | Open the last player-menu tab / close the menu |
| Escape | Cancel the drop prompt, close the current menu or panel, dismiss dialogue, or pause / resume |

The pause menu also has Resume and Quit to desktop buttons.
  Losing window focus pauses the game.
  Falling beyond the playable area returns you to the outpost and preserves current progress.

### Saving

Press E near the bed in the starting tent to save.
  A successful save restores full health and replaces the single save slot.
  Starting a fresh run leaves that slot available until you save again.
  The game uses `$XDG_DATA_HOME/plutonian-rpg/save.bin` when `XDG_DATA_HOME` is an absolute path and `~/.local/share/plutonian-rpg/save.bin` otherwise.

Combat defeat returns you to the tent at full health and restores the last save from your run, including quests, equipment, inventory, chests, pickups, and encounters.
  If you die before using the bed in a new run, that run restarts from the beginning.

## World and implementation

The core route connects the outpost, an inclined ice shelf, and a shallow cavern.
  Follow the luminous waystones toward the cavern.
  The quest log records directions and completed steps for each quest.
  The power core appears in inventory as a protected quest item and disappears from its plinth when collected.
  Speak to Nemi and choose **The Last Light** to hand over the core, light the beacon, and complete the quest.
  You can also ask about Nemi or Nivalis. Click a topic, or use Menu Up and Menu Down and press Enter.
  You can continue exploring afterward.

South of the outpost courtyard, the ice shelf opens into South Basin.
  Two paths around the ice fins lead behind a ridge,
  where a descending tunnel enters Hushcap Caverns.
  The cave is open from the start and has three chambers,
  with two passages through the middle chamber.
  Eight Hushcaps, two ice worms, and two relay roaches inhabit the cave.
  Rock formations offer cover from Hushcap spit.

`src/` separates loading, world assembly, player movement/animation, camera, shared interactions, inventory, equipment, quests, and UI into plugins.
  Avian's kinematic move-and-slide controller runs at a fixed timestep with capsule ground detection and tuned low gravity.
  The camera uses a sphere cast to avoid passing through geometry.
  Interaction requires a nearby target with a clear line of sight.
  Save files use a versioned bitcode format.

The main quest line continues from The Last Light through Old Relay and Contain the Hushcaps.
  Survival, crafting, and controller support are deferred.
  This is a fictional Pluto with terrain, illumination, and gravity tuned for play.
  The included Blender scripts generate the original models and title backdrop.
  Audio includes procedural combat and pickup effects and bundled footsteps and equipment sounds.
  See [Credits](CREDITS.md) for the bundled sound sources.

## Player menu

Press Tab during gameplay to open the player menu.
  Choose **Quest**, **Inventory**, **Equipment**, or **Character**.
  The menu starts on Quest and remembers the last tab during the current run.
  Tab, Escape, and Close return control to the player.
  The cursor is available while the menu is open, and the world keeps running.

### Character

View Vitality, Endurance, Agility, and Strength and preview each next rank.
New characters start at rank 5, and ranks cap at 100.
  Use **+** to spend
available points.
  The progression system awards three points per level gained.
You earn XP by completing quests and when an actor you've damaged dies.
  The
Character tab shows your level and progress toward the next one, and the cot
saves that progress with your attributes.
  See [Experience and Leveling](docs/design/experience.md).

### Quest

Select a quest to read its details.
  Active and completed entries list their completion reward.
  Inspecting the sealed Old Relay hatch adds that quest alongside The Last Light.
  Restoring power also adds it if you missed the hatch.

A landed attack makes Nemi flee and fails The Last Light if it is active.
  A surviving Nemi returns after the player backs off, then refuses to help.
  Follow **Relight the Beacon** to use the charged core at the tower base.
  If you already restored the beacon with Nemi, attacking them leaves the beacon powered.

Quest definitions live in `data/quests/catalog.quest.ron`.
  Add a definition with a unique ID, a positive `completion_xp` reward, activation rules, named objectives, and conditional transitions.
  Definitions can use the catalog’s existing world actions and quest-completion conditions.
  The game loads the catalog when you start or load a run.
  See [Quest System](docs/design/quests.md).

## Inventory

Carried weight includes equipped gear.
  Endurance sets the weight limits.
  At the starting rank, running, sprinting, jumping, and dodging require a load of 60 kg or less.
  You can still walk with more than 60 kg, up to 72 kg.
  Above the walking limit, drop items to move again.
  The smoke arena has two 40 kg Large Boulders.

E picks up the nearest eligible item within three meters and clear line of sight.
  E also speaks to Nemi and collects the quest core; one press performs one action.
  E or Enter dismisses an open conversation before any other interaction.

Batteries, scrap, and methane pouches are scattered around the outpost, trail, and cavern, with two of each material per region for 18 initial pickups.
  Pickups transfer their whole stack.
  Inventory starts empty and stacks by item type.
  Weight affects movement as described above.
  Materials have no crafting or consumption effects yet.

Select the Inventory tab or press I to jump straight to it.
  Each row shows the owned quantity and total weight.
  Equipped gear appears with an **Equipped** status and counts toward the row totals.
  Use W or S to move between rows and see per-unit weight and details.
  Rebind these keys under **Menu Up** and **Menu Down** in Settings.

Select a row and click **Drop** at the bottom of Inventory or press Q to open the quantity prompt.
  Type a number in the quantity field or use the minus and plus buttons.
  Click **Confirm drop** or press Enter to drop that amount.
  Escape or **Cancel** dismisses the prompt.
  For equipped gear, the prompt counts only spare copies in the bag.

Dropped quantities become one stationary world stack near the player and can be collected with E.
  Placement checks supporting ground, free space, and walls.
  The inventory keeps the stack if placement fails.
  The beacon core is marked **Quest item** and cannot be dropped.

### Tent storage

Press E near one of the three chests in the starting tent.
  Use **Store 1** or **Store stack** on a backpack item, and **Take 1** or **Take stack** on a chest item.
  Close the panel with **Close** or Escape.
  Each chest holds its own stacks.
  Stored items add no carried weight, and the contents stay where you left them when you return from exploring.

The Lamp chest starts with two batteries and two methane pouches.
  The Supply chest holds two scrap and field armor, and the Gear chest holds a field helmet.
  You can also store the beacon power core.
  Take it back out before turning it in.
  Unequip worn gear to move it from your backpack into storage.

## Equipment

Start with empty equipment slots.
  Find the field armor and helmet in the tent chests and pick up the reinforced set with E inside the cavern.

| Item | Slot | Defense |
| --- | --- | ---: |
| Field armor | Body armor | 2 |
| Reinforced armor | Body armor | 5 |
| Field helmet | Helmet | 1 |
| Reinforced helmet | Helmet | 3 |

On the **Equipment** tab, click **Equip** beside an empty slot and choose a compatible item.
  The Reinforced Helmet, Trident, and Reinforced Armor require Strength ranks 7, 8, and 10 respectively.
  You can also double-click a gear row in **Inventory**.
  If your Strength is too low, a warning appears.
  Equipping moves one copy into a slot and returns any displaced gear to the bag.
  Click **Unequip** to return an equipped item to the bag before dropping it.
  You can drop spare bag copies at any time.

Slots show their equipped piece and defense, with a combined total above them.
  Each point of defense reduces incoming damage by one, to a minimum of one damage per hit.
  Equipment survives fall recovery and is included in bed saves.

Equipped models follow the player's animated rig.
  Helmets have open faces and clearance for the alien's eyes and sensory horns.

## Weapon practice

Find the field spear, trident, and two shortswords in the outpost courtyard and pick them up with E.
  The spear and trident thrust up to 2.5 m.
  Each uses both hands and returns both equipped weapons to your bag when you swap it in.
  A shortsword slashes up to 1.4 m and can be equipped in either hand.
  Double-click the shortsword row twice in an empty loadout to equip both copies.

Aim at the practice dummy while grounded.
  Left-click to attack with the main hand or right-click to attack with the off hand.
  When dual wielding, click both buttons to swing together, or start one swing while the other hand is still attacking.
  Each hand recovers independently.
  Movement locks while either attack is active.
  A wall stops the attack, and a hit makes the dummy flash and rock.

Press X to put your equipped weapons on your back or hips.
  Press it again to draw before attacking.
  Wait for both hands to finish recovering before holstering.
  Change the shortcut under **Settings → Keybinds → Gameplay → Holster / Draw Weapons**.

## Ice worms

Two ice worms roam beside the trail and a third waits near the cavern entrance.
  Watch for the worm to rear up, move out of its bite, then attack during its recovery.
  Three spear or trident thrusts defeat a worm.
  A worm bite deals 20 damage before armor.
  After five seconds without taking damage, you recover 10% of maximum health per second.
  The health bar fades away when you reach full health.
  Defeated worms and relay roaches leave corpses with loot that persists in bed saves.
  Press E near an unlooted corpse, select **Take**, and use **Close** or Escape to return to play.
  Meat stacks in the backpack and can be dropped.

## The Old Relay

After you restore the beacon, look for the round hatch northwest of the beacon tower.
  Use E to enter.
  Follow the amber lamps to the maintenance switch, then go through the raised gate to discover the archive survey log.
  An ice worm and a fast, biting relay roach patrol the relay.
  In the archive, a rooted Hushcap spits from beside a cracked conduit.
  The service console offers cover, and the survey log can be read while the Hushcap is alive.
  A defeated Hushcap yields one Mycelium.
  Use the hatch in the vestibule to return to Nivalis.
  Gate progress and the discovery are included in bed saves.

## Contain the Hushcaps

After reading the survey log and returning through the relay hatch,
  speak to Nemi and choose **Contain the Hushcaps** to report the mushroom creature.
  They send you south through South Basin to Hushcap Caverns.
  Defeat five Hushcaps in the caverns and return with at least five Mycelium in your backpack.
  The archive specimen can supply one of the samples.
  Nemi examines the Mycelium and lets you keep it.
  The Quest Log shows how many cavern Hushcaps you have defeated and how many samples you carry.

## Blender workflow

Pass `-- --relay` to the asset command to regenerate the Old Relay source and collision files.
  The paired GLBs are `old_relay` and `old_relay_collision`.

Pass `-- --worm` to the asset command to regenerate the ice worm source and its four animation clips.
  Pass `-- --roach` to regenerate the relay roach source and animations.
  Pass `-- --hushcap` to regenerate the Hushcap and Mycelium sources and exports.
  Use `-- --export --hushcap` or `-- --export --relay` after editing those Blender sources by hand.
  Pass `-- --south-basin` to regenerate the basin and cave sources and exports.
  Use `-- --export --south-basin` to re-export their edited Blender sources.

The title screen uses a render of the outpost.
  To refresh it after editing the scenery, run `blender --background -noaudio art/outpost.blend --python scripts/render_title.py`.

Blender 5.3 was used for the included editable sources in `art/`.
  Runtime exports are checked in under `assets/models/`, so Blender is not needed to play.

| Source / export | Purpose |
| --- | --- |
| `plutonian` | Shared skinned character with locomotion, spear and sword carry, two slash actions, `SpearThrust`, and `NemiDeath` |
| `outpost` | Ice shelf, habitats, waystones, cavern, beacon tower, distant sky details |
| `collision` | Simplified static geometry, hidden at runtime |
| `core` | Collectible beacon power core |
| `battery`, `scrap`, `methane_pouch` | Ground-based material pickups |
| `large_boulder`, `practice_dummy` | Carrying-limit test pickup and weapon practice target |
| `ice_worm`, `relay_roach` | Animated enemies with idle, crawl, bite, and death clips |
| `hushcap` | Rooted enemy with idle, spit, and collapse clips |
| `old_relay`, `old_relay_collision` | Old Relay scenery and its hidden collision geometry |
| `south_basin`, `south_basin_collision` | South Basin and Hushcap Caverns scenery and collision geometry |
| `worm_meat`, `roach_meat`, `mycelium` | Corpse loot pickup models |
| `field_armor`, `reinforced_armor`, `field_helmet`, `reinforced_helmet` | Equipment sources, each exporting pickup and `_worn` GLBs |
| `field_spear`, `trident` | Weapons sharing the spear animations, each exporting pickup and `_worn` GLBs |
| `shortsword` | One-handed weapon with pickup and `_worn` GLBs for either hand |

To re-export **your edited `.blend` sources** without regenerating them:

```sh
blender --background -noaudio --factory-startup --python-exit-code 1 --python scripts/assets.py -- --export
```

Append `--outpost` to export just the outpost and its collision scene.
Append `--shortsword` to export just the player rig and the two shortsword models.

To **rebuild all source art and exports from the procedural designs**, overwriting the editable `.blend` sources:

```sh
blender --background -noaudio --factory-startup --python-exit-code 1 --python scripts/assets.py
```

To regenerate only the three material models without replacing existing world or character art, pass `-- --items` to the asset command.
  This overwrites the material sources and exports.
  Pickup models use a base origin at zero height; material models use a 0.62 m square placement footprint and 0.6 m height.
  Equipment uses its larger per-item bounds from `ItemKind::gear()` in `src/equipment.rs`; `scripts/check_assets.py` verifies the exported geometry fits those bounds.

Pass `-- --meat` to regenerate the two meat pickup models.

Pass `-- --locomotion` to regenerate the player's backpedal, strafe, and diagonal actions in `art/plutonian.blend` and re-export `assets/models/plutonian.glb`.

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

Use `-- --export --nemi-death` to rebuild the collapse action on the existing shared character rig.
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

## Verification

```sh
cargo fmt --check
cargo check --locked
cargo clippy --locked --all-targets
cargo test --locked
cargo run --locked -- --smoke-test
cargo run --locked -- --smoke-world
cargo run --locked -- --smoke-nemi
cargo run --locked -- --smoke-cave
python3 scripts/check_assets.py
```

Clippy enforces a 150-line function limit, including smoke drivers.
  GitHub Actions builds, tests, and runs Clippy for pushes and pull requests to `master`.
  See the [repository guidelines](AGENTS.md) for development and verification standards.

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
  It also checks hatch unlocking, the relay encounters, roach loot and XP, archive discovery, return and reentry, fall recovery, and combat defeat with and without a bed save.
  In the archive it checks projectile cover, reads the log with the Hushcap alive, and uses a spear to defeat and loot it.
  It verifies journal reward text, completion XP for The Last Light and Old Relay, and restoration of saved XP after combat defeat.
  After leaving Old Relay it also reports the archive Hushcap to Nemi and checks the new quest's journal counters.
  It prints `WORLD SMOKE PASS` on success.
- `--smoke-nemi` checks spear damage, fleeing and returning, the failed quest entry, Nemi’s collapse and XP reward, and XP for opening the relay by completing Relight the Beacon.
  It prints `NEMI SMOKE PASS` on success.
- `--smoke-cave` walks from the starting tent to Hushcap Caverns and back to the outpost.
  It probes the hidden entrance, both middle-chamber passages, encounter grounding, and projectile cover.
  It also observes Hushcap spit and collects Mycelium through the corpse loot panel.
  It prints `CAVE SMOKE PASS` on success.

The checks save screenshots in `/tmp/plutonian-*.png`, including the arena, outpost, inventory, beacon, and weapon and armor poses.

Manual acceptance: walk and sprint, jump and land, traverse the trail rise, push against habitat/cavern walls, orbit the camera near obstructions, try interacting out of range or through a wall, read/dismiss each dialogue, collect the core once, return to Nemi, confirm the lit beacon, pause/resume using keys and buttons, fall off the ice shelf, pick up materials on the trail and in the cavern, use the inventory quantity field and buttons, drop and recollect stacks near slopes and walls, equip and swap both gear sets, unequip/drop/recollect gear, inspect its fit during animation, and choose **Start Game** after relaunching to confirm fresh quest, inventory, and equipment state.
  Automated checks cannot establish how comfortable the movement or composition feels.

## License

Licensed under the [MIT License](LICENSE).
