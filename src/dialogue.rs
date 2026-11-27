use crate::{
    combat::Dead,
    controls::{Control, Keybinds},
    dungeon::Area,
    interaction::{INTERACT_HEIGHT, INTERACT_RANGE, unobstructed},
    inventory::Inventory,
    player::{Player, PlayerWrite},
    quest::QuestJournal,
};
use avian3d::prelude::SpatialQuery;
use bevy::{
    asset::{AssetLoader, LoadContext, io::Reader},
    prelude::*,
};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct DialoguePlugin;

impl Plugin for DialoguePlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<DialogueCatalog>()
            .init_asset_loader::<DialogueLoader>();
    }
}

#[derive(Asset, TypePath, Resource, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DialogueCatalog {
    profiles: Vec<Profile>,
    #[serde(default)]
    notices: Vec<AuthoredLine>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthoredLine {
    id: String,
    text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReplyId(pub(crate) &'static str);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NoticeId(pub(crate) &'static str);

fn line_text<'a>(lines: &'a [AuthoredLine], id: &str) -> Option<&'a str> {
    lines
        .iter()
        .find(|line| line.id == id)
        .map(|line| line.text.as_str())
}

fn validate_lines(lines: &[AuthoredLine], context: &str) -> Result<(), String> {
    let mut ids = BTreeSet::new();
    for line in lines {
        if line.id.trim().is_empty() || !ids.insert(line.id.as_str()) {
            return Err(format!("Blank or duplicate ID {:?} in {context}", line.id));
        }
        if line.text.trim().is_empty() {
            return Err(format!("Blank text for {context}/{}", line.id));
        }
    }
    Ok(())
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Profile {
    id: String,
    pub(crate) name: String,
    #[serde(default)]
    refusal: Option<String>,
    topics: Vec<Topic>,
    #[serde(default)]
    replies: Vec<AuthoredLine>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Topic {
    pub(crate) id: String,
    pub(crate) label: String,
    reply: Reply,
}

#[derive(Clone, Deserialize)]
enum Reply {
    Text(String),
    Quest,
}

impl DialogueCatalog {
    pub(crate) fn profile(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|profile| profile.id == id)
    }

    pub(crate) fn notice_text(&self, id: NoticeId) -> &str {
        line_text(&self.notices, id.0)
            .expect("dialogue loading must validate every required notice ID")
    }

    pub(crate) fn validate_content(&self) -> Result<(), String> {
        validate_lines(&self.notices, "notices")?;
        let mut profiles = BTreeSet::new();
        for profile in &self.profiles {
            if profile.id.trim().is_empty() || !profiles.insert(&profile.id) {
                return Err(format!("Blank or duplicate speaker ID: {:?}", profile.id));
            }
            if profile.name.trim().is_empty()
                || profile.topics.is_empty()
                || profile
                    .refusal
                    .as_ref()
                    .is_some_and(|text| text.trim().is_empty())
            {
                return Err(format!(
                    "Speaker {} needs a name and topics, and any refusal must have text",
                    profile.id
                ));
            }
            validate_lines(&profile.replies, &format!("speaker {} replies", profile.id))?;
            let mut topics = BTreeSet::new();
            for topic in &profile.topics {
                if topic.id.trim().is_empty() || !topics.insert(&topic.id) {
                    return Err(format!(
                        "Blank or duplicate topic ID {:?} for {}",
                        topic.id, profile.id
                    ));
                }
                if topic.label.trim().is_empty()
                    || matches!(&topic.reply, Reply::Text(text) if text.trim().is_empty())
                {
                    return Err(format!(
                        "Topic {}/{} needs a label and reply",
                        profile.id, topic.id
                    ));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn validate_registry(&self, registry: &DialogueRegistry) -> Result<(), String> {
        for profile in &self.profiles {
            for topic in &profile.topics {
                if matches!(topic.reply, Reply::Quest)
                    && registry.get(&profile.id, &topic.id).is_none()
                {
                    return Err(format!(
                        "Missing dialogue handler for {}/{}",
                        profile.id, topic.id
                    ));
                }
            }
        }
        for (&profile, topics) in &registry.topics {
            for (&topic, handler) in topics {
                let Some(definition) = self.profile(profile).filter(|definition| {
                    definition
                        .topic(topic)
                        .is_some_and(|entry| matches!(entry.reply, Reply::Quest))
                }) else {
                    return Err(format!("Missing quest topic {profile}/{topic}"));
                };
                for id in handler.replies {
                    if line_text(&definition.replies, id.0).is_none() {
                        return Err(format!(
                            "Missing reply {profile}/{} for topic {topic}",
                            id.0
                        ));
                    }
                }
            }
        }
        for &id in &registry.notices {
            if line_text(&self.notices, id).is_none() {
                return Err(format!("Missing notice {id}"));
            }
        }
        Ok(())
    }
}

#[derive(Default, TypePath)]
struct DialogueLoader;

impl AssetLoader for DialogueLoader {
    type Asset = DialogueCatalog;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        _: &mut LoadContext<'_>,
    ) -> Result<DialogueCatalog, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let catalog: DialogueCatalog =
            ron::de::from_bytes(&bytes).map_err(std::io::Error::other)?;
        catalog.validate_content().map_err(std::io::Error::other)?;
        Ok(catalog)
    }

    fn extensions(&self) -> &[&str] {
        &["dialogue.ron"]
    }
}

/// Borrows the live game state used to show topics and resolve replies.
pub(crate) struct DialogueContext<J, I> {
    pub(crate) journal: J,
    pub(crate) inventory: I,
}

pub(crate) type DialogueContextRef<'a> = DialogueContext<&'a QuestJournal, &'a Inventory>;
pub(crate) type DialogueContextMut<'a> = DialogueContext<Mut<'a, QuestJournal>, Mut<'a, Inventory>>;

impl DialogueContextMut<'_> {
    fn as_read_only(&self) -> DialogueContextRef<'_> {
        DialogueContext {
            journal: &self.journal,
            inventory: &self.inventory,
        }
    }
}

type TopicAvailable = fn(&DialogueContextRef<'_>) -> bool;
type TopicReply = fn(&mut DialogueContextMut<'_>) -> ReplyId;

/// Registers the Rust IDs that must match `data/dialogue/catalog.dialogue.ron` exactly.
/// Each registered profile and quest topic needs a matching entry.
/// Each `ReplyId` matches an entry in that profile's `replies` list,
/// and each required `NoticeId` matches an entry in the catalogue's `notices` list.
/// Loading rejects missing IDs.
#[derive(Resource, Default)]
pub(crate) struct DialogueRegistry {
    topics: BTreeMap<&'static str, BTreeMap<&'static str, QuestTopic>>,
    notices: BTreeSet<&'static str>,
}

struct QuestTopic {
    available: TopicAvailable,
    replies: &'static [ReplyId],
    reply: TopicReply,
}

impl DialogueRegistry {
    pub(crate) fn require_notices(&mut self, ids: &[NoticeId]) {
        self.notices.extend(ids.iter().map(|id| id.0));
    }

    pub(crate) fn register(
        &mut self,
        profile: &'static str,
        topic: &'static str,
        available: TopicAvailable,
        replies: &'static [ReplyId],
        reply: TopicReply,
    ) {
        assert!(
            self.topics
                .entry(profile)
                .or_default()
                .insert(
                    topic,
                    QuestTopic {
                        available,
                        replies,
                        reply
                    }
                )
                .is_none(),
            "duplicate dialogue handler for {profile}/{topic}",
        );
    }

    fn get(&self, profile: &str, topic: &str) -> Option<&QuestTopic> {
        self.topics.get(profile)?.get(topic)
    }
}

impl Topic {
    #[cfg(feature = "smoke")]
    pub(crate) fn text_reply(&self) -> Option<&str> {
        match &self.reply {
            Reply::Text(text) => Some(text),
            Reply::Quest => None,
        }
    }
}

impl Profile {
    pub(crate) fn reply_text(&self, id: ReplyId) -> &str {
        line_text(&self.replies, id.0)
            .expect("a quest dialogue handler must return a registered reply ID")
    }

    pub(crate) fn topic(&self, id: &str) -> Option<&Topic> {
        self.topics.iter().find(|topic| topic.id == id)
    }

    pub(crate) fn refusal(&self) -> Option<&str> {
        self.refusal.as_deref()
    }

    pub(crate) fn available_topics<'a>(
        &'a self,
        handlers: &DialogueRegistry,
        context: &DialogueContextRef<'_>,
    ) -> impl Iterator<Item = &'a Topic> {
        self.topics.iter().filter(move |topic| match topic.reply {
            Reply::Text(_) => true,
            Reply::Quest => handlers
                .get(&self.id, &topic.id)
                .is_some_and(|handler| (handler.available)(context)),
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Availability {
    #[default]
    Friendly,
    Unavailable,
    Refuses,
}

#[derive(Component)]
pub(crate) struct Speaker {
    pub(crate) profile: &'static str,
    pub(crate) area: Area,
    height: f32,
    pub(crate) availability: Availability,
}

impl Speaker {
    pub(crate) fn new(profile: &'static str, area: Area, height: f32) -> Self {
        Self {
            profile,
            area,
            height,
            availability: Availability::Friendly,
        }
    }

    pub(crate) fn point(&self, transform: &Transform) -> Vec3 {
        transform.translation + Vec3::Y * self.height
    }

    pub(crate) fn can_interact(&self, profile: &Profile) -> bool {
        match self.availability {
            Availability::Friendly => true,
            Availability::Unavailable => false,
            Availability::Refuses => profile.refusal().is_some(),
        }
    }
}

#[derive(Resource, Default)]
pub(crate) struct Dialogue {
    screen: Option<Screen>,
    selected_topic: Option<String>,
    closing: bool,
    revision: u64,
}

pub(crate) enum Screen {
    Notice(String),
    Conversation {
        speaker: Entity,
        profile: &'static str,
        page: Page,
    },
}

pub(crate) enum Page {
    Topics,
    Reply(String),
    Refusal(String),
}

#[derive(Clone, Debug)]
pub(crate) enum Choice {
    Topic(String),
    Continue,
    Goodbye,
}

#[derive(Resource, Default)]
pub(crate) struct DialogueClick(pub(crate) Option<(u64, Choice)>);

impl Dialogue {
    pub(crate) fn is_open(&self) -> bool {
        self.screen.is_some()
    }

    pub(crate) fn screen(&self) -> Option<&Screen> {
        self.screen.as_ref()
    }

    pub(crate) fn selected_topic(&self) -> Option<&str> {
        self.selected_topic.as_deref()
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    #[cfg(test)]
    pub(crate) fn clear(&mut self) {
        self.replace(None);
        self.closing = false;
    }

    pub(crate) fn text(&self) -> Option<&str> {
        match self.screen()? {
            Screen::Notice(text) => Some(text),
            Screen::Conversation {
                page: Page::Reply(text),
                ..
            } => Some(text),
            Screen::Conversation {
                page: Page::Refusal(text),
                ..
            } => Some(text),
            Screen::Conversation {
                page: Page::Topics, ..
            } => None,
        }
    }

    /// Keep input captured through the closing frame so a click cannot start an attack.
    pub(crate) fn request_close(&mut self) {
        self.closing = true;
    }

    pub(crate) fn interrupt(&mut self, entity: Entity) {
        if matches!(self.screen(), Some(Screen::Conversation { speaker, .. }) if *speaker == entity)
        {
            self.request_close();
        }
    }

    pub(crate) fn show_notice(&mut self, catalog: &DialogueCatalog, id: NoticeId) {
        self.replace(Some(Screen::Notice(catalog.notice_text(id).to_owned())));
    }

    #[cfg(test)]
    pub(crate) fn show_notice_text(&mut self, text: &str) {
        self.replace(Some(Screen::Notice(text.to_owned())));
    }

    pub(crate) fn start(
        &mut self,
        entity: Entity,
        speaker: &Speaker,
        profile: &Profile,
        first_topic: Option<&Topic>,
    ) {
        if !speaker.can_interact(profile) {
            return;
        }
        self.selected_topic = first_topic.map(|topic| topic.id.clone());
        let page = if speaker.availability == Availability::Refuses {
            let Some(text) = profile.refusal.clone() else {
                return;
            };
            Page::Refusal(text)
        } else {
            Page::Topics
        };
        self.replace(Some(Screen::Conversation {
            speaker: entity,
            profile: speaker.profile,
            page,
        }));
    }

    fn replace(&mut self, screen: Option<Screen>) {
        self.screen = screen;
        self.revision = self.revision.wrapping_add(1);
    }

    fn set_page(&mut self, speaker: Entity, profile: &'static str, page: Page) {
        self.replace(Some(Screen::Conversation {
            speaker,
            profile,
            page,
        }));
    }
}

pub(crate) fn finish_close(mut dialogue: ResMut<Dialogue>) {
    if dialogue.closing {
        dialogue.replace(None);
        dialogue.closing = false;
    }
}

pub(crate) fn handle_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    bindings: Res<Keybinds>,
    catalog: Res<DialogueCatalog>,
    handlers: Res<DialogueRegistry>,
    mut dialogue: ResMut<Dialogue>,
    mut click: ResMut<DialogueClick>,
    journal: ResMut<QuestJournal>,
    inventory: PlayerWrite<Inventory>,
    speakers: Query<(&Speaker, &Transform), Without<Dead>>,
    players: Query<(Entity, &Player, &Transform), Without<Dead>>,
    spatial: SpatialQuery,
) {
    let clicked = click
        .0
        .take()
        .filter(|(revision, _)| *revision == dialogue.revision());
    let clicked = clicked.map(|(_, choice)| choice);
    if dialogue.closing {
        return;
    }
    let enter = keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]);
    let advance = enter || bindings.just_pressed_any(Control::Interact, &keys, &mouse);
    let Some(screen) = dialogue.screen() else {
        return;
    };
    if keys.just_pressed(KeyCode::Escape) {
        dialogue.request_close();
        return;
    }
    let Screen::Conversation {
        speaker,
        profile,
        page,
    } = screen
    else {
        if advance || matches!(clicked, Some(Choice::Continue)) {
            dialogue.request_close();
        }
        return;
    };
    let (speaker, profile) = (*speaker, *profile);
    let Some(definition) = catalog.profile(profile).filter(|definition| {
        let (Ok((actor, transform)), Ok((player_entity, player, player_transform))) =
            (speakers.get(speaker), players.single())
        else {
            return false;
        };
        let expected = match page {
            Page::Refusal(_) => Availability::Refuses,
            Page::Reply(_) | Page::Topics => Availability::Friendly,
        };
        if actor.profile != profile
            || actor.availability != expected
            || actor.area != player.area
            || !actor.can_interact(definition)
        {
            return false;
        }
        let origin = player_transform.translation + Vec3::Y * INTERACT_HEIGHT;
        let delta = actor.point(transform) - origin;
        let distance = delta.length();
        distance < INTERACT_RANGE
            && unobstructed(&spatial, origin, delta, distance, player_entity, speaker)
    }) else {
        dialogue.request_close();
        return;
    };

    let mut context = DialogueContext {
        journal: journal.into(),
        inventory: inventory.into_inner(),
    };
    match page {
        Page::Refusal(_) if advance || matches!(clicked, Some(Choice::Continue)) => {
            dialogue.request_close()
        }
        Page::Reply(_) if advance || matches!(clicked, Some(Choice::Continue)) => {
            let context = context.as_read_only();
            let mut topics = definition.available_topics(&handlers, &context);
            let first = topics.next();
            if let Some(id) = dialogue.selected_topic()
                && !first.is_some_and(|topic| topic.id == id)
                && !topics.any(|topic| topic.id == id)
            {
                dialogue.selected_topic = first.map(|topic| topic.id.clone());
            }
            dialogue.set_page(speaker, profile, Page::Topics);
        }
        Page::Topics => {
            handle_topics(
                dialogue.reborrow(),
                (speaker, profile),
                definition,
                &handlers,
                &mut context,
                &keys,
                &bindings,
                clicked,
                enter,
            );
        }
        _ => (),
    }
}

fn handle_topics(
    mut dialogue: Mut<Dialogue>,
    (speaker, profile_id): (Entity, &'static str),
    profile: &Profile,
    handlers: &DialogueRegistry,
    context: &mut DialogueContextMut<'_>,
    keys: &ButtonInput<KeyCode>,
    bindings: &Keybinds,
    clicked: Option<Choice>,
    enter: bool,
) {
    let topics: Vec<_> = profile
        .available_topics(handlers, &context.as_read_only())
        .collect();
    let selection = dialogue.selected_topic();
    let selected = selection.map_or(Some(topics.len()), |id| {
        topics.iter().position(|topic| topic.id == id)
    });
    // Enter refers to the highlighted topic ID even if the available rows just changed.
    if let Some(choice) = clicked
        .or_else(|| enter.then(|| selection.map_or(Choice::Goodbye, |id| Choice::Topic(id.into()))))
    {
        match choice {
            Choice::Goodbye => {
                dialogue.request_close();
                return;
            }
            Choice::Continue => return,
            Choice::Topic(id) => {
                if let Some(topic) = topics.iter().find(|topic| topic.id == id) {
                    let reply = match &topic.reply {
                        Reply::Text(text) => text.clone(),
                        Reply::Quest => {
                            let Some(handler) = handlers.get(&profile.id, &id) else {
                                return;
                            };
                            let id = (handler.reply)(context);
                            assert!(
                                handler.replies.contains(&id),
                                "dialogue handler {profile_id}/{id:?} must register every reply it can return"
                            );
                            profile.reply_text(id).to_owned()
                        }
                    };
                    dialogue.selected_topic = Some(id);
                    dialogue.set_page(speaker, profile_id, Page::Reply(reply));
                    return;
                }
            }
        }
    }
    let up = bindings.just_pressed(Control::MenuUp, keys);
    let down = bindings.just_pressed(Control::MenuDown, keys);
    let next = match selected {
        Some(index) if !enter && up => index.checked_sub(1).unwrap_or(topics.len()),
        Some(index) if !enter && down => (index + 1) % (topics.len() + 1),
        Some(_) => return,
        None => 0,
    };
    dialogue.selected_topic = topics.get(next).map(|topic| topic.id.clone());
}

#[cfg(test)]
pub(crate) fn test_catalog() -> DialogueCatalog {
    ron::from_str(include_str!("../data/dialogue/catalog.dialogue.ron"))
        .expect("the shipped dialogue catalogue must parse")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_catalogue_matches_registered_quest_topics() {
        let mut catalog = test_catalog();
        let handlers = shipped_registry();
        assert!(catalog.validate_content().is_ok());
        assert!(catalog.validate_registry(&handlers).is_ok());
        assert!(
            catalog
                .validate_registry(&DialogueRegistry::default())
                .is_err()
        );
        profile_mut(&mut catalog, "nemi").topics.remove(0);
        assert!(catalog.validate_registry(&handlers).is_err());
    }

    fn shipped_registry() -> DialogueRegistry {
        let mut registry = DialogueRegistry::default();
        crate::nemi::dialogue::register_topics(&mut registry);
        crate::crossing::register_topics(&mut registry);
        registry.require_notices(crate::interaction::BEACON_NOTICES);
        registry.require_notices(crate::dungeon::RELAY_NOTICES);
        registry
    }

    #[test]
    fn missing_lines_report_their_reply_or_notice_id() {
        let mut catalog = test_catalog();
        let registry = shipped_registry();
        catalog
            .notices
            .retain(|line| line.id != "old_relay.survey_log");
        assert_eq!(
            catalog.validate_registry(&registry),
            Err("Missing notice old_relay.survey_log".into())
        );

        let nemi = profile_mut(&mut catalog, "nemi");
        nemi.replies.retain(|line| line.id != "early_core_returned");
        let error = catalog
            .validate_registry(&registry)
            .expect_err("a missing reply must fail loading");
        assert!(error.contains("nemi/early_core_returned"));
    }

    #[test]
    fn duplicate_and_blank_authored_lines_report_their_ids() {
        let mut catalog = test_catalog();
        let first = catalog.notices[0].clone();
        catalog.notices.push(first.clone());
        let error = catalog
            .validate_content()
            .expect_err("duplicate notice IDs must fail loading");
        assert!(error.contains(&first.id));
        catalog.notices.pop();

        let reply = &mut profile_mut(&mut catalog, "nemi").replies[0];
        reply.text.clear();
        let id = reply.id.clone();
        let error = catalog
            .validate_content()
            .expect_err("blank reply text must fail loading");
        assert!(error.contains("nemi") && error.contains(&id));
    }

    fn profile_mut<'a>(catalog: &'a mut DialogueCatalog, id: &str) -> &'a mut Profile {
        catalog
            .profiles
            .iter_mut()
            .find(|profile| profile.id == id)
            .expect("test dialogue profile must exist")
    }

    #[test]
    fn duplicate_ids_and_blank_replies_are_rejected() {
        let mut catalog: DialogueCatalog = ron::from_str(
            r#"(profiles: [
            (id: "traveller", name: "Traveller", topics: [
                (id: "route", label: "Route", reply: Text("Follow the blue lamps.")),
            ]),
        ])"#,
        )
        .expect("the traveller profile must parse");
        let profile = catalog.profiles[0].clone();
        catalog.profiles.push(profile.clone());
        assert!(catalog.validate_content().is_err());
        catalog.profiles.pop();
        catalog.profiles[0].topics.push(profile.topics[0].clone());
        assert!(catalog.validate_content().is_err());
        catalog.profiles[0].topics.pop();
        catalog.profiles[0].topics[0].reply = Reply::Text(" ".into());
        assert!(catalog.validate_content().is_err());
    }
}
