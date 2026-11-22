# System interactions and schedules

This document describes the current runtime architecture.
  Plugin registration is in `src/main.rs`; individual plugins own their systems and ordering.
  Bevy may run systems concurrently or in either order unless a schedule, set, chain, or explicit dependency orders them.
  The diagrams below show dependencies, not a total ordering of every system.
  Observer callbacks run when their events are triggered; they are not independent stages of the frame.

## Ownership and lifecycle

Components hold actor-specific state: health, movement, attacks, reactions, animation and equipment attachment points.
  Resources hold shared session state such as the quest journal, inventory, equipment, dialogue and save slot.
  Messages connect combat and saving without making attackers responsible for their targets' behavior.
  Observers handle discrete requests and scene/world construction.
  Species define their tuning and visuals while the shared enemy systems operate on capabilities.

`Startup` creates persistent camera, UI and audio resources.
  The title is constructed on `OnEnter(Title)`; title input runs in `PreUpdate` after Bevy input.
  Loading starts on `OnEnter(Loading)`: `save::reset_session` precedes `loading::load`, so the new session begins from the selected checkpoint rather than stale world state.
  The loading check runs in `Update`, waits for assets and instantiated scenes, and triggers world construction via `SpawnWorld` observers.
  World, dungeon, encounters and other features own their respective spawn observers.
  Scene readiness also binds animation and equipment sockets.
  On exit from loading, grounding systems place spawned objects against the world.
  On the Loading-to-Playing transition, saved actors and the gate are restored and quest effects are applied.
  These transition systems are not a single ordered chain unless their registrations say so.

## Frame and fixed-step work

| Schedule | Main responsibilities | Reason |
| --- | --- | --- |
| `PreUpdate` | Bevy input, pause, player movement input and camera look; title and settings navigation | Read current input before simulation. Pause runs after input; player input and camera look run after pause. Settings navigation follows title input and pause. |
| `FixedUpdate` | Player movement, enemy planning and movement, Nemi movement, gate movement, health protection/recovery and corpse expiry | Fixed-rate movement and timers are independent of render rate. Gate movement precedes player movement; enemy planning then movement follows player movement. Nemi also moves after the player. Dialogue gates enemy and combat timer systems. |
| `Update` | Interaction, attacks, damage, reactions, progression, saving, UI and presentation | Frame-driven input and gameplay consequences are processed here. Explicit dependencies below determine which results are visible to consumers. |
| `PostUpdate` | Camera follow and combat feedback facing | Camera follow precedes transform propagation; feedback faces the camera after follow and before propagation, so rendered transforms use the updated view. |
| `Last` | Player menu close, then inventory prompt dismissal | Closing and dismissing happen after the frame's other UI work; prompt dismissal is explicitly after menu close. |

`Playing` gates gameplay systems.
  Entering pause, looting or storage freezes the player; exiting those states resumes it.
  Leaving Playing cancels attacks and clears combat effects.
  The player menu is separate from those modal states, allowing the world to continue while the journal is open.
  State changes requested through `NextState` take effect during Bevy's state transition processing, not immediately at the point of the request.

## Combat and progression

```text
interaction target -> interaction -> player facing -> attack detection
attribute spending -> capability sync -------------------> detection
                                                     Detect -> Resolve
                                defense sync -----------------> Resolve
Resolve -> reactions / animation / audio / feedback
Resolve + interaction + Nemi reaction -> experience -> save -> HUD
```

`InteractionSystems` chains target selection and interaction, so an action uses the current target.
  Player facing and weapon attack follow interaction, and facing precedes `CombatSystems::Detect`.
  Attribute spending and capability synchronization are chained before detection, so attacks use current attributes.
  Weapon attacks and enemy spit contacts emit `Hit`.
  The Detect and Resolve sets are chained.
  Spit advances existing projectiles before spawning newly released shots,
  and defeat cleanup runs after Resolve.
  Defense synchronization precedes Resolve.

Resolve chains hit resolution, recovery delay, death marking, dead collider removal, hit reactions and corpse fading.
  Damage validation produces `ConfirmedHit`, and death handling produces `ActorDefeated`; downstream systems use those results instead of predicting whether an attack succeeded.
  Nemi's reaction and animation are chained after Resolve; enemies, dummy, player animation, combat audio and visual feedback also react after Resolve.
  This allows the same frame's confirmed damage and deaths to drive feedback.
  Fixed-step protection, recovery and corpse expiry are separate from this frame sequence.

Experience ages its notice and awards rewards after Resolve, interaction and Nemi's reaction.
  Save processing chains notice aging, defeated-player recovery and bed saving after those same sources and experience.
  Thus a checkpoint sees the relevant progression and combat consequences before capture.
  HUD and prompts follow interaction and bed saving; experience and quest displays follow experience updates.
  Character UI follows both attribute and experience updates.
  These are partial dependencies: a line in the diagram does not imply an ordering between unrelated branches.

## World interactions, quests and equipment

Interaction selects eligible nearby capabilities, then dispatches feature-specific work for pickups, quests, Nemi, chests, corpses, beds and dungeon targets.
  Storage and loot open on entering their modal states; storage clears its active target on exit.
  Inventory and equipment requests are handled by observers, which validate and mutate their owning resources.
  Inventory UI synchronizes rows, navigation, quantity editing and selection in a chain after attribute updates; equipment's `sync_worn` follows combat detection so draw/holster changes are reflected in attachments.

The asset-authored quest catalog and `QuestJournal` record actions and progression.
  Interaction and Nemi's confirmed-hit reaction can update the journal; quest effects run after interaction when the journal changes, and once on the Loading-to-Playing transition to reconstruct world effects.
  Cavern defeat credit reads the authored Hushcaps after combat resolution and records the milestone before experience and checkpoint updates.
  A change from another producer without an explicit ordering to effects may be observed on a subsequent frame; do not assume all journal writers precede effects in the same Update.
  The quest UI synchronizes after experience updates when its tab is open.

Bed saves capture session state only after preceding interaction, combat and progression work.
  Death recovery returns through loading, rebuilding the world and restoring the checkpoint; save, world, dungeon and quest plugins each own part of that reconstruction.
  This division keeps transient entities and visual state reconstructible from durable progress rather than serializing the entire ECS world.

## Extending the schedule

Put a new system in the schedule matching its clock and state, and express only the dependencies it actually needs.
  In particular, emit `Hit` in Detect, consume confirmed outcomes after Resolve, and order persistence or UI after the producers whose same-frame results they require.
  A shared resource conflict alone prevents simultaneous access but does not promise execution order.
  Preserve existing chains and transition timing during refactors; check each producer and consumer when adding a new cross-feature dependency.
  Desktop smoke drivers add their own scenario systems and are test instrumentation, not the normal gameplay pipeline.
