use crate::{
    GameState,
    combat::Dead,
    controls::{Control, Keybinds},
    dialogue::{self, Dialogue, DialogueCatalog, DialogueClick, DialogueHandlers, Speaker},
    dungeon::{Area, DungeonTarget, UseDungeonTarget},
    inventory::{Inventory, ItemKind, Pickup},
    player::Player,
    quest::{QuestAction, QuestJournal},
    save::{Bed, SaveAtBed},
    storage::ChestId,
    ui::menu_ui::PlayerMenu,
};
use avian3d::prelude::*;
use bevy::{ecs::query::QueryData, prelude::*};

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Core,
    BeaconSocket,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionTarget {
    Quest(Target),
    Talk,
    Pickup(Pickup),
    Dungeon(DungeonTarget),
    Corpse,
    Chest(ChestId),
    Bed,
}
#[derive(Component)]
pub(crate) struct CorpseLoot {
    pub(crate) stack: Option<Pickup>,
    pub(crate) area: Area,
}

impl CorpseLoot {
    pub(crate) fn new(item: ItemKind, area: Area) -> Self {
        Self {
            stack: Some(Pickup { item, quantity: 1 }),
            area,
        }
    }
}

pub(crate) const INTERACT_RANGE: f32 = 3.0;
pub(crate) const INTERACT_HEIGHT: f32 = 0.45;
const PICKUP_INTERACTION_OFFSET: f32 = 0.18;
const HATCH_INTERACTION_OFFSET: f32 = 0.65;

#[derive(Resource, Default)]
pub(crate) struct LootTarget(pub(crate) Option<Entity>);

#[derive(Resource, Default)]
pub(crate) struct ActiveChest(pub(crate) Option<Entity>);

#[derive(Resource, Default, PartialEq, Eq)]
pub struct Nearby(pub Option<(Entity, InteractionTarget)>);
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InteractionSystems;

pub struct InteractionPlugin;
impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Nearby>()
            .init_resource::<Dialogue>()
            .init_resource::<DialogueCatalog>()
            .init_resource::<DialogueHandlers>()
            .init_resource::<DialogueClick>()
            .init_resource::<LootTarget>()
            .init_resource::<ActiveChest>()
            .add_message::<SaveAtBed>()
            .add_systems(
                Update,
                (dialogue::handle_input, find_target, interact)
                    .chain()
                    .in_set(InteractionSystems)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(Last, dialogue::finish_close);
    }
}
/// Collects the interaction capabilities that may be present on an entity.
#[derive(QueryData)]
struct Candidate {
    entity: Entity,
    transform: &'static Transform,
    quest: Option<&'static Target>,
    pickup: Option<&'static Pickup>,
    dungeon: Option<&'static DungeonTarget>,
    corpse: Option<&'static CorpseLoot>,
    dead: Has<Dead>,
    speaker: Option<&'static Speaker>,
    chest: Option<&'static ChestId>,
    bed: Has<Bed>,
}

impl CandidateItem<'_, '_> {
    fn interaction(
        &self,
        area: Area,
        quest: &QuestJournal,
        catalog: &DialogueCatalog,
    ) -> Option<(InteractionTarget, Vec3)> {
        if let Some(speaker) = self.speaker.filter(|speaker| {
            !self.dead
                && speaker.area == area
                && catalog
                    .profile(speaker.profile)
                    .is_some_and(|profile| speaker.can_interact(profile))
        }) {
            return Some((InteractionTarget::Talk, speaker.point(self.transform)));
        }
        let (interaction, height) = if let Some(&target) = self.quest {
            if area != Area::Surface {
                return None;
            }
            let height = match target {
                Target::Core if !quest.has_occurred(QuestAction::RecoveredCore) => 0.0,
                Target::BeaconSocket if quest.expects(QuestAction::InstalledCore) => 0.0,
                _ => return None,
            };
            (InteractionTarget::Quest(target), height)
        } else if let Some(&pickup) = self.pickup {
            (InteractionTarget::Pickup(pickup), PICKUP_INTERACTION_OFFSET)
        } else if let Some(&dungeon) = self.dungeon {
            if area != dungeon.area() {
                return None;
            }
            let height = match dungeon {
                DungeonTarget::Hatch | DungeonTarget::Exit => HATCH_INTERACTION_OFFSET,
                _ => 0.0,
            };
            (InteractionTarget::Dungeon(dungeon), height)
        } else if self
            .corpse
            .is_some_and(|loot| self.dead && loot.area == area && loot.stack.is_some())
        {
            (InteractionTarget::Corpse, 0.0)
        } else if let Some(&chest) = self.chest.filter(|_| area == Area::Surface) {
            (InteractionTarget::Chest(chest), 0.0)
        } else if self.bed && area == Area::Surface {
            (InteractionTarget::Bed, 0.0)
        } else {
            return None;
        };
        Some((interaction, self.transform.translation + Vec3::Y * height))
    }
}

