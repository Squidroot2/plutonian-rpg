# Save and Recovery Design

This document describes the bed checkpoint, save file, title loading, and
combat death recovery implemented by `src/save.rs` and `src/loading.rs`.
The save system persists selected gameplay state and rebuilds the ECS session
from it; it does not serialize the live Bevy world.

## Goals and boundaries

- The starting tent bed is the only save point.
- There is one overwriteable save slot.
- Starting a fresh run does not erase the existing disk save.
- Combat defeat rolls the current run back to its most recent bed checkpoint.
- Falling recovery remains a separate path and preserves current progress.
- A successful bed save and a combat recovery restore the player to full health.

`SaveSlot` owns the persistent file path, the last valid disk snapshot and any
load error shown on the title screen.
`RunSave` owns the checkpoint for the current run.
Keeping these separate lets a fresh run leave an older disk save available,
while deaths in the new run still restart at its beginning until the player
saves at a bed.

## Snapshot contents

`Snapshot` records the data needed to reconstruct the supported game state:

- Quest journal state, encoded through stable quest and objective identifiers.
- Player inventory, equipment, attribute progression and experience.
- Storage chest contents and dungeon progress, including the Old Relay gate.
- Loose pickup stacks and their world positions.
- Nemi's position, rotation, health and quest-related state.
- Each authored encounter's position, rotation, health and loot availability.
- The outpost guards' shared hostility.
- XP contribution credit for living enemies and Nemi, keyed by stable encounter
  identifiers because world loading recreates those actors.

The player is spawned at the configured starting point with the saved
progression, inventory, equipment and experience restored as session state.
Nemi and encounter poses, health, contribution credit and loot are applied
after the Loading-to-Playing transition.
  Living guards regroup at their authored posts so the restored player has room to leave the tent.
  Defeated guards retain their saved poses.
Quest journal reconstruction applies durable quest effects, and actor restore
removes the core pickup when the journal says it was already recovered.

Transient UI, input, dialogue, interaction targets, camera orbit and combat
messages are reset during loading instead of persisted.
Visuals, colliders and session entities are rebuilt from assets and feature
systems.

## File format and compatibility

The file begins with the six-byte `PLUT` header and a little-endian schema
version, followed by a bitcode-encoded snapshot.
The current schema is version 9.
  It includes credited cavern encounter IDs and journal actions for caravan arrival and Tovan’s examination.
  Loading reconstructs the raised gate from the kill milestone and parks the caravan if it had arrived at the checkpoint.
  During early production, only current-version checkpoints are supported.
The earlier version 1 through 7 decoders predate the crossing expansion.
Legacy saves that predate progression or experience retain the fields they
already had and receive defaults for fields introduced later.
Version 1 equipment slots are decoded using their original three-variant enum
before `Weapon` is mapped to `MainHand`; preserving that old enum shape is
required because bitcode encodes enum cardinality.
Versions 1 through 4 used the archive encounter slot for an ice worm.
  Migration replaces that encounter with a full-health Hushcap at its authored position
  and clears pending XP contribution for the worm.
  The remaining encounters, previously earned XP, and quest progress carry forward.
Version 5 saves track Hushcap health, defeat, and loot availability like other authored encounters.
Version 6 adds stable IDs for the Hushcap Caverns encounters.
  Versions 1 through 5 decode encounter poses and XP contributions with the original six-variant ID enum,
  then add the new cave encounters at their authored positions, with full health and loot.
  Version 5 archive Hushcap progress carries forward.
Version 7 records the completed relay hatch exit and Contain the Hushcaps journal actions.
  The original migration infers the Relay return for a friendly Nemi.
  Existing cavern defeats carry forward through the saved encounter health.
Version 8 adds stable guard IDs and outpost hostility.
  Older saves seed two full-health, neutral guards.
  The file is converted on disk the next time the player saves at the bed.

Reads are limited to 1 MiB plus one byte so oversized files can be rejected.
Unknown headers, malformed bitcode, unsupported versions and invalid decoded
contents are rejected.
Validation checks inventory and chest stacks, equipment slots and strength
requirements, progression, experience, dungeon state, actor poses and health,
pickup positions, and the expected stable encounter set.
An invalid save disables title-screen loading and reports a generic error;
it is not accepted as a checkpoint.

The normal data path is
`$XDG_DATA_HOME/plutonian-rpg/save.bin` when `XDG_DATA_HOME` is absolute.
Otherwise it is `~/.local/share/plutonian-rpg/save.bin` when `HOME` is
absolute.
If neither environment variable provides an absolute directory, file saving
is unavailable.
Smoke runs use a unique temporary directory instead.

## Write lifecycle

Interaction selects the bed through the shared nearby and line-of-sight
interaction rules and emits `SaveAtBed`.
The save system accepts the request only while playing, on the surface, with a
living player, a unique Nemi actor and no pending state transition.
It runs after interaction, combat resolution, the outpost and Nemi's reactions, and experience updates
so the checkpoint includes that frame's consequences.

The system captures a snapshot and validates it before touching the file.
It creates the parent directory, writes the header and bitcode payload to a
uniquely named temporary file beside the destination, calls `sync_all`, then
renames the temporary file over the save slot.
On a failed write or rename it removes the temporary file where possible and
leaves the prior disk file, `SaveSlot`, `RunSave` checkpoint and player health
unchanged.
Only after replacement succeeds does it update both in-memory save resources
and restore the player's health.
The UI displays a short success or failure notice.

## Fresh runs, loading and recovery

On the title screen, **Start Game** clears `RunSave` and enters `Loading`.
It does not clear `SaveSlot`, so **Load Game** can still select the previous
disk snapshot.
Load Game is enabled only when a valid snapshot is available and copies that
snapshot into `RunSave` before entering `Loading`.

Entering `Loading` first despawns entities marked as part of the prior session,
restores inventory, equipment, chests and dungeon resources from the current
checkpoint (or defaults for a fresh run), and clears transient session state.
The loader waits for assets and scene instances, then constructs a quest
journal from the checkpoint's stable IDs or creates a new journal.
World and feature plugins spawn the session entities.
When loading is ready, actors and quest effects are restored and play resumes.

When a player becomes `Dead`, `recover_defeated` requests `Loading`.
The `RunSave` checkpoint is retained, so the loading lifecycle rebuilds the
world at that checkpoint.
The player returns with full health, and defeated actors, loot, inventory,
quest state and progression return to their saved values.
With no checkpoint, loading uses fresh defaults, so death before the first bed
save restarts the run.
Out-of-bounds fall recovery does not use this death rollback path and preserves
live progress.

## Ownership and extension

`src/save.rs` owns the snapshot schema, validation, file IO, bed capture,
checkpoint resources, session reset and combat death trigger.
`src/ui/title.rs` chooses fresh or saved runs, and `src/loading.rs` rebuilds
the quest journal after assets load.
World, quest, dungeon, inventory, equipment and actor plugins own their parts
of spawning and restoration.

When adding persistent state, define its stable representation and validation,
capture it in the bed snapshot, restore it in the owning feature's lifecycle,
and account for older schema versions.
Prefer stable IDs over entity IDs for anything whose entities are recreated.
Keep fall recovery distinct unless its gameplay contract is intentionally
changed.
