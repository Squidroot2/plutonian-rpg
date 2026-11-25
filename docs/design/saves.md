# Save and Recovery Design

This document describes the bed checkpoint, save file, title loading, and
combat death recovery implemented by `src/save.rs` and `src/loading.rs`.
The save system persists selected gameplay state and rebuilds the ECS session
from it; it does not serialize the live Bevy world.

## Goals and boundaries

- The starting tent bed is the only save point.
- Each bed save creates a separate file with no fixed save count.
- Starting a fresh run does not erase the existing disk save.
- Combat defeat rolls the current run back to its most recent bed checkpoint.
- Falling recovery remains a separate path and preserves current progress.
- A successful bed save and a combat recovery restore the player to full health.

`SaveSlot` owns the save directory, discovered files, active file, and any load error shown on the title screen.
`RunSave` owns the checkpoint for the current run.
Keeping these separate lets a fresh run leave an older disk save available,
while deaths in the new run still restart at its beginning until the player
saves at a bed.

## Snapshot contents

`Snapshot` records the data needed to reconstruct the supported game state:

- Quest journal state, encoded through stable quest and objective identifiers.
- Player inventory, equipment, attribute progression and experience.
- Storage chest contents and dungeon progress, including the Old Relay gate.
- Loose pickup stacks and their world positions, plus IDs of collected RON pickups.
- Nemi's position, rotation, health and quest-related state.
- Each authored encounter's position, rotation, health and loot availability.
- The outpost guards' shared hostility.
- Caravan residents’ health, poses, and flee state after the wagons park, keyed by NPC ID.
- XP contribution credit for living enemies and Nemi, keyed by stable encounter
  identifiers because world loading recreates those actors.

The player is spawned at the configured starting point with the saved
progression, inventory, equipment and experience restored as session state.
Nemi and encounter poses, health, contribution credit and loot are applied
after the Loading-to-Playing transition.
  Living guards regroup at their authored posts so the restored player has room to leave the tent.
  After the caravan parks, their posts are outside the barracks.
  Peaceful caravan residents restore at their homes, and residents who were fleeing keep their saved position and return home when safe.
  Defeated guards retain their saved poses.
Quest journal reconstruction applies durable quest effects, and actor restore
removes the core pickup when the journal says it was already recovered.

Transient UI, input, dialogue, interaction targets, camera orbit and combat
messages are reset during loading instead of persisted.
Visuals, colliders and session entities are rebuilt from assets and feature
systems.

## File format

The file begins with the six-byte `PLUT` header and a little-endian schema
version, followed by a bitcode-encoded snapshot.
The current schema is version 12, and it is the only supported format.
It stores the player name in the snapshot and restores it when the run is loaded.
It encodes item IDs, encounter IDs, and collected pickup IDs as strings.
Parked caravan speaker state is stored alongside the snapshot payload.

There is no backward compatibility.
When the schema changes, bump the version byte and update the single
decoder; saves written by any other version are rejected as unsupported
rather than migrated.
Do not add migrations or legacy decoders without an explicit request from
Hayden.

Reads are limited to 1 MiB plus one byte so oversized files can be rejected.
Unknown headers, malformed bitcode, unsupported versions, and invalid decoded contents make the affected entry unreadable and unloadable.
The game checks inventory and chest stacks, progression, experience, dungeon state, actor poses, pickup positions, and required encounter IDs.
It validates saved item IDs against the current item catalog when loading an entry.
If an equipped item no longer meets its slot or Strength rules, the game returns it to the bag.
A missing item definition or a full stack that prevents returning gear produces an error and preserves the disk checkpoint.

When spawning actors, the game clamps a living enemy’s saved health to its configured maximum and keeps defeated enemies defeated.
New encounter placements start fresh, and removed placements and their pending XP credit are omitted.
Authored pickups are reconstructed from the current placement catalog and the checkpoint’s set of collected IDs.
A dropped item stays in the loose pickup snapshot after its original placement has been collected.
Starter chest stock is loaded for new runs.

The normal save directory is `$XDG_DATA_HOME/plutonian-rpg` when `XDG_DATA_HOME` is absolute.
Otherwise it is `~/.local/share/plutonian-rpg` when `HOME` is absolute.
If neither environment variable provides an absolute directory, file saving is unavailable.
Smoke runs use a unique temporary directory instead.
Each save is named `<player-name>-<unix-epoch-seconds>.save`.
If that name already exists, the game appends `-2`, `-3`, and so on before the extension.
The game discovers `.save` files by listing the directory.
The former `save.bin` is ignored.

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
renames the temporary file to the new save filename.
On a failed write or rename it removes the temporary file where possible and
leaves prior disk files, the active save, `RunSave` checkpoint and player health
unchanged.
Only after the write succeeds does it update both in-memory save resources
and restore the player's health.
The UI displays a short success or failure notice.

## Fresh runs, loading and recovery

On the title screen, **Start Game** clears `RunSave` and enters `Loading` without writing a file.
**Load Game** opens a scrollable list of saves ordered by last-modified time, newest first.
Each entry shows its stored player name and modification time, or identifies an unreadable file.
A confirmation protects deletion, which removes only the selected file.
Selecting a readable entry validates it and makes it the active save and `RunSave` checkpoint.
Each successful bed save creates a new active file and replaces the current run's checkpoint.

Entering `Loading` first despawns entities marked as part of the prior session,
restores inventory, equipment, chests and dungeon resources from the current
checkpoint (or defaults for a fresh run), and clears transient session state.
The loader waits for assets and scene instances, then constructs a quest
journal from the checkpoint's stable IDs or creates a new journal.
World and feature plugins spawn the session entities.
When loading is ready, actors and quest effects are restored and play resumes.

When a player becomes `Dead`, `recover_defeated` requests `Loading`.
The selected save or latest successful bed save in the session remains the `RunSave` checkpoint,
so the loading lifecycle rebuilds the world at that checkpoint.
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
capture it in the bed snapshot, and restore it in the owning feature's lifecycle.
When the schema changes, bump the save version and let older saves be
rejected; do not add migrations or legacy decoders without an explicit
request from Hayden.
Prefer stable IDs over entity IDs for anything whose entities are recreated.
Keep fall recovery distinct unless its gameplay contract is intentionally
changed.

## Golden saves

Use `cargo run --locked -- --save-dir golden-saves/<scenario>` to capture or test a known state.
Relative directories resolve against the process working directory.
The explicit directory replaces the normal save directory for listing, loading, saving, and deletion;
smoke scenarios always use their own temporary directories.
The game logs the resolved directory on startup when an override is used.

Use one subdirectory per scenario, such as `golden-saves/before-caverns/`,
so the load picker contains only saves for that scenario.
To capture a golden save, play to the desired state, save at the bed, and quit.
Every bed save creates a new file,
so keep only the intended file, identified by its player name and capture time, and commit it.

To test the scenario, launch with the same `--save-dir` and select the entry in Load Game.
A later bed save creates another file beside the committed golden save,
and the picker's delete action can remove files in that directory.
After testing, restore tracked golden saves with `git checkout -- golden-saves`
and remove untracked files created by the run with `git clean -f golden-saves`.

Golden saves use the current version 12 format.
A future schema bump will make them unreadable in the picker;
recapture the scenarios after such a bump rather than expecting old files to load.
