# Enemy Respawning Design

This document describes how disposable enemies return to the world after
the player kills them.
The decisions here were settled in issue #24
and implemented in issue #84.

## The rule

Disposable enemies respawn.
An area's disposable enemies are fully reset once six days have passed
with no disposable kill in that area.
The day counter advances by one when the player sleeps at the tent bed.
Respawning replaces bodies; it never erases records.

## Disposable and consequential actors

An actor is consequential if it is an individual,
and disposable if it is a population wearing one body.
Consequential actors never respawn,
and their saved state behaves exactly as it does today.

- Disposable: ice worms, relay roaches, and hushcaps.
  A replacement individual has moved in;
  nothing about the previous one is remembered in the world.
- Consequential: the outpost guards, Nemi, and other named NPCs.
  Their state, including guard hostility, is a story fact and is permanent.

Quest records are not bodies.
The cavern hushcaps' bodies are disposable,
but their defeat records are consequential:
a renewed cavern still counts its chambers as defeated,
and the journal never moves backwards.

Whether an encounter is disposable is declared with its data,
in the enemy definition or the encounter's catalog entry,
not hard-coded per spawn site.
The classification derives from the archetype in the placements catalog:
worms, roaches, and hushcaps are disposable;
guards and other named NPCs are not.

## The clock

The snapshot carries a day counter that starts at zero.
Sleeping at the tent bed advances it by one.
Sleep already creates the checkpoint and restores the player's health;
it now also passes a day.
There is no real-time clock,
and nothing respawns from play time alone.

## Renewal

Respawn state is tracked per area,
using the existing area identity:
the surface (which includes the caverns) and the Old Relay.
Each area records the day of its most recent disposable kill.
An area is due when the current day minus that day reaches six
(`RESPAWN_DAYS = 6`).

Renewal is evaluated at exactly two points:
when the world is built from a snapshot
(game start, save load, and death rollback),
and immediately after a sleep advances the day.
Both are masked transitions,
so nothing ever pops in during play.
The bed's own area renews on sleep like any other;
enemy homes are away from the bed,
and the sleep transition covers the change.

Renewal is a full reset of the area's disposable encounters.
Their saved state is discarded:
the dead return at their homes at full health,
the wounded are reset too,
and corpses, including any unlooted loot, are swept.
There are no wounded survivors.

Wounding an enemy without killing it does not start the timer
and does not move it.
A wounded enemy keeps its wounds until its area renews
because something in the area died.

## Rewards

Every kill of a disposable enemy drops its definition's loot
and awards XP, every time.
Loot is a renewable resource by design;
meat and mycelium are consumables.

Kill XP falls off as the player outlevels the enemy,
so the easiest loop stops paying.
Enemy definitions carry a `level`,
using the shared `Level` type:
ice worm 1, relay roach 1, hushcap 2, outpost guard 3.
The guard's level is never consulted
(consequential actors are exempt from falloff),
but the field is total,
so no definition is a special case.

`Level` is a general actor attribute,
while progression, earning XP, and advancing stay player-specific.
With `d = player level - enemy level`:

- `d <= 0`: full XP.
- Otherwise the award scales by `1 - d / FALLOFF_LEVELS`,
  with `FALLOFF_LEVELS = 4`:
  75% at `d = 1`, 50% at `d = 2`, 25% at `d = 3`, and zero from `d = 4`.
- The award is computed in integer arithmetic and floors.

For example, an ice worm's 35 XP pays 35 at player level 1,
17 at level 3, and nothing from level 5.
Level 2 costs 70 XP and each further level costs 50 more,
so a worm camp is a reasonable start
and a dead end a few levels later.

Quest XP is never reduced.
Consequential actors are not subject to falloff either:
they die once and award once at full value, as today,
under the existing contribution bookkeeping.
Disposable kills no longer consult or record encounter contributions.

Enemy stats are not scaled to the player, deliberately.
Authored stats stay authored.
An outleveled area becomes safe but unrewarding,
which is the intended pressure toward new areas.

## Why this is enough against farming

This is a single-player game,
so the goal is that farming is never the best strategy,
not that it is impossible.
Two bounds achieve that.
The falloff caps what any area can pay:
an area only pays in full inside its level band
and stops paying four levels above it,
so sleeping to re-clear an area yields a bounded, decaying return.
And renewal requires six days with no kill in the area,
so a clear is never immediately repeatable.
Progress, through quest XP and higher-band areas, always pays better.

## Save representation

The new state is the day counter
and one last-kill day per area.
Both are world-level state stored with the snapshot's world state,
not new per-encounter fields.
Adding them is a save format change,
so the save version bumps from v13 to v14
with the usual golden-fixture and schema updates.
Issue #62 established the world-state struct in v13;
the respawn state belongs in that struct.

Death rollback stays consistent by construction:
the day counter and the encounter states live in the same snapshot,
so rolling back restores the pair as of the checkpoint.
A renewal saved at the bed is part of that checkpoint.
Kills and other changes after the checkpoint are undone by dying.

## Out of scope

- Separate timers for cleared and visited areas; six days is flat.
- Per-encounter respawn timers.
- Enemy stat scaling and XP bonuses for above-level kills.
- Renewal triggered by wounding alone.
- Respawning for consequential actors, in any form.

## Implementation notes

Renewal is a rule for building the world from a snapshot,
plus a pass over the live world when a sleep advances the day.
A disposable kill records its area's last-kill day in the run state,
which the next bed save persists;
if the player dies first, rollback discards it consistently.

Tests should cover the day advancing on sleep,
the due boundary at five versus six days,
the full reset of dead and wounded enemies,
records surviving renewal
(cavern defeats, journal state, guard hostility),
the falloff table including the zero floor,
and rollback consistency.
Tuning lives in named constants (`RESPAWN_DAYS`, `FALLOFF_LEVELS`)
and in the enemy definitions' `level` fields,
so the numbers above can move without touching the mechanism.
