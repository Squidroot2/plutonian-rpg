# Quest System

## Purpose

Quest definitions live in `data/quests/catalog.quest.ron`.
  The runtime loads
them as a Bevy asset, validates their structure, and uses a `QuestJournal`
resource to track each quest independently.
  The journal drives the Quest Log
and can also be queried by gameplay systems to gate interactions and world
progress.
  Quest objectives are presented in the journal; the system does not
spawn objective markers or put objective text on the HUD.

## Philosophy

The game relies on exploration rather than following quest markers. 
Quest text should be detailed enough that the user can find the next objective without assistance from quest markers.

## Runtime model

The asset catalog contains a list of `QuestDefinition`s.
  Each definition has a
stable quest ID, player-facing title, summary and completion text, completion
XP, activation conditions, a first objective, optional interrupts, and ordered
objective definitions.
  An objective has a stable ID, title, directions, and transitions.
Each transition matches a condition and selects a destination: another
objective, completion, or failure.

`QuestJournal` owns the loaded catalog, current world layout, progress keyed by
quest ID, and the set of actions that have occurred during the run.
  A quest is
absent from the journal until its activation rule matches.
  Once present, it is
active at one objective, completed, or failed.
  Several quests may be active at
once, and one recorded action may advance multiple quests.

The runtime processes a newly recorded action against the active objective of
each started quest.
  A matching interrupt fails that quest before its objective
transition is considered.
  It then repeatedly settles persistent conditions:
activation rules may start quests, and active objectives may advance when a
persistent condition is already true.
  This lets a quest activate after its
prerequisite happened earlier.
  Settling uses a snapshot of conditions on each
pass, so quest order in the catalog does not decide which quest can advance
from a newly satisfied persistent condition.
  If more than one transition on
the same objective can match, the first matching transition in that objective's
`transitions` list wins.
  Do not make persistent transitions cycle; the asset
validator rejects cycles.

## Conditions and transitions

The current condition forms are:

| Condition | Meaning |
| --- | --- |
| `SessionStart` | Matches when a fresh journal is created. Valid only in `starts_when`. |
| `Action(SomeAction)` | Matches the newly recorded action. It does not replay for actions that happened before this objective became current. |
| `Occurred(SomeAction)` | Matches whenever that action has occurred at least once in this run, including after a later quest activation. |
| `QuestCompleted("quest_id")` | Matches once the referenced quest is complete. |
| `QuestFailed("quest_id")` | Matches once the referenced quest has failed. |

`Action` is an event edge; `Occurred` and quest status conditions are persistent
state.
  The list of action variants is defined by `QuestAction` in
`src/quest.rs`.
  Gameplay code records these actions at the point where the
corresponding interaction or event succeeds.

For a transition to another objective or to completion, `credit` defaults to
`true`: the objective being left is added to the completed-steps list.
  Set
`credit: false` when the transition should advance without claiming that
objective as completed.
  A transition to failure does not credit the current
objective. `credit_early`
supports nonlinear play: when an `Occurred` condition is already true, the
referenced objective can appear in completed steps without moving the current
objective.
  It is valid only with an `Occurred` condition, `credit: true`, and a
non-failure destination.
  This is useful when a player performs a later task
before receiving the quest, while the quest should still begin at its intended
first objective.

## Completion rewards

Set `completion_xp` to a positive whole number in every quest definition.
The Last Light and Relight the Beacon each award 70 XP.
  Both routes restore
the beacon and earn level 2 from quest XP alone.
  Old Relay awards 120 XP for
finding the survey log, which brings either route to level 3.
  The archive Hushcap makes reaching the log dangerous,
  but defeating it is optional and grants a separate combat reward.

## Contain the Hushcaps

Restoring the beacon and leaving through the Relay hatch after reading the survey log makes the Hushcap quest available.
  Nemi gives the briefing, and a warning in the log supplies it on the solo route.
  The journal credits combat defeat messages for distinct cavern Hushcaps after the quest activates.
  Defeats credited during the report step stay recorded while the report remains current.
  The fifth credited defeat opens the southwest gate for the caravan.
  Tovan can examine the samples when the caravan parks and the player carries five Mycelium.
  Tovan leaves the samples with the player and completion awards 150 XP.

## Reward lifecycle

`QuestJournal` queues the reward when an active quest transitions to
`Complete`, whether the transition follows a fresh action or settles a
persistent condition.
  Activation, individual objectives, early credit, and
failure grant zero XP.
  A completed outcome is terminal, which prevents
repeated actions from awarding XP again.

The experience system drains the queued rewards after quest actions and
combat resolve.
  It combines them with any kill XP from the same update,
applies level gains, and displays the shared XP notice.
  Bed saves capture
the resulting XP and quest outcomes together.
  See [Experience and Leveling](experience.md).

## Loading, layout, and saves

During loading, the game requests `data://quests/catalog.quest.ron`.
  The
`QuestCatalogLoader` parses RON and validates the catalog before play begins.
Validation rejects missing or duplicate quest IDs and objective IDs, blank
IDs, empty activation/objective/transition lists, unknown objective targets,
invalid quest references, missing or zero `completion_xp`, invalid
`credit_early` combinations, and cycles made
of persistent transitions.
  The shipped catalog also requires the IDs
