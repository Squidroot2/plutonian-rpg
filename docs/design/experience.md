# Experience and Leveling

Players start at level 1 with zero XP. Killable NPCs award XP when they die
after the player has damaged them. The player can contribute any amount of
actual damage and another source can finish the kill. The practice dummy is
an indestructible training target. Player defeat and quest completion carry
no XP reward.

| Actor | XP |
| --- | ---: |
| Ice worm | 35 |
| Relay roach | 25 |
| Keeper Nemi | 35 |

## Level pacing

The next level costs `70 + 50 × (current level − 1)` XP. Two worms on the
early route are enough to unlock the first attribute upgrades. The five
worms and one roach across the shipped map total 200 XP, reaching level 3
with 10 XP toward level 4. This pacing lets players reach level 3 through
hostile encounters alone. Later levels take more encounters as the player
builds up their attributes.

| Level reached | XP from previous level | Total XP |
| --- | ---: | ---: |
| 2 | 70 | 70 |
| 3 | 120 | 190 |
| 4 | 170 | 360 |
| 5 | 220 | 580 |

Carry surplus XP into the next level. A large award can cross multiple
thresholds and grants three [attribute points](attributes.md) for each
level reached. The Character tab shows the current level, XP toward the
next level, its threshold, and the unspent point balance. A three-second
gameplay notice shows XP from all credited kills in the same update. When
the award levels the player, the same notice shows the resulting level and
points gained.

## ECS and checkpoint lifecycle

- The player root owns `Experience` for XP toward the next level and
  `CharacterProgress` for level and unspent attribute points. The XP system
  advances both together. Character spending and capability synchronization
  run before attack detection.
- Each killable NPC defines a positive `ExperienceReward` alongside its
  health. The component holds the reward and records whether the player
  contributed damage during that actor's life. New enemy definitions supply
  their reward when spawning and use the shared damage and death systems.
- Shared combat emits `ActorDefeated` when it marks a new death. During
  `GameState::Playing`, the XP system records damage from player
  `ConfirmedHit` messages, then processes defeat messages. This ordering
  includes hits that landed earlier in the lethal update. The system consumes
  the actor's credit when it awards XP. Restoring a corpse only restores its
  death marker, so loading and looting produce no second reward.
- The cot snapshots XP after combat and level changes resolve. Bitcode v4
  stores XP toward the next level and contribution credit for living actors,
  keyed by encounter ID and Nemi's saved identity. Legacy saves start with
  zero XP toward their restored level and no contributions. V3 saves keep
  their levels, attribute ranks, and unspent points.
- Combat death restores XP and contribution credit along with the rest of
  the cot checkpoint. A player defeated before their first save starts from
  level 1 and zero XP in a fresh run. Fall recovery preserves live progress.

## Acceptance checks

- A zero-damage hit earns no credit. A hit followed by another source's
  lethal blow earns one reward, including when both hits resolve in the same
  update. Repeated hits on a dead actor yield zero XP.
- Exact thresholds, surplus XP, and multiple level gains grant the expected
  attribute points. The Character tab reflects XP changes between levels too.
- Checkpoint restoration preserves partial level progress and credit for
  living actors. Legacy saves load with their original progression and no
  XP credit, and saved corpses produce no XP or notice on load.
