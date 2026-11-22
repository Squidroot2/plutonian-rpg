# NPC dialogue

## Topics and quest choices

A conversation starts with the speaker's topic list.
  Each answer returns to that list, so the player can ask another question, reread a reply, or leave.
  Repeatable topics suit short conversations about a person, a place, or a quest without requiring the player to replay an introductory exchange.

Nemi offers two lore topics and separate topics for The Last Light and Contain the Hushcaps.
  Quest topics appear when their quests are active and remain available after completion as reminders.
  Opening the conversation and reading lore leave the journal and backpack as they were.
  A quest choice can complete more than one step when its prerequisites are already satisfied.
  For example, choosing The Last Light for the first time with the core in the backpack records the meeting and hands over the core in the same exchange.

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
  Supply a read-only availability function and a reply function that uses the current journal and inventory.
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
  Pointer clicks carry a page revision, and quest selections are checked against the currently available topics.
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
  The shipped-world smoke reads Nemi's two lore replies before the first core handoff and selects the Hushcap briefing after leaving the Relay.
  The Nemi smoke verifies refusal and the solo beacon route after an attack.