`last_light`, `old_relay`, `relight_beacon`, and `contain_the_hushcaps`; keep these definitions unless
the runtime's required-ID check is deliberately changed.

`outpost_only: true` prevents a quest from activating or settling outside the
Outpost layout.
  Omit it for quests that should also run in the arena.
  A fresh
game constructs a journal from the catalog and settles `SessionStart`.
  Loading
a bed checkpoint reconstructs progress from stable quest and objective IDs.
Snapshots store the active objective by ID, completed objective IDs, outcome,
and occurred actions.
  Renaming or removing IDs used by saved progress makes
that checkpoint invalid; the loader rejects it and returns the player to the
title flow rather than silently remapping progress.
  Treat IDs as save data and
preserve them when editing quest prose or order.

Restored journals start with an empty reward queue.
  Completed quests in an
existing save keep their outcome and the player keeps their saved XP.
  If
combat death rolls a quest back to an active objective, reaching completion
again awards XP on that restored run.

## Journal UI

The Quest Log is the Quest tab in the player menu and is opened by the
rebindable Quest Log shortcut (default `L`).
  Entries are grouped into Active,
Completed, and Failed sections.
  Selecting an entry shows its summary, current
objective and directions, or its completion/failure text.
  Active and completed
entries show the configured completion XP.
  Completed objective
titles are listed as completed steps.
  The menu remains part of the live game:
opening it does not pause the world.

## Creating a quest

For a quest that can use existing gameplay actions, add a definition to
`data/quests/catalog.quest.ron`:

1. Choose a stable, unique snake_case quest ID and stable objective IDs.
  IDs
   are persisted in bed saves; changing display text does not require changing
   IDs.
2. Write `title`, `summary`, and `completion` for the journal.
  Set a positive
   `completion_xp`, add `starts_when` conditions, and choose `first_objective`.
3. Add objectives with a player-facing `title`, actionable `directions`, and
   one or more transitions.
  Use `Action(...)` for a new event at the current
   step, `Occurred(...)` for an event that can be credited after the fact, and
   `QuestCompleted(...)` or `QuestFailed(...)` for quest dependencies.
4. Decide whether leaving an objective should credit it.
  The default is true;
   set `credit: false` when it should not be shown as completed.
  Use
   `credit_early: true` only for an `Occurred(...)` transition whose objective
   should be remembered without skipping the current objective.
5. Add `outpost_only: true` if the quest belongs only to the Outpost.
  Add an
   `interrupts` entry when an action should fail the quest with a reason.
6. Check that every objective has transitions, every objective destination
   exists, quest references point to another catalog quest, and persistent
   transitions cannot loop.
  Keep the three required shipped quest IDs in the
   catalog.
7. Run the relevant smoke scenario and inspect the Quest Log through
   activation, progression, completion/failure, and any intended early-action
   path.
  Verify the directions against the world and check that completion
   awards the configured XP once.

Example of a minimal shape (replace the sample IDs and prose):

```ron
(
    id: "sample_signal",
    title: "A Signal in the Snow",
    summary: "Find the source of a distant signal.",
    completion_xp: 40,
    starts_when: [SessionStart],
    first_objective: "inspect_signal",
    objectives: [
        (
            id: "inspect_signal",
            title: "Inspect the signal",
            directions: "Follow the lights to the ridge and inspect the transmitter.",
            transitions: [(when: Action(ReadSurveyLog), to: Complete)],
        ),
    ],
    completion: "You found the source of the signal.",
)
```

The example uses `ReadSurveyLog` only to show the data shape; choose an action
that actually represents the objective.
  If no existing `QuestAction` matches
the desired gameplay event, add a variant in `src/quest.rs` and record it from
the system that owns that successful event.
  If the event needs to affect world
state or interaction availability, wire those checks into the owning gameplay
feature as well.
  Update or add quest progression tests and the relevant smoke
scenario for that new behavior.

## Current action producers

Actions are intentionally emitted by gameplay code, not by quest assets.
  The
current catalog uses actions for Nemi dialogue and combat, beacon-core recovery
and delivery, hatch inspection and relay entry, archive-gate opening, and
survey-log reading.
  When creating a quest, follow the existing producer for
the action you depend on so the action is recorded only after the interaction
actually succeeds.

## Implementation map

- `src/quest.rs`: asset schema and validation, action types, journal
  progression, persistence, and beacon presentation effects.
- `data/quests/catalog.quest.ron`: shipped quest definitions.
- `src/loading.rs`: catalog asset request and fresh/save-backed journal setup.
- `src/quest_ui.rs`, `src/menu_ui.rs`, `src/controls.rs`: Quest Log display,
  menu tab, and shortcut.
- `src/interaction.rs`, `src/dungeon.rs`, and `src/nemi/dialogue.rs` produce quest actions and handle quest-dependent interactions.
  See [NPC dialogue](dialogue.md) for topic authoring and quest handler registration.
- `src/save.rs`: bed-save snapshot and world restoration that consults journal
  history.
