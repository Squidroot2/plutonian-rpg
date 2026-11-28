# Shields

A shield is off-hand gear for one-handed weapons.
While the player holds the block input,
the shield is raised:
its defense joins the player's total,
blocked hits cost stamina instead of health,
and the player trades away dodging, jumping,
attacking, and most of their movement speed
for as long as the shield stays up.

Blocking is a stance, not a stat.
Everything below follows from that:
the defense is conditional,
the costs are paid per hit,
and lowering the shield is the recovery.

## Pairing

A shield occupies the off-hand equipment slot.
It therefore pairs with one-handed weapons only:

- A two-handed weapon needs both hands,
  so it excludes a shield.
- Dual wielding puts a weapon in the off hand,
  so it excludes a shield.
- A one-handed weapon — melee or the scatter
  pistol — leaves the off hand free for one.

Balance note, recorded as a direction only:
two-handed weapons should compensate for giving
up the shield with longer reach.
No weapon's reach changes as part of this work;
the principle is documented here so a future
balance pass can act on it.

## The block input

Holding the Off-hand attack control
(right click by default) blocks,
for as long as it is held.
Blocking is a hold, not a toggle,
the same way aiming is a hold.

What the input means depends on the off hand:

- Off hand holds a shield: the input blocks.
  There is no off-hand weapon,
  so nothing is given up.
- Off hand holds a weapon: the input is the
  off-hand attack, as today.
- Off hand is empty and a ranged weapon is
  drawn: the input aims, per the ranged-weapons
  design.
  That design's rule is generalised here:
  aiming requires an *empty* off hand.
  A shield occupies the hand,
  so a shield cannot be aimed behind —
  the scatter pistol paired with a shield is a
  hip-fire-only weapon that can block between
  shots.

Blocking requires the weapons drawn,
like attacking.
A holstered shield does not block,
and the block input does not draw it.

## What blocking does to a hit

Blocking reuses the existing defense mechanic.
Combat already scales and rounds damage,
then applies the target's defense,
for the player and for NPCs alike.
A shield changes no formula:

- The shield's defense counts toward the
  wearer's total **only while blocking**.
  Body and helmet armor always count;
  the shield is held up, not worn,
  so its contribution is conditional.
- A blocked hit therefore resolves exactly
  like any other hit, against the higher
  defense.
  Chip damage needs no rule of its own:
  a hit stronger than the total defense gets
  the remainder through, as with armor today.
  The existing floor still applies:
  a valid blocked hit deals at least one
  point, like every valid hit.
- Blocking works against hits arriving from
  the front hemisphere —
  the half of the world the player is facing.
  A hit from behind resolves against the
  normal, unblocked defense.
  Facing is a live choice while blocking,
  which is what the movement rules below are
  for.
- Projectiles block like melee hits.
  Enemy spit is the shipped case;
  a shield that did not stop it would feel
  broken in this game specifically.

The known limit of flat defense is accepted:
it cannot express a percentage reduction,
so a future high-damage enemy tier would make
it feel thin.
The escape hatch, if that day comes,
is a block-specific reduction field on the
shield's gear profile —
the same role damage falloff plays as the
named future lever for ranged weapons.
It is not built now.

## Stamina

Being attacked while blocking reduces stamina.
Holding the shield up is free;
only blocked hits cost.

- The cost scales with the incoming hit,
  measured **before** defense:
  the full force of the blow is what the arm
  behind the shield absorbs,
  and costing the post-defense remainder would
  let defense discount the same hit twice.
  `cost = incoming damage × 0.5`,
  before the Strength reduction below.
- Strength reduces the cost,
  through a multiplier on the same curve the
  damage multipliers use, inverted:
  `100 / (95 + rank)`.
  Rank 5 pays full price;
  rank 100 pays about half.
  Strength is the hold-the-line stat,
  and Endurance still matters alongside it —
  it sets the size of the pool being spent.
  This is deliberately not the Agility
  movement-cost curve that discounts jump and
  dodge costs: that curve reaches zero at
  rank 100, and blocking must never become
  free — a zero-cost block would remove the
  guard break entirely.
  The Strength multiplier bottoms out near
  half price instead.
