use crate::{
    dialogue::DialogueHandlers,
    inventory::{Inventory, ItemKind},
    quest::{
        CONTAIN_THE_HUSHCAPS, LAST_LIGHT, MYCELIUM_GOAL, QuestAction, QuestJournal, QuestStatus,
    },
};

pub(crate) const PROFILE: &str = "nemi";

pub(crate) fn register_topics(handlers: &mut DialogueHandlers) {
    handlers.register(
        PROFILE,
        LAST_LIGHT,
        |journal| available(journal, LAST_LIGHT),
        speak_about_beacon,
    );
    handlers.register(
        PROFILE,
        CONTAIN_THE_HUSHCAPS,
        |journal| available(journal, CONTAIN_THE_HUSHCAPS),
        speak_about_cavern,
    );
}

fn available(journal: &QuestJournal, quest: &str) -> bool {
    matches!(
        journal.status(quest),
        Some(QuestStatus::Active | QuestStatus::Completed)
    )
}

pub(crate) fn speak_about_beacon(
    quest: &mut QuestJournal,
    inventory: &mut Inventory,
) -> &'static str {
    let first_meeting = quest.expects(QuestAction::TalkedToNemi);
    if first_meeting {
        quest.record(QuestAction::TalkedToNemi);
    }
    if quest.expects(QuestAction::ReturnedCore) {
        if inventory.consume_core() {
            quest.record(QuestAction::ReturnedCore);
            if first_meeting {
                return "You found the core in the blue cavern? Our beacon went dark, and families have been waiting to cross the ice. With this, I can light their way again.\n\nThe beacon also powers the Old Relay. Look for the round hatch west and a little south of here. Its archive may still hold the first survey log.\n\nQUEST COMPLETE / THE LAST LIGHT";
            }
            return "There it is. I thought we had lost that light for good. The crossing is safe again.\n\nThe beacon also powers the Old Relay. Look for the round hatch west and a little south of here. Its archive may still hold the first survey log.\n\nQUEST COMPLETE / THE LAST LIGHT";
        }
        if first_meeting {
            return "You found the core in the blue cavern? Our beacon went dark, and families have been waiting to cross the ice. Bring the core to me when you have it with you.";
        }
        return "Bring the beacon power core back to me first.";
    }
    if first_meeting {
        "You made it home before the long dark. Our beacon has gone silent. The families here depend on its signal to cross the ice.\n\nA charged core waits in the blue cavern, beyond the waystones. Follow their lights north, then bring it back."
    } else if quest.is_complete(LAST_LIGHT) {
        "The light reaches across the ice again. Look for the round Old Relay hatch west and a little south of here."
    } else {
        "Follow the blue waystones north from the outpost. The core is inside the cavern at the end of the trail."
    }
}

pub(crate) fn speak_about_cavern(
    quest: &mut QuestJournal,
    inventory: &mut Inventory,
) -> &'static str {
    let reported = quest.expects(QuestAction::ReportedArchiveHushcap);
    if reported {
        quest.record(QuestAction::ReportedArchiveHushcap);
    }
    let ready_to_show = quest.expects(QuestAction::ShowedNemiMycelium);
    if ready_to_show && inventory.count(ItemKind::Mycelium) >= MYCELIUM_GOAL {
        quest.record(QuestAction::ShowedNemiMycelium);
        return if reported {
            "YOU\n\nThere was a mushroom creature in the Relay archive. I found more of them in a cave south of here.\n\nNEMI\n\nHushcaps. Their colony in Hushcap Caverns has started to spread. You've already thinned it, and these samples tell me you reached the growth below. Keep them. I'll watch the old passages.\n\nQUEST COMPLETE / CONTAIN THE HUSHCAPS"
        } else {
            "Five samples. You've thinned the colony enough to slow the spread. Keep the Mycelium. I'll watch the old passages for more.\n\nQUEST COMPLETE / CONTAIN THE HUSHCAPS"
        };
    }
    if reported && ready_to_show {
        "YOU\n\nThere was a mushroom creature in the Relay archive. I found more of them in a cave south of here.\n\nNEMI\n\nHushcaps. Their colony in Hushcap Caverns has started to spread. You've already thinned it. Bring five Mycelium in your backpack so I can examine the growth."
    } else if reported {
        "YOU\n\nThere was a mushroom creature in the Relay archive.\n\nNEMI\n\nA Hushcap. Their colony is spreading beyond Hushcap Caverns. Go south from the courtyard into South Basin. Round the ice fins and follow the bend behind the ridge. Defeat five Hushcaps in the caverns and bring five Mycelium back for me to examine."
    } else if ready_to_show {
        "Bring me five Mycelium in your backpack so I can examine the growth. You can keep the samples afterward."
    } else if quest.is_complete(CONTAIN_THE_HUSHCAPS) {
        "I'll keep watch on the old passages. For now, the spread has slowed."
    } else {
        "The caverns lie south, behind the ridge in South Basin. Defeat five Hushcaps inside before you come back."
    }
}
