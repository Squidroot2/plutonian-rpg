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

Field spear and trident ship with a 2.5 m reach and the two-handed spear animation profile.
The shortsword ships with a 1.4 m reach and the independent sword animations.
The item catalog supplies damage, reach, and contact radius per item.
An active swing holds the equipped item ID and attack stats for that hand.
See [Gameplay Data](gameplay-data.md) for authoring details.

## Enemy melee

Enemies use shared pursuit and melee behavior.
The enemy catalog supplies each variant’s movement speed, bite range, windup, recovery, and damage.
A bite is emitted once when its windup expires. 
For example, the roach bite deals 12 damage and the worm bite deals 20. 
Enemy attacks enter the same shared resolver as player attacks.

## Outpost guards

Two Plutonian guards stand watch at separate posts in Nivalis.
  They route around the keeper’s hut and regroup outside the barracks when the caravan arrives.
  One overlooks the Old Relay hatch and the other faces the ice trail out of the settlement.
  A confirmed hit from the player on Nemi or either guard makes both surviving guards hostile.
  Hostility is shared across the outpost and follows the checkpoint.
  A guard pursues within 12 m of the settlement center on the surface.
  Leaving that area cancels an active thrust and sends the guards back to their posts,
  where they resume watching their assigned approaches.
  Returning to the courtyard exposes the player to another attack.

Guards use the same equipment slots, swing timing, contact casts, and Strength scaling as the player.
  Their AI alternates usable hands when dual wielding.
  The shipped guard loadout uses Vitality 15 for 140 HP and reinforced gear for 8 defense.
  Strength 98 scales the Guard Polearm’s 30 base damage to 58.
  Each guard awards 100 XP and leaves the equipped main-hand weapon.
  Guard-inflicted player death uses the normal checkpoint reload.

Nemi and the caravan speakers use the same attribute formulas and equipment rules.
  Caravan speakers become damageable once parked.
  A surviving speaker flees, returns when safe, and resumes dialogue.
  Tovan’s inspection station is available if he dies before examining the five Mycelium samples.

## Enemy ranged attacks

Hushcaps take root and turn in place to aim.
The enemy catalog supplies their spit and projectile tuning.
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
