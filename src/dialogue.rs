use crate::{
    combat::Dead,
    controls::{Control, Keybinds},
    dungeon::Area,
    interaction::{INTERACT_HEIGHT, INTERACT_RANGE, unobstructed},
    inventory::Inventory,
    player::Player,
    quest::QuestJournal,
};
use avian3d::prelude::SpatialQuery;
use bevy::{
    asset::{AssetLoader, LoadContext, io::Reader},
    prelude::*,
};
use serde::Deserialize;
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};

pub(crate) struct DialoguePlugin;

impl Plugin for DialoguePlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<DialogueCatalog>()
            .init_asset_loader::<DialogueLoader>();
    }
}

#[derive(Asset, TypePath, Resource, Clone, Default, Deserialize)]
#[serde(transparent)]
pub(crate) struct DialogueCatalog(Vec<Profile>);

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Profile {
    id: String,
    pub(crate) name: String,
    #[serde(default)]
    refusal: Option<String>,
    topics: Vec<Topic>,
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
        self.0.iter().find(|profile| profile.id == id)
    }

    fn validate_content(&self) -> Result<(), String> {
        let mut profiles = BTreeSet::new();
        for profile in &self.0 {
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

    pub(crate) fn validate_handlers(&self, handlers: &DialogueHandlers) -> Result<(), String> {
        for profile in &self.0 {
            for topic in &profile.topics {
                if matches!(topic.reply, Reply::Quest)
                    && handlers.get(&profile.id, &topic.id).is_none()
                {
                    return Err(format!(
                        "Missing dialogue handler for {}/{}",
                        profile.id, topic.id
                    ));
                }
            }
        }
        for (&profile, topics) in &handlers.0 {
            for &topic in topics.keys() {
                if !self.profile(profile).is_some_and(|definition| {
                    definition
                        .topics
                        .iter()
                        .any(|entry| entry.id == topic && matches!(entry.reply, Reply::Quest))
                }) {
                    return Err(format!("Missing quest topic {profile}/{topic}"));
                }
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

#[derive(Resource, Default)]
pub(crate) struct DialogueHandlers(BTreeMap<&'static str, BTreeMap<&'static str, QuestTopic>>);

struct QuestTopic {
    available: fn(&QuestJournal) -> bool,
    reply: fn(&mut QuestJournal, &mut Inventory) -> &'static str,
}

impl DialogueHandlers {
    pub(crate) fn register(
        &mut self,
        profile: &'static str,
        topic: &'static str,
        available: fn(&QuestJournal) -> bool,
        reply: fn(&mut QuestJournal, &mut Inventory) -> &'static str,
    ) {
        assert!(
            self.0
                .entry(profile)
                .or_default()
                .insert(topic, QuestTopic { available, reply })
                .is_none(),
            "duplicate dialogue handler for {profile}/{topic}",
        );
    }

    fn get(&self, profile: &str, topic: &str) -> Option<&QuestTopic> {
        self.0.get(profile)?.get(topic)
    }
}

impl Profile {
    pub(crate) fn available_topics<'a>(
        &'a self,
        handlers: &DialogueHandlers,
        journal: &QuestJournal,
    ) -> impl Iterator<Item = &'a Topic> {
        self.topics.iter().filter(move |topic| match topic.reply {
            Reply::Text(_) => true,
            Reply::Quest => handlers
                .get(&self.id, &topic.id)
                .is_some_and(|handler| (handler.available)(journal)),
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
            Availability::Refuses => profile.refusal.is_some(),
        }
    }
}

#[derive(Resource, Default)]
pub(crate) struct Dialogue {
    screen: Option<Screen>,
    selected: usize,
    closing: bool,
    revision: u64,
}

pub(crate) enum Screen {
    Notice(&'static str),
    Conversation {
        speaker: Entity,
        profile: &'static str,
        page: Page,
    },
}

pub(crate) enum Page {
    Topics,
    Reply(Cow<'static, str>),
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

    pub(crate) fn selected(&self) -> usize {
        self.selected
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

    pub(crate) fn show_notice(&mut self, text: &'static str) {
        self.replace(Some(Screen::Notice(text)));
    }

    pub(crate) fn start(&mut self, entity: Entity, speaker: &Speaker, profile: &Profile) {
        if !speaker.can_interact(profile) {
            return;
        }
        self.selected = 0;
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
    bindings: Res<Keybinds>,
    catalog: Res<DialogueCatalog>,
    handlers: Res<DialogueHandlers>,
    mut dialogue: ResMut<Dialogue>,
    mut click: ResMut<DialogueClick>,
    mut journal: ResMut<QuestJournal>,
    mut inventory: ResMut<Inventory>,
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
    let advance = enter || bindings.just_pressed(Control::Interact, &keys);
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

    match page {
        Page::Refusal(_) if advance || matches!(clicked, Some(Choice::Continue)) => {
            dialogue.request_close()
        }
        Page::Reply(_) if advance || matches!(clicked, Some(Choice::Continue)) => {
            dialogue.set_page(speaker, profile, Page::Topics);
        }
        Page::Topics => {
            let topics: Vec<_> = definition.available_topics(&handlers, &journal).collect();
            let selected = dialogue.selected.min(topics.len());
            if let Some(choice) = clicked.or_else(|| {
                enter.then(|| {
                    topics
                        .get(selected)
                        .map_or(Choice::Goodbye, |topic| Choice::Topic(topic.id.clone()))
                })
            }) {
                match choice {
                    Choice::Goodbye => dialogue.request_close(),
                    Choice::Continue => (),
                    Choice::Topic(id) => {
                        if let Some(index) = topics.iter().position(|topic| topic.id == id) {
                            dialogue.selected = index;
                            let reply = match &topics[index].reply {
                                Reply::Text(text) => Cow::Owned(text.clone()),
                                Reply::Quest => {
                                    let Some(handler) = handlers.get(profile, &id) else {
                                        return;
                                    };
                                    Cow::Borrowed((handler.reply)(&mut journal, &mut inventory))
                                }
                            };
                            dialogue.set_page(speaker, profile, Page::Reply(reply));
                        }
                    }
                }
            } else {
                let next = if bindings.just_pressed(Control::MenuUp, &keys) {
                    selected.checked_sub(1).unwrap_or(topics.len())
                } else if bindings.just_pressed(Control::MenuDown, &keys) {
                    (selected + 1) % (topics.len() + 1)
                } else {
                    selected
                };
                if dialogue.selected != next {
                    dialogue.selected = next;
                }
            }
        }
        _ => (),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_catalogue_matches_registered_quest_topics() {
        let mut catalog: DialogueCatalog =
            ron::from_str(include_str!("../data/dialogue/catalog.dialogue.ron"))
                .expect("the shipped dialogue catalogue must parse");
        let mut handlers = DialogueHandlers::default();
        crate::nemi::dialogue::register_topics(&mut handlers);
        assert!(catalog.validate_content().is_ok());
        assert!(catalog.validate_handlers(&handlers).is_ok());
        assert!(
            catalog
                .validate_handlers(&DialogueHandlers::default())
                .is_err()
        );
        catalog.0[0].topics.remove(0);
        assert!(catalog.validate_handlers(&handlers).is_err());
    }

    #[test]
    fn duplicate_ids_and_blank_replies_are_rejected() {
        let mut catalog: DialogueCatalog = ron::from_str(
            r#"[
            (id: "traveller", name: "Traveller", topics: [
                (id: "route", label: "Route", reply: Text("Follow the blue lamps.")),
            ]),
        ]"#,
        )
        .expect("the traveller profile must parse");
        let profile = catalog.0[0].clone();
        catalog.0.push(profile.clone());
        assert!(catalog.validate_content().is_err());
        catalog.0.pop();
        catalog.0[0].topics.push(profile.topics[0].clone());
        assert!(catalog.validate_content().is_err());
        catalog.0[0].topics.pop();
        catalog.0[0].topics[0].reply = Reply::Text(" ".into());
        assert!(catalog.validate_content().is_err());
    }
}