- Stamina does not recover while the block is
  held.
  Lowering the shield is the recovery decision;
  recovery while turtling would let a patient
  enemy be blocked forever.
- **Guard break:** when a blocked hit costs
  more than the stamina remaining,
  the stamina empties,
  the block is forced down,
  and the player is staggered for a short
  duration (a named constant, ~0.8 s)
  during which they cannot block, attack,
  or dodge.
  The breaking hit resolves against the
  normal defense, not the blocking defense.
  Running dry in front of a heavy enemy is
  meant to be a real event.

## Movement and restrictions while blocking

While the shield is up:

- No dodging, no jumping, no attacking.
  Attacks are refused, not treated as
  "attack cancels the block":
  the player releases the block input,
  then swings.
  That block → release → swing → re-block
  rhythm is the intended feel.
- No sprinting.
  Movement in any direction is capped at a
  dedicated blocking speed —
  its own named constant,
  slower than walking
  (a starting point of 55–60% of walk speed,
  tuned by feel),
  separate from the aiming speed constant
  because the stances tune independently.
  Retreating while facing the enemy is the
  core use, so direction is free;
  only pace is capped.
- Raising and lowering the shield are timed
  transitions (starting points ~0.25 s up,
  ~0.2 s down, tuned by feel),
  scaled by the Agility weapon-speed
  multiplier from issue #49 —
  raising a shield is weapon handling.
  The blocking defense applies only once the
  raise completes;
  during the raise the player is not yet
  protected.
- Interactions while blocking are
  deliberately not given a rule in this
  design.
  Blocking does not drop for an interaction,
  and no interaction is refused for blocking.
  Revisit if it feels wrong in play.

## The first shield

One shield ships with this design:
the **carapace shield**,
a plate cut from a hushcap shell.
The hushcap is already the settlement's
resource animal —
its methane pouches are the ranged weapons'
ammunition —
and a chitin plate fits the scavenged-materials
register of field armor and field spears.

Catalog shape: no new item type.
A shield is a gear entry whose slot is the
off hand;
the gear definition's existing fields
(`defense`, `minimum_strength`, `worn_model`)
are everything it needs,
and the slot carries the semantics —
gear in the off hand is a shield,
mutually exclusive with an off-hand weapon
because it is the same slot.

Starting values, all balance-tunable:

- Defense 8 while blocking —
  double the best shipped armor,
  against armor's 2–4.
- Minimum Strength 3,
  the shortsword's gate:
  a starting item, not an aspirational one.
- Weight ~4 kg,
  so carrying it is a real choice against
  the carry limits.

It is found in the Barracks,
with the guards' gear —
where the methane musket is found,
and where a shield would be stored.

## Enemies

Blocking is player-only in this design.
No enemy blocks,
and no NPC loadout carries a shield —
since a shield's defense counts only while
blocking and NPCs never block,
an NPC shield would be a dead item.

That is a scoping decision, not a direction.
Blocking humanoid enemies,
and ranged humanoid enemies,
are both expected eventually;
each gets its own design discussion
when it comes.
Until then, the implementation guidance is:
build blocking on the shared actor systems
(combat resolution, the defense sync,
stamina) rather than on player-only
assumptions wherever that is avoidable,
so an AI can drive the same stance later
without a refactor.
The genuinely player-specific parts are
the input, the camera, and the HUD.

## Save impact

None.
Equipment serialises by item and slot;
adding a catalog entry does not change the
save format, and no version bump is taken.

## Consequences for the follow-on work

- The design PR also updates
  `docs/design/combat.md` and
  `docs/design/attributes.md` where this
  design extends their claims —
  defense gained a conditional contributor,
  and stamina gained a spender —
  so the design documents do not disagree.
- Art and implementation sub-issues of #39
  are created from this document once it
  merges, the way the ranged-weapons
  sub-issues were created from #127's.

## Out of scope

- Enemy blocking or ranged-enemy behaviour
  (see Enemies).
- Any change to an existing weapon's reach.
- A percentage block-reduction stat
  (the named future lever above).
- New attributes:
  Strength and Endurance carry this design.
