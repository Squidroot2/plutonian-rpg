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

## Save inspection and creation

Run `cargo run --locked --bin plutonian-save -- inspect [PATH]` to inspect a save as text,
or add `--json` to print JSON for diffing and tooling.
A path can name one save file or a directory;
a directory selects its newest `.save` by modification time.
Without a path, the tool uses the normal save directory.
The newest file is inspected even if it is unreadable.
The inspector uses the game's current-version decoder and validates the saved contents,
without applying catalog-dependent changes or modifying any file.

The JSON object is the shared representation for `inspect --json` and `create`.
Its top-level `format_version` is the binary save version.
The remaining top-level fields are `player_name`, `journal`, `inventory`, `equipment`, `chests`,
`dungeon`, `pickups`, `collected_pickups`, `cavern_defeats`, `nemi`, `nemi_state`,
`encounters`, `progression`, `experience`, `guards_hostile`, and `caravan`.
Item, encounter, collected pickup, credited cavern encounter, and caravan IDs use their stable string keys.
Quest progress is keyed by quest ID and records its outcome and completed objective IDs.
Inventories and each chest map item IDs to quantities,
while equipment maps slot names to item IDs.
Pickups contain a stack and a three-coordinate position.
Each encounter and caravan actor records its position, quaternion rotation, and health,
alongside the encounter's loot flag or the caravan actor's flee state.
The journal records quest outcomes, completed objectives, and occurred actions;
progression records attribute ranks, level, and unspent points.
The `experience` object includes the player's XP and contribution credits.
Enums use serde's externally tagged representation,
so a unit state is a string and a state with data is an object keyed by its variant.
The separately stored `cavern_defeats` and `caravan` fields are included alongside the wire snapshot,
so the JSON captures the complete saved state.

Run `cargo run --locked --bin plutonian-save -- create fixture.json --output out.save`
to validate a fixture against the current save format and shipped gameplay catalogs,
then write a save using the game's atomic writer.
The command requires an explicit output path
and rejects invalid fixtures before writing.
The validated snapshot is written as provided,
so any load-time equipment adjustments happen when the game loads the save.
Use the values and array ordering emitted by `inspect --json`,
including the saved precision of positions and rotations.

The `schema` subcommand prints the JSON Schema for this inspection document.
The committed schema is `golden-saves/save.schema.json`.
Regenerate it after changing save representation types with
`cargo run --locked --bin plutonian-save -- schema > golden-saves/save.schema.json`.
Point an editor's JSON Schema setting for files under `golden-saves/` at this file
to get completion and structural errors while editing fixtures.
CI checks that the committed schema matches fresh output
and validates each golden save fixture against it.

## Golden saves

The [golden saves developer guide](../contributing/golden_saves.md) covers capture, regeneration, and fixture checks.

Commit one JSON fixture per scenario at `golden-saves/<scenario>.json`.
The `after-old-relay` fixture comes from a bed save after the survey log was read and the Old Relay quest was completed.
To capture a new fixture, play to the desired state, save at the bed, and quit.
Run `cargo run --locked --bin plutonian-save -- inspect --json PATH > golden-saves/<scenario>.json`,
where `PATH` names the captured save.

Run `python3 scripts/golden_saves.py` to generate the saves.
For each scenario, the script replaces existing `.save` files in that scenario's generated directory
with a save named for the stored player and the current Unix time.
Generated `.save` files are ignored by Git.
After testing, rerun the script to restore the scenario from its committed JSON fixture.

Launch with `cargo run --locked -- --save-dir golden-saves/<scenario>`,
then select the generated save in **Load Game**.
Relative directories resolve against the process working directory.
The override applies to listing, loading, saving, and deletion.
Smoke scenarios use separate temporary directories.
The game logs the resolved directory at startup.

CI generates each fixture in a temporary directory
and checks that inspecting the resulting save reproduces the fixture's parsed JSON.
Update fixtures when a schema or catalog change invalidates them.