fn find_target(
    players: Query<(Entity, &Transform, &Player), Without<Dead>>,
    targets: Query<
        Candidate,
        Or<(
            With<Target>,
            With<Speaker>,
            With<Pickup>,
            With<DungeonTarget>,
            With<CorpseLoot>,
            With<ChestId>,
            With<Bed>,
        )>,
    >,
    quest: Res<QuestJournal>,
    catalog: Res<DialogueCatalog>,
    dialogue: Res<Dialogue>,
    spatial: SpatialQuery,
    log: Option<Res<PlayerMenu>>,
    mut nearby: ResMut<Nearby>,
) {
    if dialogue.is_open() || log.is_some_and(|log| log.is_open()) {
        nearby.set_if_neq(Nearby::default());
        return;
    }
    let Ok((player_entity, transform, player)) = players.single() else {
        nearby.set_if_neq(Nearby::default());
        return;
    };
    let mut selected = None;
    let origin = transform.translation + Vec3::Y * INTERACT_HEIGHT;
    let mut closest = INTERACT_RANGE;
    for candidate in &targets {
        let Some((interaction, point)) = candidate.interaction(player.area, &quest, &catalog)
        else {
            continue;
        };
        let entity = candidate.entity;
        let delta = point - origin;
        let distance = delta.length();
        if distance >= closest {
            continue;
        }
        if unobstructed(&spatial, origin, delta, distance, player_entity, entity) {
            closest = distance;
            selected = Some((entity, interaction));
        }
    }
    nearby.set_if_neq(Nearby(selected));
}

/// Check line of sight between the player and target, excluding their own colliders.
pub(crate) fn unobstructed(
    spatial: &SpatialQuery,
    origin: Vec3,
    delta: Vec3,
    distance: f32,
    player: Entity,
    target: Entity,
) -> bool {
    Dir3::new(delta).is_ok_and(|direction| {
        spatial
            .cast_ray(
                origin,
                direction,
                distance,
                true,
                &SpatialQueryFilter::from_excluded_entities([player, target]),
            )
            .is_none()
    })
}
fn interact(
    keys: Res<ButtonInput<KeyCode>>,
    bindings: Res<Keybinds>,
    nearby: Res<Nearby>,
    mut dialogue: ResMut<Dialogue>,
    mut quest: ResMut<QuestJournal>,
    mut inventory: ResMut<Inventory>,
    mut commands: Commands,
    pickups: Query<&Pickup>,
    speakers: Query<&Speaker, Without<Dead>>,
    catalog: Res<DialogueCatalog>,
    mut loot: ResMut<LootTarget>,
    mut chest: ResMut<ActiveChest>,
    mut saves: MessageWriter<SaveAtBed>,
    mut next: ResMut<NextState<GameState>>,
    log: Option<Res<PlayerMenu>>,
) {
    if dialogue.is_open()
        || keys.just_pressed(KeyCode::Escape)
        || log.is_some_and(|log| log.is_open())
    {
        return;
    }
    let interact_pressed = bindings.just_pressed(Control::Interact, &keys);
    if !interact_pressed && !keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) {
        return;
    }
    if let Some((entity, target)) = nearby.0 {
        match target {
            InteractionTarget::Talk => {
                if let Ok(speaker) = speakers.get(entity)
                    && let Some(profile) = catalog.profile(speaker.profile)
                {
                    dialogue.start(entity, speaker, profile);
                }
            }
            InteractionTarget::Quest(Target::Core) => {
                if !quest.has_occurred(QuestAction::RecoveredCore)
                    && inventory.add(ItemKind::BeaconCore, 1)
                {
                    quest.record(QuestAction::RecoveredCore);
                    dialogue.show_notice(if quest.expects(QuestAction::InstalledCore) {
                        "POWER CORE RECOVERED\n\nInstall it in the socket at the south side of the beacon tower."
                    } else if quest.expects(QuestAction::TalkedToNemi) {
                        "POWER CORE RECOVERED\n\nIts light pulses against your hands. There is still warmth inside.\n\nSpeak to Nemi at the outpost about restoring the beacon."
                    } else {
                        "POWER CORE RECOVERED\n\nIts light pulses against your hands. There is still warmth inside.\n\nReturn to Nemi at the outpost to restore the beacon."
                    });
                    finish_pickup(&mut commands, entity);
                }
            }
            InteractionTarget::Quest(Target::BeaconSocket) => {
                dialogue.show_notice(install_core(&mut quest, &mut inventory));
            }
            InteractionTarget::Pickup(_) if interact_pressed => {
                // Re-read the live component before transferring the stack.
                if let Ok(pickup) = pickups.get(entity) {
                    if inventory.add(pickup.item, pickup.quantity) {
                        finish_pickup(&mut commands, entity);
                    }
                }
            }
            InteractionTarget::Dungeon(target) => commands.trigger(UseDungeonTarget(target)),
            InteractionTarget::Corpse
                if interact_pressed && matches!(*next, NextState::Unchanged) =>
            {
                loot.0 = Some(entity);
                next.set(GameState::Looting);
            }
            InteractionTarget::Chest(_)
                if interact_pressed && matches!(*next, NextState::Unchanged) =>
            {
                chest.0 = Some(entity);
                next.set(GameState::Storage);
            }
            InteractionTarget::Bed if interact_pressed && matches!(*next, NextState::Unchanged) => {
                saves.write(SaveAtBed(entity));
            }
            _ => (),
        }
    }
}

