# NPC dialogue

## Topics and quest choices

A conversation starts with the speaker's topic list.
  Each answer returns to that list, so the player can ask another question, reread a reply, or leave.
  Repeatable topics suit short conversations about a person, a place, or a quest without requiring the player to replay an introductory exchange.

Nemi offers two repeatable lore topics and conversational choices for the beacon, the Old Relay, and the Hushcaps.
  Quest choices reflect the current objective and the contents of the backpack.
  The beacon question gives way to an offer to hand over the core when it is carried.
  The Hushcap report gives way to directions or an offer to show the samples.
  Each quest's choices disappear when that quest is complete.
  Opening the conversation and reading lore leave the journal and backpack as they were.
  Telling Nemi you found the core on a first visit records the meeting and hands over the core in the same exchange.

The Old Relay topic appears when the player discovers the sealed hatch or restores the beacon.
  Nemi gives advice for the current objective.
  The player progresses the quest at the corresponding world interactions.

## Authoring

The game loads `data/dialogue/catalog.dialogue.ron` from the same data source as the quest catalogue.
  The asset controls the speaker's display name, optional refusal, topic order, labels, and lore replies.
  For example, add this entry to a speaker's `topics` list to offer another question.

```ron
(
    id: "crossing_lamps",
    label: "Ask about the crossing lamps",
    reply: Text("Look for two blue lights. One might be the ice looking back."),
),
```

For a new speaker, add a profile with a unique `id`, a `name`, and a `topics` list.
  Attach `Speaker::new(profile_id, area, interaction_height)` to the actor at spawn.
  Use the actor's behavior systems to set `Speaker::availability` when their willingness to talk changes.
  A refusing speaker can be approached when their profile supplies refusal text, for example `refusal: Some("Come back later.")`.

For a gameplay topic, use `reply: Quest` and register its profile and topic IDs with `DialogueHandlers` in the feature's plugin.
  Both functions receive the live journal and inventory.
  The read-only availability function decides whether a choice appears.
  Use separate topic IDs and labels for different player intents.
  Choices that share an outcome can reuse the same reply function.
  The quest functions stay in Rust so progression and item changes use the same validated operations as other gameplay.
  Loading checks the catalogue for blank fields and duplicate IDs and checks that quest topics and handlers agree in both directions.

## Session and input

Interaction and dialogue share the same range, area, and line-of-sight rules.
  A conversation keeps the speaker entity and its current page in a transient resource.
  Before accepting a choice, the dialogue system verifies that the speaker is still present, alive, and able to talk.
  It checks the player's position and area again while the conversation is open.
  A confirmed player hit on Nemi closes that conversation, and Nemi's existing flee and refusal behavior determines whether he can be approached again.

The dialogue input system runs at the start of the interaction chain.
  Opening a conversation later in that chain keeps the opening press from also selecting a topic.
  Pointer clicks carry a page revision, and keyboard focus tracks a topic ID.
  The system rechecks availability before accepting a choice, so a disappearing row cannot redirect Enter to another action.
  Menu Up and Menu Down move among topics, Enter selects the highlighted row, and a primary click selects a row directly.
  Interact or Enter continues past a reply, refusal, or world notice.
  Goodbye and Escape end the conversation.
  Dismissal takes effect in `Last` so the closing input stays captured for the rest of the frame.

Topic handlers run before the journal's world effects, quest XP awards, and bed saving.
  The panel is hidden while paused and can resume with the same conversation after rechecking eligibility.
  Loading clears the dialogue and pending click.
  Quest effects use the existing journal and inventory save format, so no dialogue fields are required in a bed save.

## Verification

Renderer-free tests exercise definition validation, repeatable topics, entry and dismissal input, and stale choices.
  The arena smoke follows The Last Light through the topic list.
  The shipped-world smoke reads Nemi's two lore replies before handing over the core, asks about the Old Relay, and reports the Hushcap after leaving the Relay.
  The Nemi smoke verifies refusal and the solo beacon route after an attack.
