# Gameplay Data

Items, enemies, and placements use the same `data://` Bevy asset source as dialogue and quests.
The game validates the catalogs before it opens the title screen and checks item models while loading the world.
Edit the files directly and restart the game to pick up changes.

- `data/items/catalog.item.ron` contains each item's display text, weight in kg, pickup model, and pickup dimensions in meters.
- `data/enemies/catalog.enemy.ron` contains each enemy's health, XP, loot stack, and archetype-specific movement and attack tuning.
- `data/npcs/catalog.npc.ron` contains Plutonian NPC ranks, worn gear, and starting bags.
- `data/world/catalog.placement.ron` contains starter chest stock, pickups, and non-guard encounters.

Use stable IDs of up to 48 lowercase ASCII letters, digits, or underscores.
Start each ID with a letter.
Change the display name freely and preserve the ID when editing the same thing.
The order of the item definitions determines their order in inventory and storage.
Each catalog begins with the game's shipped values.

## Add a weapon and armor

Append entries inside the item catalog list.
These examples reuse shipped GLBs.
New GLBs can be referenced by paths relative to `assets/`.
Worn weapon models use the same grip orientation as their profile.
`Spear` uses both hands, and `Shortsword` can be used in either hand.
A profile determines the swing animation and impact frame.
Each entry supplies its own damage, reach, and contact radius.

```ron
(
    id: "relay_blade",
    name: "Relay blade",
    description: "A short blade made from salvaged relay alloy.",
    weight_kg: 2,
    pickup_model: "models/shortsword.glb",
    pickup_size_m: (0.26, 0.2, 0.9),
    kind: Weapon(
        profile: Shortsword,
        minimum_strength: 3,
        worn_model: "models/shortsword_worn.glb",
        stats: (damage: 11, reach_m: 1.5, contact_radius_m: 0.18),
    ),
),
(
    id: "relay_cuirass",
    name: "Relay cuirass",
    description: "Salvaged plates fitted over a field cuirass.",
    weight_kg: 6,
    pickup_model: "models/field_armor.glb",
    pickup_size_m: (0.86, 0.76, 0.8),
    kind: Armor(
        slot: Body,
        defense: 4,
        minimum_strength: 6,
        worn_model: "models/field_armor_worn.glb",
    ),
),
```

Use `Helmet` for head armor and `Material` for a stackable item.
The beacon core retains its quest interactions and cannot be placed as an ordinary pickup.

## Add an enemy

Append an entry inside the enemy catalog list.
This example uses the roach model, collider, animations, and pursuit behavior.
`Worm` uses the same bite fields.
`Hushcap` uses the spit fields shown in the shipped catalog.
`Guard` uses the existing outpost posts and specifies pursuit speed and XP.
The NPC catalog supplies each guard’s health, combat stats, defense, and weapon drop through the shared ranks and loadout.

```ron
(
    id: "relay_sentinel",
    name: "Relay sentinel",
    health: 55,
    xp: 50,
    loot: (item: "relay_blade", quantity: 1),
    behavior: Roach(
        speed_mps: 3.0,
        bite: (range_m: 1.7, windup_seconds: 0.5, recovery_seconds: 0.9, damage: 18),
    ),
),
```

## Edit Plutonian NPCs

Use the existing NPC’s ID to set their ranks, worn items, and starting bag.
Omitted ranks default to 5.
The equipment list uses `Body`, `Helmet`, `MainHand`, and `OffHand`.
Every item must meet the same slot, Strength, and handedness rules as the player.
Guards require a main-hand weapon and drop it when defeated.
The guard role supplies pursuit speed and XP.
Remove `health`, `loot`, and the old defense and thrust fields from custom guard role definitions when switching to this format.

```ron
(
    id: sella,
    attributes: (vitality: 15, strength: 98),
    equipment: [
        (slot: Body, item: "reinforced_armor"),
        (slot: Helmet, item: "reinforced_helmet"),
        (slot: MainHand, item: "guard_polearm"),
    ],
    inventory: [(item: "battery", quantity: 1)],
),
```

The other NPC IDs are `orren`, `nemi`, `isera`, `vey`, `tovan`, and `orla`.

## Place variants

Add stacks to the existing `Lamp`, `Supply`, or `Gear` entry in `starter_stock`.
The contents of saved chests come from the checkpoint.
To place the examples directly, add these entries to the corresponding lists.

```ron
pickups: [
    (id: "relay_cuirass_cache", area: Surface, position: (5.5, 2.0, 3.0),
     stack: (item: "relay_cuirass", quantity: 1)),
],
encounters: [
    (id: "relay_sentinel_patrol", enemy: "relay_sentinel", area: OldRelay,
     position: (7.0, -8.8, -16.0), yaw_degrees: 180.0),
],
```

Positions are world coordinates in meters, including for `OldRelay`.
Place pickups and moving enemies just above traversable ground so their grounding probes can find the floor.
Place stationary Hushcaps at floor height.
Yaw is optional and defaults to zero.

Edit an encounter entry to change its enemy definition or position.
Guard posts remain scripted and reserve the `guard_west` and `guard_east` IDs.
Cavern quest credit follows the Hushcap encounter IDs declared in `src/quest.rs`;
keep those IDs and their Mycelium drops when editing quest encounters.

## Checkpoints

Checkpoints look up items and encounters by ID.
Previously collected authored pickups stay collected.
Uncollected authored pickups use their current entries, and newly added encounters start fresh.
Removed encounter placements disappear, including their pending XP contribution credit.
A saved item still in the bag, equipped, in a chest, or on the ground needs its catalog definition to load.
See [Save and Recovery Design](saves.md) for how checkpoints handle format and balance changes.
