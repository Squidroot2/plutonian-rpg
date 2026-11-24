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
See the [controls guide](docs/guide/controls.md) for the default controls and rebinding instructions.

The first build compiles Bevy and its renderer and can take several minutes.
  Run from the repository root so the game can find `assets/` and the quest catalog in `data/`. `Cargo.lock` pins the dependency versions used for this application.

Choose **Start Game** on the title screen to begin a fresh run in Nivalis, or **Load Game** to resume your last bed save.
  See the [controls guide](docs/guide/controls.md) for how to navigate menus and play.

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
  Choose **I'm here to help** when speaking to Nemi to get directions.
  With the core in your backpack, tell Nemi **I found the beacon core** to hand it over and relight the beacon.
  You can also ask about Nemi or Nivalis. Click a topic, or use Menu Up and Menu Down and press Enter.
  You can continue exploring afterward.

Blue-ice cliffs and ceramic walls surround Nivalis and the South Basin shelf.
  The southwest gate opens during Contain the Hushcaps, revealing the Listening Flats.

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

The main quest line continues from The Last Light through Old Relay to the crossing caravan in Contain the Hushcaps.
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

Two heavily armored Plutonian guards keep watch over Nivalis.
  One watches the Old Relay hatch and the other faces the ice trail out of the settlement.
  Landing a hit on Nemi or either guard provokes both surviving guards.
  They defend the outpost and stay hostile if you escape and return.
  Two clean trident thrusts can defeat a starting character.
  If they kill you, the game reloads your last bed save or starts a fresh run if you have never saved.

A landed attack makes Nemi flee and fails The Last Light if it is active.
  A surviving Nemi returns after the player backs off, then refuses to help.
  Follow **Relight the Beacon** to use the charged core at the tower base.
  If you already restored the beacon with Nemi, attacking them leaves the beacon powered.

Quest definitions live in `data/quests/catalog.quest.ron`.
  Add a definition with a unique ID, a positive `completion_xp` reward, activation rules, named objectives, and conditional transitions.
  Definitions can use the catalog’s existing world actions and quest-completion conditions.
  The game loads the catalog when you start or load a run.
  See [Quest System](docs/design/quests.md).

## Gameplay Data

Items, equipment, enemy variants, and new world placements can be authored in RON.
The game reads these catalogs at startup.
See [Gameplay Data](docs/design/gameplay-data.md) for examples and checkpoint behavior.

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

Ask Nemi about the Old Relay after inspecting the sealed hatch or restoring the beacon.
  Once the beacon is lit, use E at the round hatch northwest of the tower to enter.
  Follow the amber lamps to the maintenance switch, then go through the raised gate to discover the archive survey log.
  An ice worm and a fast, biting relay roach patrol the relay.
  In the archive, a rooted Hushcap spits from beside a cracked conduit.
  The service console offers cover, and the survey log can be read while the Hushcap is alive.
  A defeated Hushcap yields one Mycelium.
  Use the hatch in the vestibule to return to Nivalis.
  Gate progress and the discovery are included in bed saves.

## Contain the Hushcaps

Restore the beacon, read the survey log, and return to the surface through the Relay hatch.
  Ask Nemi about the creature in the archive, or use the warning in the survey log if Nemi is unavailable.
  Head south into South Basin and follow the ridge to Hushcap Caverns.
  Defeat five distinct cavern Hushcaps after the quest starts.
  Each Hushcap yields one Mycelium.
  The archive specimen can supply a sample.

The fifth credited defeat raises the southwest gate.
  Approach the gate to see the crossing families file in with two vellik-drawn wagons.
  When the wagons park, speak to Tovan beside the rear wagon with five Mycelium in your backpack.
  Tovan examines the samples and returns them.
  The road beyond the gate loops through the Listening Flats, past the bell cairn and an ice overlook.

## License

Licensed under the [MIT License](LICENSE).
