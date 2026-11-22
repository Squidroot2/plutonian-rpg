# Combat

## Overview

Combat separates attack detection from shared resolution. 
Attack systems emit `Hit` messages during `CombatSystems::Detect`; `CombatSystems::Resolve` applies damage, reports accepted hits, and marks defeated actors. 
This lets player weapons and enemy attacks share defense, hit protection, death, reaction, and downstream quest/experience behavior.

## Player melee

The player can attack independently with either equipped hand.
  The weapon must remain equipped for its swing to continue.
  Attacks start only while grounded, drawn, outside a dodge, and with menus/dialogue not blocking input. 
A sphere shape cast checks contact at the weapon's impact frame.
  Only the first contact at impact produces a hit.

Player weapon damage is scaled by the Strength derived multiplier before a `Hit` is emitted.
  The multiplier is `(95 + Strength rank)%`; rank 5 gives 100%, and ranks 1 through 10 give 96% through 105%.

## Melee types

- Spear: 2.5m reach
- Shortsword: 1.4 m reach

## Enemy melee

Enemies use shared pursuit and melee behavior configured with species-specific bite range, windup, recovery, and damage. 
A bite is emitted once when its windup expires. 
For example, the roach bite deals 12 damage and the worm bite deals 20. 
Enemy attacks enter the same shared resolver as player attacks.

## Enemy ranged attacks

Hushcaps take root and turn in place to aim.
  One grows beside a cracked archive conduit in Old Relay.
  Hushcap Caverns groups several across three chambers,
  with rock cover between firing positions.
  Each has 42 health and deals 24 base damage per spit.
  A target within 8 m and visible from the vent starts a 0.85 s windup,
  with a contracting cap and a brightening glow.
  Aim locks at the start of the windup so a sidestep or solid cover can break the shot's path.
The projectile sweeps its radius against colliders at 9 m/s for up to 10 m.
  It stops at first contact and submits damage through the shared resolver only when it hits the player.
  The Hushcap recovers for 1.25 s between releasing a shot and lining up the next one.
  Dialogue and paused gameplay suspend attacks and flight.
  Defeating a Hushcap clears its projectiles.
  Crossing between Old Relay and the surface clears projectiles in the area the player left.
  The corpse yields one Mycelium through the shared loot panel.

## Damage resolution

For a hit against an actor with health, resolution rejects it if the damage is zero, the target is already at zero health, the target's hit-protection window is active, or the source is already dead.
  Otherwise defense is subtracted with saturation, but a valid hit always deals at least one point:

`applied damage = max(1, saturating_sub(incoming damage, defense))`

Health is reduced by that amount and saturates at zero.
  A hit-protection timer, when present, is reset to its configured duration after damage.
  The player has a 0.75 second protection window.
  Targets without health accept no ordinary hit; the healthless training target is a deliberate exception that confirms contact without damage.
  Confirmed hits drive reactions, recovery delay, combat feedback, quest responses, and experience credit.

## Death and recovery

Changed health at zero marks an actor `Dead` and emits `ActorDefeated`.
  Components opt actors into collider removal and optional corpse lifetime.
  Player health recovery waits five seconds after a confirmed damaging hit, then restores fractional-rate health in whole points; any new hit restarts the delay and clears fractional progress.
  Only the first contact at impact produces a hit.
protection timers advance in fixed updates while playing and outside dialogue.
  Death and hit reactions, collider release, corpse expiry, and presentation are handled by their owning systems.

## System order

Player and enemy attack detection runs before shared hit resolution.
  Resolution then delays health recovery, marks deaths, releases opted-in colliders, and updates reactions.
  Systems such as experience and quest reactions consume confirmed hits or defeat messages after resolution.
