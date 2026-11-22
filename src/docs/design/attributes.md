# Character Attributes

## Progression

Each attribute has ranks **1–100**. New characters start at rank **5** in every
attribute. Rank 5 reproduces the current health, stamina, melee damage, movement
costs, and carrying capacity. Future character creation can set different
starting ranks within the same bounds.

The player starts at character level 1 with zero unspent points. Each level
gained grants **3 attribute points** to a shared pool. One point raises one
attribute by one rank. Players can bank points or spend them in the **Character**
tab, which shows their level, unspent points, and the current and next rank's
effects for each attribute. Each spend takes effect immediately. A separate
design will define XP sources, thresholds, and level pacing.

## Scaling

In each formula, `rank` is the rank of the attribute on that row.

| Attribute effect | Formula | Rank 1 | Rank 5 | Rank 100 |
| --- | --- | ---: | ---: | ---: |
| **Vitality** · Maximum health | `80 + 4 × rank` | 84 | 100 | 480 |
| **Vitality** · Health recovery per second | `10% × maximum health` | 8.4 | 10 | 48 |
| **Endurance** · Maximum stamina | `80 + 4 × rank` | 84 | 100 | 480 |
| **Endurance** · Full stamina recovery per second | `25% × maximum stamina` | 21 | 25 | 120 |
| **Endurance** · Carrying capacity | `55 + rank` kg | 56 kg | 60 kg | 155 kg |
| **Strength** · Melee weapon damage | `(95 + rank)% × base damage` | 96% | 100% | 195% |

**Vitality** sets the passive healing rate after the existing five-second wait
following damage. Accumulate fractional healing and apply it in whole HP.
Taking damage restarts the wait and clears the accumulated healing. Recovery
applies to living characters up to their maximum health.

**Endurance** grants full stamina recovery while eligible and stationary and
half that rate while walking. Sprinting costs 14 stamina per second. Carried
weight includes equipped gear. The player can run, sprint, jump, and dodge at
or below carrying capacity. Above capacity, the player can only walk, up
through `floor(1.2 × capacity)` kg. Exceeding that second threshold prevents
movement. Movement, attack input, and the inventory display use the same
derived limits.

**Strength** scales each connecting melee weapon hit, including each hand when
dual wielding. Round to the nearest whole point with halves rounding up, then
apply the target's defense.

Raising maximum health or stamina preserves the current amount. The HUD uses
the new maximum immediately, and regular recovery fills the additional capacity.

### Agility

Use `base cost × ln(105 / (rank + 5)) / ln(10.5)`, with base costs of **20
stamina for a jump** and **25 for a dodge**. The curve gives larger stamina
savings per rank early on and smaller savings near the cap. The final rank
removes the entire remaining cost, about 0.08 stamina for a jump and 0.10 for a
dodge.

| Agility rank | Jump cost | Dodge cost |
| --- | ---: | ---: |
| 1 | 24.34 | 30.43 |
| 5 | 20.00 | 25.00 |
| 25 | 10.66 | 13.32 |
| 50 | 5.50 | 6.87 |
| 99 | 0.08 | 0.10 |
| **100** | **0.00** | **0.00** |

Keep full precision for affordability and spending, and display costs to two
decimal places. Use the same derived dodge cost for attack priority and the
actual charge. Charge once per successful action. **Only rank 100** allows a
jump or dodge at zero stamina, subject to the usual action requirements.

## Strength requirements

Each melee weapon or armor piece has a minimum Strength rank. The initial
requirements are **7** for the Reinforced Helmet, **8** for the Trident, and
**10** for Reinforced Armor. The remaining shipped gear requires rank 1.

Show the requirement on gear details and disable equip actions below it.
Enforce the same check on both quick-equip and slot-specific requests before
moving any items. Characters can carry gear they have yet to qualify for.

## Data, saves, and extension

- Keep ranks, character level, and unspent points on the player entity. Grant
  points for each newly reached level and record the new level in the same
  operation. Repeated or older level notifications award zero points. Check
  the point balance and rank cap when applying each spend.
- Define attributes centrally with stable IDs, default ranks, display text,
  and tuning. Gameplay systems and Character tab previews use the same
  derived values. To add an attribute, register its definition and connect
  its effects to the systems that use them.
- Store actual ranks by stable ID alongside character level and unspent points
  in the next bitcode bed-save version. Missing ranks use their configured
  defaults. Migrate v1 and v2 saves to level 1, ranks of 5, and zero unspent
  points. Return newly ineligible equipped items to the backpack through
  checked transfers. If a transfer fails, report the load error and preserve
  the original save file.
- Bed saves checkpoint ranks and points. Combat death restores that checkpoint,
  while fall recovery preserves current allocations. Initialize health and
  stamina at their restored maxima when loading a checkpoint.

## Acceptance checks

- Verify rank 1, 5, and 100 values, recovery timing, live HUD maxima, and
  encumbrance boundaries at multiple Endurance ranks.
- Verify both Agility costs decrease through rank 100, stay positive at ranks
  1–99, and allow actions at zero stamina only at rank 100.
- Verify damage scaling per hand before armor, equipment gating through both
  equip paths, and inventory preservation when an equip attempt fails.
- Verify level awards, spending, banking, save round-trips, legacy gear
  migration, combat death rollback, and fall recovery.
