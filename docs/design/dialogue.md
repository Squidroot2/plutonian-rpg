# NPC dialogue

## Topics and quest choices

A conversation starts with the speaker's topic list.
  Each answer returns to that list, so the player can ask another question, reread a reply, or leave.
  Repeatable topics suit short conversations about a person, a place, or a quest without requiring the player to replay an introductory exchange.

Nemi offers two repeatable lore topics and conversational choices for the beacon, the Old Relay, and the Hushcaps.
  Quest choices reflect the current objective and the contents of the backpack.
  The offer to help gives way to an offer to hand over the core when it is carried.
  The Hushcap report gives way to directions to the caverns or the caravan.
  Tovan examines five samples once he reaches his workshop.
  Isera, Vey, and Orla have repeatable topics about the crossing, velliks, and home.
  Each quest's choices disappear when that quest is complete.
  Opening the conversation and reading lore leave the journal and backpack as they were.
  Telling Nemi you found the core on a first visit records the meeting and hands over the core in the same exchange.

The Old Relay topic appears when the player discovers the sealed hatch or restores the beacon.
  Nemi gives advice for the current objective.
  The player progresses the quest at the corresponding world interactions.

## Authoring

The game loads `data/dialogue/catalog.dialogue.ron` from the same data source as the quest catalogue.
  Its `profiles` list holds speaker names, refusals, ordered topics, and replies.
  Its `notices` list holds text displayed by world interactions, including readable objects such as the survey log.
  Each notice has a namespaced ID, for example `old_relay.survey_log`.
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

For a gameplay topic, use `reply: Quest` and register its profile and topic IDs with `DialogueRegistry` in the feature's plugin.
  Availability functions take `&DialogueContextRef<'_>` and decide which choices appear.
  Reply functions take `&mut DialogueContextMut<'_>`, apply gameplay effects, and return a `ReplyId` from the set declared at registration.
  Both forms of `DialogueContext` borrow the live journal and player inventory.
  To make more game state available to handlers, extend the context and supply the new field when opening, processing, and rendering dialogue.
  If the field affects topic visibility, include it in the UI's change detection.
  Choices that share an outcome can reuse the same reply ID and wording from the profile's `replies` list.

Edit the `text` field to revise wording for an existing outcome or notice.
  The RON `id` must exactly match the string value used in Rust for speaker profiles, registered quest topics, replies, and notices.
  Update both locations when renaming an ID.
  To add a quest outcome, add an `(id: "outcome_id", text: "Reply text")` entry to the speaker's `replies` list and register that ID with the handler that can return it.
  To add a notice, put its ID and text in `notices` and register the ID with `require_notices` in the feature's plugin.
  Loading rejects duplicate IDs, blank text, and missing registered replies or notices.
  The quest catalogue continues to define journal text, objectives, and transitions.

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
