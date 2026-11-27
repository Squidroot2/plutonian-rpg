# Ranged weapons

Ranged weapons give the player a slow,
deliberate way to deal heavy damage at distance,
paid for in ammunition,
reload time,
and mobility.
The first ranged weapons are the methane musket,
a two-handed single-shot weapon that fires one methane pouch per shot,
and the scatter pistol,
a one-handed weapon that fires a cone of crystal shards.
This document settles the attribute,
the aiming model,
and the ammunition economy that later ranged weapons build on.

## The Marksmanship attribute

Ranged weapons are governed by a new attribute,
Marksmanship,
which mirrors Strength's relationship to melee weapons.

- A ranged weapon's catalog entry carries a minimum Marksmanship,
  enforced at equip the same way `minimum_strength` is enforced for gear,
  including the inventory warning flow.
- Marksmanship derives a ranged damage multiplier from the rank
  using the same curve as the melee multiplier:
  `95 + rank` percent.
  At the default rank of 5 the multiplier is 100%,
  and the character menu displays it as "% ranged damage"
  beside Strength's "% melee damage".
- The fiction for the damage increase is shot placement:
  a better marksman hits more vital areas.
  The weapon's power does not change —
  the projectile is no faster,
  no larger,
  and flies no farther.
  Marksmanship therefore does not affect spread,
  projectile speed,
  range,
  or reload time.

Adding an attribute changes the saved attribute ranks,
so the implementing change bumps the save version
and refreshes the inspector fixtures and golden saves.
No migration:
old saves are rejected,
per the standing save rule.
NPC definitions,
which already carry per-attribute values,
gain a Marksmanship value the same way.

## The methane musket

The first ranged weapon is a two-handed musket
that burns condensed methane as its propellant.
Its ammunition already exists:
the methane pouch,
catalogued as "a reinforced pouch of condensed methane."
The pouch is treated as a self-contained cartridge —
charge and slug in one package,
the way a historical musket's paper cartridge held powder and ball —
so one pouch is consumed per shot
and no second ammunition item exists.

Starting values,
to be tuned by feel after play:

| Property | Value |
| --- | --- |
| Base damage | 30 |
| Minimum Marksmanship | 1 |
| Reload duration | 2.5 s at base Agility |
| Projectile speed | 60 m/s |
| Maximum range | 45 m |
| Hip-fire spread | ±4° |
| Ammunition | 1 methane pouch per shot, 1 kg each |

Minimum Marksmanship 1 mirrors the field spear's minimum Strength 1:
the first ranged weapon is effectively ungated,
so every player can try the system.
Later ranged weapons are where meaningful minimums belong,
the way the trident's minimum Strength 8 gates it above the spear.

## The scatter pistol

The second ranged weapon is a one-handed pistol
that spends the same methane pouch charge
to throw a cone of crystal shards.
One ammunition type serves the whole ranged family:
the pouch is the standard cartridge,
and the supply and carry-weight economy stays single-track.

Starting values,
to be tuned by feel after play:

| Property | Value |
| --- | --- |
| Pellets per shot | 6 |
| Damage per pellet | 6 |
| Minimum Marksmanship | 1 |
| Reload duration | 1.5 s at base Agility |
| Pellet speed | 60 m/s |
| Maximum range | 12 m |
| Hip-fire spread | ±10° |
| Aimed spread | ±4° |
| Ammunition | 1 methane pouch per shot |

Each pellet is its own projectile with its own spread roll,
and the Marksmanship multiplier applies per pellet.
If every pellet lands at point blank the shot deals 36 —
more than the musket's 30 —
but the spread makes a full hit unlikely past a few meters,
which is the pistol's identity:
devastating close,
wasteful far.

The pistol is also the design's one-handed test case.
It can be dual wielded,
fired from the off hand,
and swapped loaded-for-loaded under the loaded-state rules —
none of which the two-handed musket can exercise.

## The firing cycle

All ranged weapons in this design are single-shot.
The cycle is aim,
fire,
recover,
reload —
every shot,
with no magazines or fire modes.
A multi-shot weapon is a future design extension,
not a variant this document absorbs.

- Firing requires the weapon drawn,
  as with melee attacks.
- After a shot,
  the weapon is empty.
  Pressing Attack while empty starts the reload,
  which consumes one methane pouch from the bag on completion.
  A reload with no pouches in the bag fails with the same feedback
  as any other impossible action.
- Dodging or holstering cancels a reload in progress,
  and a cancelled reload leaves the weapon empty.

## Loaded state

Loaded state belongs to the weapon,
not to the play session.
For every stack of a ranged weapon,
the number of loaded weapons in the stack is recorded alongside
the quantity —
in the bag,
in equipment,
in chests and the weapon rack,
and in the save file.
Putting a loaded weapon back in the inventory keeps it loaded;
drawing it again later,
or loading the game,
brings it back exactly as it was left.
Recording loaded counts in item stacks is part of the save change
described above.

