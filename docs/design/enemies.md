# Enemies

## Purpose and roles

The ice worm, Relay roach, and Hushcap give encounters different pressures using a common combat and death lifecycle.
Worms and roaches close distance and bite; the rooted Hushcap makes the player read a ranged windup and use movement or cover.
Their different speeds, durability, damage, and attack timings make the same shared rules produce distinct encounters.

| Enemy | Health | XP | Loot | Movement and attack |
| --- | ---: | ---: | --- | --- |
| Ice worm | 30 | 35 | 1 worm meat | 2.4 m/s; 20 damage bite, 1.5 m range, 0.55 s windup, 0.85 s recovery |
| Relay roach | 20 | 25 | 1 roach meat | 3.5 m/s; 12 damage bite, 1.5 m range, 0.4 s windup, 0.8 s recovery |
| Hushcap | 42 | 45 | 1 Mycelium | Stationary; 24 damage spit, 0.85 s windup, 1.25 s recovery |

These are the shipped values in `data/enemies/catalog.enemy.ron`.
Damage is the input to shared combat resolution, so defense and hit protection affect the result.

## Melee pursuit

Worms and roaches share ground movement and melee decisions in `src/enemy.rs`.
They see the player within 7 m when a clear line of sight exists and both are within 10 m of the enemy's home.
They pursue within the same area, turn toward the player, and begin a bite when in range.
At the end of the windup, the bite hits only if the player is still in range, within 35 degrees of the enemy's forward direction, and visible from the bite origin.
It then recovers before pursuing again.

When pursuit conditions fail, the enemy returns home and must arrive before reacquiring the player.
Movement uses collision-aware sliding and ground probes to avoid unsupported or excessively steep steps.
AI and movement run in fixed updates during gameplay outside dialogue.
The worm's slower, stronger bite contrasts with the faster, more fragile roach.

## Hushcap spit

Hushcaps turn in place instead of pursuing.
A visible player within 8 m triggers a windup: the cap contracts and its glow brightens.
The Hushcap locks its aim at the start, giving the player time to sidestep or move behind cover.
The projectile travels at 9 m/s for up to 10 m, sweeping a 0.14 m radius and stopping at the first collision.
It emits a hit when it contacts the player.
Attacks and projectiles pause outside active gameplay or during dialogue; defeating the owner or leaving its area clears its projectiles.
The ranged implementation lives in `src/enemy/ranged.rs`, with the model and collider in `src/hushcap.rs`.

## Encounter design

Surface worms shelter toward the ice escarpments,
leaving the marked waystone route clear in keeping with Orren's description of a safe path.
West of the trail's midpoint,
behind a screen of escarpments and a curving western cliff cove,
a sunken worm nest holds ten ice worms around five burrow mounds and a lost scout cache.
Its pack density overwhelms low-level explorers,
while a prepared player can pull worms around cover to claim the Survey pike and salvage supplies.
Old Relay contains a worm at the junction, a roach, and a Hushcap beside the archive conduit.
The archive Hushcap replaced a worm, but defeating it is not required to reach the survey log.
Hushcap Caverns combines rooted firing positions with worms and roaches across three chambers; rocks provide cover from spit.
The cavern is part of the surface combat area, while Old Relay is a separate area.

The quest *Contain the Hushcaps* asks for five cavern Hushcap defeats and five Mycelium samples.
Eligible early defeats count once the objective is reached; the archive Hushcap is separate from the cavern count.
See `data/quests/catalog.quest.ron` for the objective transitions and `src/cave.rs` for encounter identities and defeat tracking.

## Shared lifecycle and authoring

Species spawn through shared enemy helpers with health, hit reactions, loot, experience reward, death collider removal, and animation ownership.
Worm and roach add ground pursuit; Hushcap adds ranged attack capabilities.
This capability-based structure lets future enemy types reuse the lifecycle without requiring every enemy to move or attack alike.
Scenes bind animation players when ready; each model supplies idle, attack, death, and optionally locomotion clips.
The ice worm's jaws and inner mouth fade to the color of the mouth cavity as its death clip plays,
giving the player a visible death cue for an enemy that already moves along the ground.

Attacks emit `Hit` into the shared combat resolver, which applies damage and emits confirmed hits and defeat events.
The player earns XP when they contributed damage to an enemy that is defeated, even if another source lands the final hit.
Unlooted corpses remain available through the Take/Close loot panel; after successful looting they shrink and despawn.
Bed save data preserves encounter state and loot for rollback, including whether the player has contributed toward an XP reward.
See [Combat](combat.md), [Gameplay Data](gameplay-data.md), and [Save and Recovery Design](saves.md) for the related systems and authoring rules.
