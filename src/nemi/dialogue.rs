//! Keep `PROFILE`, the quest topic IDs registered in `register_topics`,
//! and the strings inside the `ReplyId` constants in sync with Nemi's profile,
//! topic, and reply IDs in `data/dialogue/catalog.dialogue.ron`.

use crate::{
    dialogue::{DialogueRegistry, ReplyId},
    inventory::{Inventory, ItemKind},
    quest::{
        CONTAIN_THE_HUSHCAPS, LAST_LIGHT, MYCELIUM_GOAL, OLD_RELAY, QuestAction, QuestJournal,
        QuestStatus,
    },
};

pub(crate) const PROFILE: &str = "nemi";

pub(crate) const RETURN_BEACON_CORE: &str = "return_beacon_core";
pub(crate) const REPORT_ARCHIVE_HUSHCAP: &str = "report_archive_hushcap";
pub(crate) const SHOW_MYCELIUM: &str = "show_mycelium";

pub(crate) const RELAY_SEALED: ReplyId = ReplyId("relay_sealed");
pub(crate) const RELAY_HATCH: ReplyId = ReplyId("relay_hatch");
pub(crate) const RELAY_SWITCH: ReplyId = ReplyId("relay_switch");
pub(crate) const RELAY_SURVEY_LOG: ReplyId = ReplyId("relay_survey_log");
pub(crate) const EARLY_CORE_RETURNED: ReplyId = ReplyId("early_core_returned");
pub(crate) const CORE_RETURNED: ReplyId = ReplyId("core_returned");
pub(crate) const EARLY_CORE_STORED: ReplyId = ReplyId("early_core_stored");
pub(crate) const CORE_MISSING: ReplyId = ReplyId("core_missing");
pub(crate) const BEACON_BRIEFING: ReplyId = ReplyId("beacon_briefing");
pub(crate) const CORE_DIRECTIONS: ReplyId = ReplyId("core_directions");
pub(crate) const REPORT_AND_SAMPLES_SHOWN: ReplyId = ReplyId("report_and_samples_shown");
pub(crate) const SAMPLES_SHOWN: ReplyId = ReplyId("samples_shown");
pub(crate) const REPORT_NEEDS_SAMPLES: ReplyId = ReplyId("report_needs_samples");
pub(crate) const CAVERN_BRIEFING: ReplyId = ReplyId("cavern_briefing");
pub(crate) const SAMPLES_NEEDED: ReplyId = ReplyId("samples_needed");
pub(crate) const CAVERN_DIRECTIONS: ReplyId = ReplyId("cavern_directions");
const RELAY_REPLIES: &[ReplyId] = &[RELAY_SEALED, RELAY_HATCH, RELAY_SWITCH, RELAY_SURVEY_LOG];
const BEACON_REPLIES: &[ReplyId] = &[
    EARLY_CORE_RETURNED,
    CORE_RETURNED,
    EARLY_CORE_STORED,
    CORE_MISSING,
    BEACON_BRIEFING,
    CORE_DIRECTIONS,
];
const CAVERN_REPLIES: &[ReplyId] = &[
    REPORT_AND_SAMPLES_SHOWN,
    SAMPLES_SHOWN,
    REPORT_NEEDS_SAMPLES,
    CAVERN_BRIEFING,
    SAMPLES_NEEDED,
    CAVERN_DIRECTIONS,
];

pub(crate) fn register_topics(handlers: &mut DialogueRegistry) {
    handlers.register(
        PROFILE,
        LAST_LIGHT,
        |journal, inventory| active(journal, LAST_LIGHT) && !can_return_core(journal, inventory),
        BEACON_REPLIES,
        speak_about_beacon,
    );
    handlers.register(
        PROFILE,
        RETURN_BEACON_CORE,
        can_return_core,
        BEACON_REPLIES,
        speak_about_beacon,
    );
    handlers.register(
        PROFILE,
        OLD_RELAY,
        |journal, _| active(journal, OLD_RELAY),
        RELAY_REPLIES,
        |journal, _| speak_about_relay(journal),
    );
    handlers.register(
        PROFILE,
        REPORT_ARCHIVE_HUSHCAP,
        |journal, _| journal.expects(QuestAction::ReportedArchiveHushcap),
        CAVERN_REPLIES,
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
        CAVERN_REPLIES,
        speak_about_cavern,
    );
    handlers.register(
        PROFILE,
        SHOW_MYCELIUM,
        can_show_samples,
        CAVERN_REPLIES,
        speak_about_cavern,
    );
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

fn speak_about_relay(journal: &QuestJournal) -> ReplyId {
    if !journal.beacon_powered() {
        RELAY_SEALED
    } else if journal.expects(QuestAction::EnteredRelay) {
        RELAY_HATCH
    } else if journal.expects(QuestAction::OpenedArchiveGate) {
        RELAY_SWITCH
    } else {
        RELAY_SURVEY_LOG
    }
}

pub(crate) fn speak_about_beacon(quest: &mut QuestJournal, inventory: &mut Inventory) -> ReplyId {
    let first_meeting = quest.expects(QuestAction::TalkedToNemi);
    if first_meeting {
        quest.record(QuestAction::TalkedToNemi);
    }
    if quest.expects(QuestAction::ReturnedCore) {
        if inventory.consume_core() {
            quest.record(QuestAction::ReturnedCore);
            if first_meeting {
                return EARLY_CORE_RETURNED;
            }
            return CORE_RETURNED;
        }
        if first_meeting {
            return EARLY_CORE_STORED;
        }
        return CORE_MISSING;
    }
    if first_meeting {
        BEACON_BRIEFING
    } else {
        CORE_DIRECTIONS
    }
}

pub(crate) fn speak_about_cavern(quest: &mut QuestJournal, inventory: &mut Inventory) -> ReplyId {
    let reported = quest.expects(QuestAction::ReportedArchiveHushcap);
    if reported {
        quest.record(QuestAction::ReportedArchiveHushcap);
    }
    let ready_to_show = quest.expects(QuestAction::ShowedNemiMycelium);
    if can_show_samples(quest, inventory) {
        quest.record(QuestAction::ShowedNemiMycelium);
        return if reported {
            REPORT_AND_SAMPLES_SHOWN
        } else {
            SAMPLES_SHOWN
        };
    }
    if reported && ready_to_show {
        REPORT_NEEDS_SAMPLES
    } else if reported {
        CAVERN_BRIEFING
    } else if ready_to_show {
        SAMPLES_NEEDED
    } else {
        CAVERN_DIRECTIONS
    }
}