This makes carrying several loaded ranged weapons a real tactic:
fire one,
put it away,
and draw another already loaded instead of reloading —
the pirate's answer to a slow reload.
That play is intended,
not an exploit.
Its counterweights are exposure and time:
the equipment menu is not a pause menu,
so swapping weapons mid-fight happens in real time
with enemies still moving.
If swapping still proves too strong,
draw and holster time is the lever to add —
a weapon would take time to ready after being drawn,
naturally scaled by weapon speed (issue #49).
Draw time is deliberately not in this design;
it is named here as the future fix,
to be added only if the feel demands it.

## Aiming

Holding the Off-hand attack control aims the equipped ranged weapon.
A two-handed ranged weapon has no off-hand attack,
so the control is free to mean aim;
melee weapons keep their off-hand attacks untouched.

Aiming is a property of the player's hands,
not of the weapon:
the player can aim only while the off hand is not holding a second weapon.
The musket is two-handed and is always aimable.
The scatter pistol is aimable while wielded alone,
and cannot be aimed while dual wielding —
with a weapon in each hand,
both are hip-fired,
the off-hand weapon through the Off-hand attack control.

While aiming:

- The camera tightens to an over-the-shoulder view
  with a modest field-of-view squeeze,
  and a crosshair is shown.
- The shot takes the weapon's aimed spread.
  Aiming narrows spread;
  whether it eliminates it is a property of the weapon.
  The musket's aimed spread is zero —
  the shot goes where the crosshair sits —
  while the scatter pistol keeps a narrowed cone.
- Movement is a dedicated aim speed,
  slower than walking.
  Sprinting and dodging are unavailable.

Hip fire is always available while the weapon is drawn and loaded.
A hip-fired shot takes the weapon's full hip-fire spread,
has no crosshair or camera change,
and is otherwise the same shot —
same damage,
same pouch,
same reload after.

## Agility and weapon speed

Weapon speed (issue #49) applies to ranged weapons
through the same derived multiplier it applies to melee weapons.
For a ranged weapon it scales:

- the reload duration,
- the post-fire recovery before another action can begin,
- and the reload animation's playback rate,
  so the animation and the timer never disagree.

Marksmanship decides how much a hit hurts and what may be equipped;
Agility decides how fast the weapon cycles;
Endurance still owns stamina and the carry capacity
that hauling 1 kg pouches draws on.

## Ammunition supply

Making the methane pouch ammunition creates a supply requirement:
pouches cannot remain finite placed loot only,
or the musket is a sometimes-toy.
The design requires a renewable source.
Hushcaps are the thematic source —
they are the methane cavern's creatures —
so hushcaps drop methane pouches as loot,
in addition to the pouches already placed in chests.
Drop quantities are a balance decision for the implementing change.

## Acquisition

The first musket is found in the Barracks.
It is the guards' weapon,
it gives the weapon rack a reason to hold one,
and a stock of methane pouches placed nearby teaches the ammunition
economy in the same room.
The first scatter pistol is in the Supply chest by the starting tent,
next to the Lamp chest's methane pouches,
so the one-handed system is available from the start of the game.

## Balance anchors

Current melee weapons deal 7–10 damage per hit
(shortsword 7,
field spear and trident 10).
Current enemy health:
relay roach 20,
ice worm 30,
hushcap 42.

At base damage 30 and the default Marksmanship rank:

- one shot kills a relay roach or an ice worm;
- a hushcap survives at 12 health —
  a second shot,
  or close in and finish it with a melee weapon
  while the reload runs;
- at Marksmanship rank 45 the multiplier reaches 140%
  and one shot kills a hushcap exactly.

For the scatter pistol,
a full point-blank hit deals 36 —
enough for a roach or a worm,
leaving a hushcap at 6 —
but each pellet that misses to spread is 6 damage gone,
so its expected damage falls off with distance
through geometry rather than a falloff rule.

There is no damage falloff in this design.
Falloff is the first tuning lever to reach for
if the musket proves too strong at range;
it is deliberately left out of the first implementation
and added later only if the feel demands it.

## Consequences for the follow-on work

- Art (issue #128) refreshes against this document:
  the musket's and scatter pistol's pickup and worn models,
  fire and reload clips under the clip-name contract,
  the slug and shard projectiles and a methane muzzle flash,
  and display on the weapon rack.
  Before-and-after screenshots are the acceptance evidence.
- Coding (issue #129) refreshes against this document:
  the Marksmanship attribute and its save bump,
  loaded counts in item stacks wherever stacks are saved,
  the ranged weapon profile in the item catalog
  (damage,
  pellets per shot,
  minimum Marksmanship,
  reload duration,
  projectile speed and travel,
  hip-fire and aimed spread),
  aiming and hip fire,
  multi-pellet firing,
  the projectile implementation following the enemy spit pattern
  in `src/enemy/ranged.rs`,
  hushcap pouch drops,
  the Barracks and Supply chest placements,
  and the player guide's ranged weapons section.

## Out of scope

- Further ranged weapon variants beyond the musket and scatter pistol.
- Multi-shot weapons,
  magazines,
  and fire modes.
- Damage falloff (named above as a future tuning lever).
- Draw and holster time (named above as the future counterweight
  to loaded-weapon swapping).
- Enemies operating ranged weapons;
  guards keep their polearms.
- A weak-point or critical-hit system.
  The vital-areas fiction leaves the door open,
  but nothing in this design depends on it.