fn finish_pickup(commands: &mut Commands, entity: Entity) {
    commands.entity(entity).despawn();
    commands.trigger(crate::combat_audio::ItemPickedUp);
}

fn install_core(quest: &mut QuestJournal, inventory: &mut Inventory) -> &'static str {
    if quest.expects(QuestAction::InstalledCore) && inventory.consume_core() {
        quest.record(QuestAction::InstalledCore);
        "BEACON RESTORED\n\nThe core locks into the socket and the tower lights up. The Old Relay hatch northwest of the tower now has power."
    } else {
        "BEACON SOCKET\n\nThe socket needs a charged beacon core."
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        nemi::{
            self,
            dialogue::{register_topics, speak_about_beacon, speak_about_cavern},
        },
        quest::{CONTAIN_THE_HUSHCAPS, LAST_LIGHT},
        storage::{ChestStorage, StoragePlugin},
        ui::storage_ui::{StorageAction, StorageSide, StorageUiPlugin, TransferAmount},
    };
    use bevy::{
        asset::AssetPlugin, input::mouse::AccumulatedMouseScroll, mesh::MeshPlugin,
        state::app::StatesPlugin, time::TimeUpdateStrategy, transform::TransformPlugin,
    };
    use std::time::Duration;
    #[test]
    fn turn_in_requires_and_consumes_the_core() {
        let mut journal = crate::quest::test_journal();
        let mut inventory = Inventory::default();
        speak_about_beacon(&mut journal, &mut inventory);
        journal.record(QuestAction::RecoveredCore);
        assert!(inventory.add(ItemKind::BeaconCore, 1));
        let mut chest = ChestStorage::default();
        let supplies = chest.get_mut(ChestId::Supply);
        assert!(inventory.transfer_to(supplies, ItemKind::BeaconCore, 1));
        speak_about_beacon(&mut journal, &mut inventory);
        assert!(journal.expects(QuestAction::ReturnedCore));
        assert!(supplies.transfer_to(&mut inventory, ItemKind::BeaconCore, 1));
        speak_about_beacon(&mut journal, &mut inventory);
        assert!(journal.is_complete(LAST_LIGHT));
        assert_eq!(inventory.count(ItemKind::BeaconCore), 0);
    }

    #[test]
    fn an_early_core_in_storage_advances_to_turn_in_on_the_first_meeting() {
        let mut journal = crate::quest::test_journal();
        let mut inventory = Inventory::default();
        assert!(inventory.add(ItemKind::BeaconCore, 1));
        journal.record(QuestAction::RecoveredCore);
        let mut chest = ChestStorage::default();
        let supplies = chest.get_mut(ChestId::Supply);
        assert!(inventory.transfer_to(supplies, ItemKind::BeaconCore, 1));

        let dialogue = speak_about_beacon(&mut journal, &mut inventory);
        assert!(dialogue.contains("Bring the core to me"));
        assert!(journal.expects(QuestAction::ReturnedCore));
        assert!(!journal.beacon_powered());
        assert_eq!(supplies.count(ItemKind::BeaconCore), 1);

        assert!(supplies.transfer_to(&mut inventory, ItemKind::BeaconCore, 1));
        speak_about_beacon(&mut journal, &mut inventory);
        assert!(journal.is_complete(LAST_LIGHT));
        assert_eq!(inventory.count(ItemKind::BeaconCore), 0);
    }

    #[test]
    fn installing_the_core_consumes_it_once_and_requires_the_solo_objective() {
        let mut journal = crate::quest::test_journal();
        let mut inventory = Inventory::default();
        journal.record(QuestAction::TalkedToNemi);
        assert!(inventory.add(ItemKind::BeaconCore, 1));
        journal.record(QuestAction::RecoveredCore);
        install_core(&mut journal, &mut inventory);
        assert_eq!(inventory.count(ItemKind::BeaconCore), 1);
        journal.record(QuestAction::AttackedNemi);
        install_core(&mut journal, &mut inventory);
        assert!(journal.beacon_powered());
        assert_eq!(inventory.count(ItemKind::BeaconCore), 0);
        assert!(inventory.add(ItemKind::BeaconCore, 1));
        install_core(&mut journal, &mut inventory);
        assert_eq!(inventory.count(ItemKind::BeaconCore), 1);
    }

    #[test]
    fn nemi_checks_the_backpack_and_leaves_the_mycelium_with_the_player() {
        let mut app = test_app();
        let mut journal = crate::quest::test_relay_journal();
        journal.offer_cavern_report();
        let world = app.world_mut();
        world.insert_resource(journal);
        world.spawn((nemi::speaker(), Transform::from_xyz(0.0, 0.0, -2.0)));
        assert!(world.resource_mut::<Inventory>().add(ItemKind::Mycelium, 4));
        assert!(
            world
                .resource_mut::<ChestStorage>()
                .get_mut(ChestId::Supply)
                .add(ItemKind::Mycelium, 1)
        );

        press_interact(&mut app);
        press_key(&mut app, KeyCode::KeyS);
        press_key(&mut app, KeyCode::Enter);
        assert!(
            app.world()
                .resource::<Dialogue>()
                .text()
                .is_some_and(|text| text.contains("South Basin"))
        );
        let mut journal = app.world_mut().resource_mut::<QuestJournal>();
        journal.record(QuestAction::DefeatedFiveCavernHushcaps);
        assert!(journal.expects(QuestAction::ShowedNemiMycelium));
        press_interact(&mut app);
        press_key(&mut app, KeyCode::Enter);
        assert!(
            !app.world()
                .resource::<QuestJournal>()
                .is_complete(CONTAIN_THE_HUSHCAPS)
        );
        assert!(
            app.world()
                .resource::<Dialogue>()
                .text()
                .is_some_and(|text| text.contains("backpack"))
        );

        app.world_mut()
            .resource_scope(|world, mut storage: Mut<ChestStorage>| {
                assert!(storage.get_mut(ChestId::Supply).transfer_to(
                    &mut world.resource_mut::<Inventory>(),
                    ItemKind::Mycelium,
                    1
                ));
            });
        press_interact(&mut app);
        press_key(&mut app, KeyCode::Enter);
        assert!(
            app.world()
                .resource::<QuestJournal>()
                .is_complete(CONTAIN_THE_HUSHCAPS)
        );
        assert_eq!(
            app.world()
                .resource::<Inventory>()
                .count(ItemKind::Mycelium),
            5
        );
        assert_eq!(
            app.world_mut()
                .resource_mut::<QuestJournal>()
                .take_completion_xp(),
            150
        );
        press_interact(&mut app);
        press_key(&mut app, KeyCode::Enter);
        assert_eq!(
            app.world()
                .resource::<Inventory>()
                .count(ItemKind::Mycelium),
            5
        );
        assert_eq!(
            app.world_mut()
                .resource_mut::<QuestJournal>()
                .take_completion_xp(),
            0
        );
    }

    #[test]
    fn early_cavern_progress_waits_for_the_hatch_exit_and_report() {
        let mut journal = crate::quest::test_relay_journal();
        let mut inventory = Inventory::default();
        assert!(inventory.add(ItemKind::Mycelium, 6));
        journal.record(QuestAction::DefeatedFiveCavernHushcaps);
        assert_eq!(journal.status(CONTAIN_THE_HUSHCAPS), None);
        journal.offer_cavern_report();
        assert!(journal.expects(QuestAction::ReportedArchiveHushcap));
        assert!(!journal.expects(QuestAction::ShowedNemiMycelium));
        let response = speak_about_cavern(&mut journal, &mut inventory);
        assert!(response.contains("mushroom creature") && response.contains("QUEST COMPLETE"));
        assert!(journal.is_complete(CONTAIN_THE_HUSHCAPS));
        assert_eq!(inventory.count(ItemKind::Mycelium), 6);
        assert_eq!(journal.take_completion_xp(), 150);
    }

    #[test]
    fn the_briefing_recognizes_early_defeats_when_samples_are_still_missing() {
        let mut journal = crate::quest::test_relay_journal();
        let mut inventory = Inventory::default();
        journal.record(QuestAction::DefeatedFiveCavernHushcaps);
        journal.offer_cavern_report();

        let response = speak_about_cavern(&mut journal, &mut inventory);
        assert!(response.contains("already thinned it") && response.contains("five Mycelium"));
        assert!(journal.expects(QuestAction::ShowedNemiMycelium));
        assert_eq!(journal.take_completion_xp(), 0);
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            TransformPlugin,
            AssetPlugin::default(),
            MeshPlugin,
            PhysicsPlugins::default(),
        ))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
            1.0 / 60.0,
        )))
        .init_state::<GameState>()
        .insert_resource(crate::quest::test_journal())
        .init_resource::<Dialogue>()
        .init_resource::<Inventory>()
        .init_resource::<Keybinds>()
        .init_resource::<PlayerMenu>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<AccumulatedMouseScroll>()
        .add_plugins((
            InteractionPlugin,
            crate::ui::loot_ui::LootUiPlugin,
            crate::ui::dialogue_ui::DialogueUiPlugin,
            StoragePlugin,
            StorageUiPlugin,
        ));
        let catalog: DialogueCatalog =
            ron::from_str(include_str!("../data/dialogue/catalog.dialogue.ron"))
                .expect("the shipped dialogue catalogue must parse");
        app.insert_resource(catalog);
        register_topics(&mut app.world_mut().resource_mut::<DialogueHandlers>());
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        app.world_mut()
            .spawn((Player::default(), Transform::default()));
        app.finish();
        app.cleanup();
        for _ in 0..3 {
            app.update();
        }
        app
    }
    fn press_interact(app: &mut App) {
        press_key(app, KeyCode::KeyE);
    }

    fn press_key(app: &mut App, key: KeyCode) {
        let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        input.reset_all();
        input.press(key);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.update();
    }

    #[test]
    fn bed_requires_the_interact_key_and_clear_reach() {
        let mut app = test_app();
        let bed = app
            .world_mut()
            .spawn((Bed, Transform::from_xyz(0.0, 0.45, -4.0)))
            .id();
        let mut saves = app
            .world()
            .resource::<Messages<SaveAtBed>>()
            .get_cursor_current();
        press_interact(&mut app);
        assert_eq!(
            saves
                .read(app.world().resource::<Messages<SaveAtBed>>())
                .count(),
            0
        );

        app.world_mut()
            .get_mut::<Transform>(bed)
            .expect("bed position")
            .translation
            .z = -2.0;
        let wall = app
            .world_mut()
            .spawn((
                RigidBody::Static,
                Collider::cuboid(3.0, 3.0, 0.2),
                Transform::from_xyz(0.0, 0.0, -1.0),
            ))
            .id();
        for _ in 0..3 {
            app.update();
        }
        press_interact(&mut app);
        assert_eq!(
            saves
                .read(app.world().resource::<Messages<SaveAtBed>>())
                .count(),
            0
        );

        app.world_mut().despawn(wall);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);
        app.update();
        assert_eq!(
            app.world().resource::<Nearby>().0,
            Some((bed, InteractionTarget::Bed))
        );
        assert_eq!(
            saves
                .read(app.world().resource::<Messages<SaveAtBed>>())
                .count(),
            0
        );
        press_interact(&mut app);
        assert_eq!(
            saves
                .read(app.world().resource::<Messages<SaveAtBed>>())
                .map(|event| event.0)
                .next(),
            Some(bed)
        );
    }

    #[test]
    fn the_player_can_collect_the_core_and_turn_it_in_at_the_first_meeting() {
        let mut app = test_app();
        let core = app
            .world_mut()
            .spawn((Target::Core, Transform::from_xyz(0.0, 0.45, -1.0)))
            .id();
        app.world_mut()
            .spawn((nemi::speaker(), Transform::from_xyz(0.0, 0.0, -2.0)));

        press_interact(&mut app);
        let world = app.world();
        let journal = world.resource::<QuestJournal>();
        assert!(journal.has_occurred(QuestAction::RecoveredCore));
        assert!(journal.expects(QuestAction::TalkedToNemi));
        assert!(!journal.beacon_powered());
        assert_eq!(world.resource::<Inventory>().count(ItemKind::BeaconCore), 1);
        assert!(world.get_entity(core).is_err());
        assert!(
            world
                .resource::<Dialogue>()
                .text()
                .expect("collecting the core must prompt the player to speak to Nemi")
                .contains("Speak to Nemi")
        );

        press_interact(&mut app);
        assert!(app.world().resource::<Dialogue>().text().is_none());
        press_key(&mut app, KeyCode::Enter);
        assert!(app.world().resource::<Dialogue>().is_open());
        assert!(
            app.world()
                .resource::<QuestJournal>()
                .expects(QuestAction::TalkedToNemi)
        );
        press_key(&mut app, KeyCode::Enter);
        let world = app.world();
        let journal = world.resource::<QuestJournal>();
        assert!(journal.has_occurred(QuestAction::TalkedToNemi));
        assert!(journal.is_complete(LAST_LIGHT));
        assert_eq!(world.resource::<Inventory>().count(ItemKind::BeaconCore), 0);
        assert!(
            world
                .resource::<Dialogue>()
                .text()
                .expect("handing over the core must open Nemi's response")
                .contains("You found the core")
        );
    }

    #[test]
    fn a_catalogue_only_speaker_repeats_replies_through_mouse_and_keyboard() {
        let mut app = test_app();
        let catalog: DialogueCatalog = ron::from_str(
            r#"[
            (id: "traveller", name: "Traveller", topics: [
                (id: "route", label: "Ask about the route", reply: Text("Follow the blue lamps.")),
            ]),
        ]"#,
        )
        .expect("the traveller profile must parse");
        let world = app.world_mut();
        world.insert_resource(catalog);
        world.spawn((
            Speaker::new("traveller", Area::Surface, 0.35),
            Transform::from_xyz(0.0, 0.0, -2.0),
        ));
        let window = world.spawn_empty().id();
        let camera = world.spawn_empty().id();

        press_interact(&mut app);
        let world = app.world_mut();
        let button = world
            .query::<(&Text, &ChildOf)>()
            .iter(world)
            .find_map(|(text, parent)| (text.0 == "Ask about the route").then_some(parent.parent()))
            .expect("the authored topic must render as a button");
        world.trigger(crate::smoke::pointer_click(button, window, camera));
        app.update();
        assert_eq!(
            app.world().resource::<Dialogue>().text(),
            Some("Follow the blue lamps.")
        );
        press_interact(&mut app);
        press_key(&mut app, KeyCode::Enter);
        assert_eq!(
            app.world().resource::<Dialogue>().text(),
            Some("Follow the blue lamps.")
        );
        press_interact(&mut app);
        press_key(&mut app, KeyCode::KeyS);
        press_key(&mut app, KeyCode::Enter);
        assert!(!app.world().resource::<Dialogue>().is_open());
        assert!(
            app.world()
                .resource::<QuestJournal>()
                .expects(QuestAction::TalkedToNemi)
        );
    }

    #[test]
    fn an_old_choice_cannot_turn_in_the_core_in_a_new_conversation() {
        let mut app = test_app();
        let world = app.world_mut();
        let speaker = world
            .spawn((nemi::speaker(), Transform::from_xyz(0.0, 0.0, -2.0)))
            .id();
        let mut journal = world.resource_mut::<QuestJournal>();
        journal.record(QuestAction::RecoveredCore);
        assert!(
            world
                .resource_mut::<Inventory>()
                .add(ItemKind::BeaconCore, 1)
        );

        press_interact(&mut app);
        let revision = app.world().resource::<Dialogue>().revision();
        press_key(&mut app, KeyCode::Escape);
        press_interact(&mut app);
        let world = app.world_mut();
        world.resource_mut::<DialogueClick>().0 =
            Some((revision, dialogue::Choice::Topic(LAST_LIGHT.into())));
        app.update();
        assert!(app.world().resource::<Dialogue>().is_open());
        assert!(
            app.world()
                .resource::<QuestJournal>()
                .expects(QuestAction::TalkedToNemi)
        );

        let world = app.world_mut();
        let revision = world.resource::<Dialogue>().revision();
        world.resource_mut::<DialogueClick>().0 =
            Some((revision, dialogue::Choice::Topic(LAST_LIGHT.into())));
        world.despawn(speaker);
        app.update();
        assert!(!app.world().resource::<Dialogue>().is_open());
        assert!(
            app.world()
                .resource::<QuestJournal>()
                .expects(QuestAction::TalkedToNemi)
        );
        assert_eq!(
            app.world()
                .resource::<Inventory>()
                .count(ItemKind::BeaconCore),
            1
        );
    }

    #[test]
    fn each_chest_keeps_its_own_contents_and_does_not_restock_on_reopen() {
        let mut app = test_app();
        let world = app.world_mut();
        let window = world.spawn_empty().id();
        let camera = world.spawn_empty().id();
        let lamp = world
            .spawn((ChestId::Lamp, Transform::from_xyz(0.0, 0.0, -1.0)))
            .id();
        let gear = world
            .spawn((ChestId::Gear, Transform::from_xyz(0.0, 0.0, -2.0)))
            .id();
        let click = |app: &mut App, action| {
            let world = app.world_mut();
            let button = world
                .query::<(Entity, &StorageAction)>()
                .iter(world)
                .find(|(_, current)| **current == action)
                .expect("storage action")
                .0;
            world.trigger(crate::smoke::pointer_click(button, window, camera));
        };
        let take =
            StorageAction::Transfer(StorageSide::Chest, ItemKind::Battery, TransferAmount::Stack);
        let store = StorageAction::Transfer(
            StorageSide::Backpack,
            ItemKind::Battery,
            TransferAmount::One,
        );

        press_interact(&mut app);
        assert_eq!(app.world().resource::<ActiveChest>().0, Some(lamp));
        click(&mut app, take);
        click(&mut app, take);
        assert_eq!(
            app.world().resource::<Inventory>().count(ItemKind::Battery),
            2
        );
        assert_eq!(
            app.world()
                .resource::<ChestStorage>()
                .get(ChestId::Lamp)
                .count(ItemKind::Battery),
            0
        );
        click(&mut app, StorageAction::Close);
        app.update();
        assert_eq!(app.world().resource::<ActiveChest>().0, None);
        press_interact(&mut app);
        click(&mut app, take);
        assert_eq!(
            app.world().resource::<Inventory>().count(ItemKind::Battery),
            2
        );
        click(&mut app, StorageAction::Close);
        app.update();

        app.world_mut()
            .get_mut::<Transform>(lamp)
            .expect("lamp position")
            .translation
            .z = -4.0;
        press_interact(&mut app);
        assert_eq!(app.world().resource::<ActiveChest>().0, Some(gear));
        click(&mut app, store);
        let storage = app.world().resource::<ChestStorage>();
        assert_eq!(storage.get(ChestId::Gear).count(ItemKind::Battery), 1);
        assert_eq!(storage.get(ChestId::Gear).count(ItemKind::FieldHelmet), 1);
        assert_eq!(storage.get(ChestId::Lamp).count(ItemKind::Battery), 0);
    }

    #[test]
    fn only_dead_unobstructed_corpses_in_the_same_area_and_in_range_are_targeted() {
        let mut app = test_app();
        let corpse = app
            .world_mut()
            .spawn((
                Name::new("Ice worm"),
                Transform::from_xyz(0.0, 0.0, -2.0),
                CorpseLoot::new(ItemKind::WormMeat, Area::Surface),
            ))
            .id();

        press_interact(&mut app);
        assert!(app.world().resource::<Nearby>().0.is_none());

        app.world_mut().entity_mut(corpse).insert(Dead);
        app.world_mut()
            .get_mut::<CorpseLoot>(corpse)
            .expect("loot")
            .area = Area::OldRelay;
        press_interact(&mut app);
        assert!(app.world().resource::<Nearby>().0.is_none());

        app.world_mut()
            .get_mut::<CorpseLoot>(corpse)
            .expect("loot")
            .area = Area::Surface;
        app.world_mut()
            .get_mut::<Transform>(corpse)
            .expect("position")
            .translation
            .z = -4.0;
        press_interact(&mut app);
        assert!(app.world().resource::<Nearby>().0.is_none());

        app.world_mut()
            .get_mut::<Transform>(corpse)
            .expect("position")
            .translation
            .z = -2.0;
        let wall = app
            .world_mut()
            .spawn((
                RigidBody::Static,
                Collider::cuboid(3.0, 3.0, 0.2),
                Transform::from_xyz(0.0, 0.0, -1.0),
            ))
            .id();
        for _ in 0..3 {
            app.update();
        }
        press_interact(&mut app);
        assert!(app.world().resource::<Nearby>().0.is_none());

        app.world_mut().despawn(wall);
        for _ in 0..3 {
            app.update();
        }
        assert_eq!(
            app.world().resource::<Nearby>().0,
            Some((corpse, InteractionTarget::Corpse))
        );
    }

    #[test]
    fn loot_panel_transfers_each_corpse_stack_once_and_keeps_the_body() {
        use crate::{smoke::pointer_click, ui::loot_ui::LootAction};

        let mut app = test_app();
        let window = app.world_mut().spawn_empty().id();
        let camera = app.world_mut().spawn_empty().id();
        let click = |app: &mut App, action| {
            let world = app.world_mut();
            let button = world
                .query::<(Entity, &LootAction)>()
                .iter(world)
                .find(|(_, current)| **current == action)
                .expect("loot action")
                .0;
            world.trigger(pointer_click(button, window, camera));
        };
        for item in [ItemKind::WormMeat, ItemKind::RoachMeat, ItemKind::Mycelium] {
            let corpse = app
                .world_mut()
                .spawn((
                    Dead,
                    Name::new(item.name()),
                    Transform::from_xyz(0.0, 0.0, -2.0),
                    CorpseLoot::new(item, Area::Surface),
                ))
                .id();

            press_interact(&mut app);
            assert_eq!(
                *app.world().resource::<State<GameState>>().get(),
                GameState::Looting
            );
            click(&mut app, LootAction::Close);
            app.update();
            assert_eq!(app.world().resource::<Inventory>().count(item), 0);

            press_interact(&mut app);
            // A full stack must leave the loot available for a later attempt.
            assert!(
                app.world_mut()
                    .resource_mut::<Inventory>()
                    .add(item, u32::MAX)
            );
            click(&mut app, LootAction::Take);
            assert!(
                app.world()
                    .get::<CorpseLoot>(corpse)
                    .expect("loot")
                    .stack
                    .is_some()
            );
            assert!(
                app.world_mut()
                    .resource_mut::<Inventory>()
                    .remove(item, u32::MAX)
            );

            click(&mut app, LootAction::Take);
            assert_eq!(app.world().resource::<Inventory>().count(item), 1);
            assert!(
                app.world()
                    .get::<CorpseLoot>(corpse)
                    .expect("corpse remains")
                    .stack
                    .is_none()
            );
            click(&mut app, LootAction::Close);
            app.update();
            press_interact(&mut app);
            assert!(app.world().resource::<Nearby>().0.is_none());
            assert_eq!(app.world().resource::<Inventory>().count(item), 1);
        }
    }

    #[test]
    fn one_press_performs_one_interaction_and_dialogue_dismissal_takes_precedence() {
        let mut app = test_app();
        let item = app
            .world_mut()
            .spawn((
                Pickup {
                    item: crate::inventory::ItemKind::Battery,
                    quantity: 2,
                },
                Transform::from_xyz(0.0, 0.0, -1.0),
            ))
            .id();
        app.world_mut()
            .spawn((nemi::speaker(), Transform::from_xyz(0.0, 0.0, -2.0)));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyE);
        app.update();
        assert_eq!(
            app.world()
                .resource::<Inventory>()
                .count(crate::inventory::ItemKind::Battery),
            2
        );
        assert!(
            app.world()
                .resource::<QuestJournal>()
                .expects(QuestAction::TalkedToNemi)
        );
        assert!(app.world().get_entity(item).is_err());
        // Dismissal must not collect another nearby stack on the same key press.
        app.world_mut()
            .resource_mut::<Dialogue>()
            .show_notice("A conversation");
        app.world_mut().spawn((
            Pickup {
                item: crate::inventory::ItemKind::Battery,
                quantity: 1,
            },
            Transform::from_xyz(0.0, 0.0, -1.0),
        ));
        app.update();
        assert!(app.world().resource::<Dialogue>().text().is_none());
        assert_eq!(
            app.world()
                .resource::<Inventory>()
                .count(crate::inventory::ItemKind::Battery),
            2
        );
    }
    #[test]
    fn range_walls_and_inventory_tab_block_collection() {
        let mut app = test_app();
        let item = app
            .world_mut()
            .spawn((
                Pickup {
                    item: crate::inventory::ItemKind::Scrap,
                    quantity: 1,
                },
                Transform::from_xyz(0.0, 0.0, -4.0),
            ))
            .id();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyE);
        app.update();
        assert!(app.world().resource::<Nearby>().0.is_none());
        app.world_mut()
            .get_mut::<Transform>(item)
            .expect(
                "an out-of-range pickup must remain spawned with its Transform after interaction",
            )
            .translation
            .z = -2.0;
        let wall = app
            .world_mut()
            .spawn((
                RigidBody::Static,
                Collider::cuboid(3.0, 3.0, 0.2),
                Transform::from_xyz(0.0, 0.0, -1.0),
            ))
            .id();
        // Let the physics query tree synchronize before pressing E.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        for _ in 0..3 {
            app.update();
        }
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyE);
        app.update();
        assert!(app.world().resource::<Nearby>().0.is_none());
        assert!(app.world().get_entity(item).is_ok());
        app.world_mut().despawn(wall);
        app.world_mut()
            .resource_mut::<PlayerMenu>()
            .show(crate::ui::menu_ui::MenuTab::Inventory);
        for _ in 0..3 {
            app.update();
        }
        assert!(app.world().get_entity(item).is_ok());
        assert_eq!(
            app.world()
                .resource::<Inventory>()
                .count(crate::inventory::ItemKind::Scrap),
            0
        );
    }
    #[test]
    fn opening_inventory_wins_over_same_frame_pickup() {
        let mut app = test_app();
        app.add_systems(PreUpdate, crate::ui::pause);
        app.world_mut().spawn((
            Pickup {
                item: crate::inventory::ItemKind::Scrap,
                quantity: 1,
            },
            Transform::from_xyz(0.0, 0.0, -1.0),
        ));
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.press(KeyCode::KeyI);
        keys.press(KeyCode::KeyE);
        app.update();
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::Playing
        );
        assert!(
            app.world()
                .resource::<PlayerMenu>()
                .is_on(crate::ui::menu_ui::MenuTab::Inventory)
        );
        assert_eq!(
            app.world()
                .resource::<Inventory>()
                .count(crate::inventory::ItemKind::Scrap),
            0
        );
    }
    #[test]
    fn drop_requires_support_and_cannot_pass_through_walls() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = test_app();
        let player = app.world_mut().spawn_empty().id();
        let transform = Transform::from_xyz(0.0, 0.905, 0.0);
        let check = move |spatial: SpatialQuery| {
            crate::inventory::find_drop_position(
                &spatial,
                player,
                &transform,
                crate::inventory::ItemKind::Battery,
            )
        };
        assert!(
            app.world_mut()
                .run_system_once(check)
                .expect(
                    "test_app must initialize the physics resources required to run the drop query"
                )
                .is_none()
        );
        app.world_mut().spawn((
            RigidBody::Static,
            Collider::cuboid(20.0, 1.0, 20.0),
            Transform::from_xyz(0.0, -0.5, 0.0),
        ));
        for _ in 0..3 {
            app.update();
        }
        let position = app
            .world_mut()
            .run_system_once(check)
            .expect("test_app must initialize the physics resources required to run the drop query")
            .expect("clear floor should allow dropping");
        assert!((position.y - 0.012).abs() < 0.001);
        for (position, size) in [
            (Vec3::new(0.0, 1.0, 0.65), Vec3::new(2.0, 3.0, 0.2)),
            (Vec3::new(0.0, 1.0, -0.65), Vec3::new(2.0, 3.0, 0.2)),
            (Vec3::new(0.65, 1.0, 0.0), Vec3::new(0.2, 3.0, 2.0)),
            (Vec3::new(-0.65, 1.0, 0.0), Vec3::new(0.2, 3.0, 2.0)),
        ] {
            app.world_mut().spawn((
                RigidBody::Static,
                Collider::cuboid(size.x, size.y, size.z),
                Transform::from_translation(position),
            ));
        }
        for _ in 0..3 {
            app.update();
        }
        assert!(
            app.world_mut()
                .run_system_once(check)
                .expect(
                    "test_app must initialize the physics resources required to run the drop query"
                )
                .is_none()
        );
    }
}
