use crate::{
    dialogue::DialogueHandlers,
    inventory::{Inventory, ItemKind},
    quest::{
        CONTAIN_THE_HUSHCAPS, LAST_LIGHT, MYCELIUM_GOAL, OLD_RELAY, QuestAction, QuestJournal,
        QuestStatus,
    },
};

pub(crate) const PROFILE: &str = "nemi";

pub(crate) const RETURN_BEACON_CORE: &str = "return_beacon_core";
pub(crate) const REPORT_ARCHIVE_HUSHCAP: &str = "report_archive_hushcap";
const SHOW_MYCELIUM: &str = "show_mycelium";

pub(crate) fn register_topics(handlers: &mut DialogueHandlers) {
    handlers.register(
        PROFILE,
        LAST_LIGHT,
        |journal, inventory| active(journal, LAST_LIGHT) && !can_return_core(journal, inventory),
        speak_about_beacon,
    );
    handlers.register(
        PROFILE,
        RETURN_BEACON_CORE,
        can_return_core,
        speak_about_beacon,
    );
    handlers.register(
        PROFILE,
        OLD_RELAY,
        |journal, _| active(journal, OLD_RELAY),
        |journal, _| speak_about_relay(journal),
    );
    handlers.register(
        PROFILE,
        REPORT_ARCHIVE_HUSHCAP,
        |journal, _| journal.expects(QuestAction::ReportedArchiveHushcap),
        speak_about_cavern,
    );
    handlers.register(
        PROFILE,
        CONTAIN_THE_HUSHCAPS,
        |journal, inventory| {
            active(journal, CONTAIN_THE_HUSHCAPS)
                && !journal.expects(QuestAction::ReportedArchiveHushcap)
                && !can_show_samples(journal, inventory)
        },
        speak_about_cavern,
    );
    handlers.register(PROFILE, SHOW_MYCELIUM, can_show_samples, speak_about_cavern);
}

fn active(journal: &QuestJournal, quest: &str) -> bool {
    journal.status(quest) == Some(QuestStatus::Active)
}

fn can_return_core(journal: &QuestJournal, inventory: &Inventory) -> bool {
    inventory.count(ItemKind::BeaconCore) > 0
        && (journal.expects(QuestAction::ReturnedCore)
            || (journal.expects(QuestAction::TalkedToNemi)
                && journal.has_occurred(QuestAction::RecoveredCore)))
}

fn can_show_samples(journal: &QuestJournal, inventory: &Inventory) -> bool {
    journal.expects(QuestAction::ShowedNemiMycelium)
        && inventory.count(ItemKind::Mycelium) >= MYCELIUM_GOAL
}

fn speak_about_relay(journal: &QuestJournal) -> &'static str {
    if !journal.beacon_powered() {
        "The round hatch northwest of the beacon tower leads to the Old Relay. The first survey crews sheltered down there. Once the beacon has power again, the hatch will open."
    } else if journal.expects(QuestAction::EnteredRelay) {
        "Look northwest of the beacon tower for a round hatch with a blue ring. The beacon has lit it again. Down there, the old lamps still remember the way the first survey crews took."
    } else if journal.expects(QuestAction::OpenedArchiveGate) {
        "Go north from the entry vestibule to the junction and follow the amber lamps east. The maintenance switch on the plinth opens the archive gate."
    } else {
        "Follow the blue lamps north from the junction and go through the raised gate. The first survey log is on a pedestal in front of the wall map."
    }
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
    if can_show_samples(quest, inventory) {
        quest.record(QuestAction::ShowedNemiMycelium);
        return if reported {
            "Hushcaps. Their colony in Hushcap Caverns has started to spread. You've already thinned it, and these samples tell me you reached the growth below. Keep them. I'll watch the old passages.\n\nQUEST COMPLETE / CONTAIN THE HUSHCAPS"
        } else {
            "Five samples. You've thinned the colony enough to slow the spread. Keep the Mycelium. I'll watch the old passages for more.\n\nQUEST COMPLETE / CONTAIN THE HUSHCAPS"
        };
    }
    if reported && ready_to_show {
        "Hushcaps. Their colony in Hushcap Caverns has started to spread. You've already thinned it. Bring five Mycelium in your backpack so I can examine the growth."
    } else if reported {
        "A Hushcap. Their colony is spreading beyond Hushcap Caverns. Go south from the courtyard into South Basin. Round the ice fins and follow the bend behind the ridge. Defeat five Hushcaps in the caverns and bring five Mycelium back for me to examine."
    } else if ready_to_show {
        "Bring me five Mycelium in your backpack so I can examine the growth. You can keep the samples afterward."
    } else {
        "The caverns lie south, behind the ridge in South Basin. Defeat five Hushcaps inside before you come back."
    }
}
