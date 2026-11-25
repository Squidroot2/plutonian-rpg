//! Actor attribute ranks and the gameplay capabilities derived from them.
use crate::{
    GameState,
    combat::{CombatSystems, Dead, Defense, Health, HealthRecovery},
    experience::{Experience, ExperienceUpdates},
    player::{CharacterProgress, Player, Stamina},
    ui::menu_ui::{MenuTab, PlayerMenu},
};
use bevy::prelude::*;
use strum::VariantArray;

#[cfg(test)]
use crate::player::Level;

pub(crate) const DEFAULT_RANK: u8 = 5;
pub(crate) const MAX_RANK: u8 = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq, VariantArray)]
pub(crate) enum AttributeId {
    Vitality,
    Endurance,
    Agility,
    Strength,
}

impl AttributeId {
    /// Return the stable lowercase key used to save and restore this rank.
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Vitality => "vitality",
            Self::Endurance => "endurance",
            Self::Agility => "agility",
            Self::Strength => "strength",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Vitality => "Vitality",
            Self::Endurance => "Endurance",
            Self::Agility => "Agility",
            Self::Strength => "Strength",
        }
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(crate) struct AttributeRanks([u8; AttributeId::VARIANTS.len()]);

impl Default for AttributeRanks {
    fn default() -> Self {
        Self([DEFAULT_RANK; AttributeId::VARIANTS.len()])
    }
}

impl AttributeRanks {
    pub(crate) fn from_array(ranks: [u8; AttributeId::VARIANTS.len()]) -> Self {
        Self(ranks)
    }

    pub(crate) fn rank(&self, id: AttributeId) -> u8 {
        self.0[id as usize]
    }

    pub(crate) fn with_rank(mut self, id: AttributeId, rank: u8) -> Self {
        assert!(
            (1..=MAX_RANK).contains(&rank),
            "starting rank must be valid"
        );
        self.0[id as usize] = rank;
        self
    }

    pub(crate) fn actor_components(self) -> impl Bundle {
        let effects = self.effects();
        (
            self,
            Health::new(effects.health),
            Defense::default(),
            HealthRecovery::character(effects.health_per_second),
            Stamina::new(effects.stamina, effects.stamina_per_second),
            effects.costs,
            effects.carry,
            effects.melee,
        )
    }

    pub(crate) fn effects(&self) -> AttributeEffects {
        AttributeEffects::from(self)
    }
}

#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub(crate) struct MovementCosts {
    pub(crate) jump: f32,
    pub(crate) dodge: f32,
}

impl Default for MovementCosts {
    fn default() -> Self {
        Self::for_rank(DEFAULT_RANK)
    }
}

impl MovementCosts {
    fn for_rank(rank: u8) -> Self {
        let scale = ((105.0 / (f64::from(rank) + 5.0)).ln() / 10.5_f64.ln()) as f32;
        Self {
            jump: 20.0 * scale,
            dodge: 25.0 * scale,
        }
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct CarryLimits {
    pub(crate) capacity_kg: u64,
    pub(crate) movement_limit_kg: u64,
}

impl Default for CarryLimits {
    fn default() -> Self {
        Self::for_rank(DEFAULT_RANK)
    }
}

impl CarryLimits {
    pub(crate) fn ground_speed(self, weight_kg: u64, desired: f32) -> f32 {
        if weight_kg > self.movement_limit_kg {
            0.0
        } else if weight_kg > self.capacity_kg {
            desired.min(crate::player::Pace::Walk.speed())
        } else {
            desired
        }
    }

    fn for_rank(rank: u8) -> Self {
        let capacity_kg = 55 + u64::from(rank);
        Self {
            capacity_kg,
            movement_limit_kg: capacity_kg * 6 / 5,
        }
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MeleeDamageMultiplier(u16);

impl Default for MeleeDamageMultiplier {
    fn default() -> Self {
        Self::for_rank(DEFAULT_RANK)
    }
}

impl MeleeDamageMultiplier {
    fn for_rank(rank: u8) -> Self {
        Self(95 + u16::from(rank))
    }

    pub(crate) fn scale(self, damage: u16) -> u16 {
        let scaled = (u32::from(damage) * u32::from(self.0) + 50) / 100;
        scaled.min(u32::from(u16::MAX)) as u16
    }
}

pub(crate) struct AttributeEffects {
    pub(crate) health: u16,
    pub(crate) health_per_second: f64,
    pub(crate) stamina: f32,
    pub(crate) stamina_per_second: f32,
    costs: MovementCosts,
    carry: CarryLimits,
    melee: MeleeDamageMultiplier,
}

impl From<&AttributeRanks> for AttributeEffects {
    fn from(ranks: &AttributeRanks) -> Self {
        let maximum = |id| 80 + 4 * u16::from(ranks.rank(id));
        let health = maximum(AttributeId::Vitality);
        let stamina = f32::from(maximum(AttributeId::Endurance));
        Self {
            health,
            health_per_second: f64::from(health) / 10.0,
            stamina,
            stamina_per_second: stamina / 4.0,
            costs: MovementCosts::for_rank(ranks.rank(AttributeId::Agility)),
            carry: CarryLimits::for_rank(ranks.rank(AttributeId::Endurance)),
            melee: MeleeDamageMultiplier::for_rank(ranks.rank(AttributeId::Strength)),
        }
    }
}

impl AttributeEffects {
    fn describe(&self, id: AttributeId) -> String {
        match id {
            AttributeId::Vitality => {
                format!("{} HP, {:.1} HP/s", self.health, self.health_per_second)
            }
            AttributeId::Endurance => format!(
                "{:.0} stamina, {:.0} stamina/s, {} kg capacity",
                self.stamina, self.stamina_per_second, self.carry.capacity_kg
            ),
            AttributeId::Agility => format!(
                "{:.2} jump, {:.2} dodge stamina",
                self.costs.jump, self.costs.dodge
            ),
            AttributeId::Strength => format!("{}% melee damage", self.melee.0),
        }
    }
}

#[derive(Message)]
struct SpendAttribute {
    player: Entity,
    id: AttributeId,
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct AttributeUpdates;

pub(crate) struct AttributesPlugin;

impl Plugin for AttributesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SpendAttribute>()
            .add_systems(
                Update,
                (apply_spends, sync_capabilities)
                    .chain()
                    .in_set(AttributeUpdates)
                    .before(CombatSystems::Detect)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                refresh_character
                    .after(AttributeUpdates)
                    .after(ExperienceUpdates),
            );
    }
}

fn apply_spends(
    mut players: Query<
        (Entity, &mut AttributeRanks, &mut CharacterProgress),
        (With<Player>, Without<Dead>),
    >,
    mut spends: MessageReader<SpendAttribute>,
    menu: Res<PlayerMenu>,
) {
    let Ok((player, mut ranks, mut progress)) = players.single_mut() else {
        spends.clear();
        return;
    };
    if !menu.accepts(MenuTab::Character) {
        spends.clear();
        return;
    }
    for spend in spends.read().filter(|spend| spend.player == player) {
        let rank = ranks.rank(spend.id);
        if progress.unspent > 0 && rank < MAX_RANK {
            ranks.0[spend.id as usize] = rank + 1;
            progress.unspent -= 1;
        }
    }
}

fn sync_capabilities(
    mut actors: Query<
        (
            &AttributeRanks,
            &mut Health,
            &mut HealthRecovery,
            &mut Stamina,
            &mut MovementCosts,
            &mut CarryLimits,
            &mut MeleeDamageMultiplier,
        ),
        Changed<AttributeRanks>,
    >,
) {
    for (ranks, mut health, mut recovery, mut stamina, mut costs, mut carry, mut melee) in
        &mut actors
    {
        let effect = ranks.effects();
        if health.maximum() != effect.health {
            health.set_maximum(effect.health);
        }
        if recovery.rate() != effect.health_per_second {
            recovery.set_rate(effect.health_per_second);
        }
        if stamina.tuning() != (effect.stamina, effect.stamina_per_second) {
            stamina.set_tuning(effect.stamina, effect.stamina_per_second);
        }
        costs.set_if_neq(effect.costs);
        carry.set_if_neq(effect.carry);
        melee.set_if_neq(effect.melee);
    }
}

#[derive(Component)]
enum CharacterText {
    Name,
    Progress,
    Attribute(AttributeId),
}

#[derive(Component)]
struct SpendButton(AttributeId);

#[derive(Component)]
struct AttributeDetails(AttributeId);

pub(crate) fn spawn_character(page: &mut ChildSpawnerCommands) {
    use crate::{ui::menu_ui, ui::text_style};

    page.spawn(menu_ui::scroll_pane(MenuTab::Character, percent(100)))
        .with_children(|pane| {
            pane.spawn((CharacterText::Name, Text::default(), text_style(18.0)));
            pane.spawn((CharacterText::Progress, Text::default(), text_style(18.0)));
            for id in AttributeId::VARIANTS {
                pane.spawn((Node {
                    align_items: AlignItems::Center,
                    column_gap: px(12),
                    ..default()
                },))
                    .with_children(|row| {
                        row.spawn((
                            CharacterText::Attribute(*id),
                            Text::default(),
                            text_style(15.0),
                            Node {
                                flex_grow: 1.0,
                                min_width: px(0),
                                ..default()
                            },
                        ))
                        .observe(show_attribute_details)
                        .observe(hide_attribute_details);
                        row.spawn((
                            AttributeDetails(*id),
                            Text::new(""),
                            text_style(13.0),
                            Visibility::Hidden,
                            Pickable::IGNORE,
                        ));
                        row.spawn((SpendButton(*id), menu_ui::button()))
                            .with_child((Text::new("+"), text_style(20.0), Pickable::IGNORE))
                            .observe(spend_click)
                            .observe(show_rank_up_details)
                            .observe(hide_rank_up_details);
                    });
            }
        });
}

fn show_attribute_details(
    over: On<PointerOver>,
    labels: Query<&CharacterText>,
    ranks: Single<&AttributeRanks, With<Player>>,
    mut details: Query<(&AttributeDetails, &mut Text, &mut Visibility)>,
) {
    let Ok(CharacterText::Attribute(id)) = labels.get(over.entity) else {
        return;
    };
    for (detail, mut text, mut visibility) in &mut details {
        if detail.0 == *id {
            text.0 = ranks.effects().describe(*id);
            *visibility = Visibility::Visible;
        }
    }
}

fn hide_attribute_details(
    out: On<PointerOut>,
    labels: Query<&CharacterText>,
    mut details: Query<(&AttributeDetails, &mut Visibility)>,
) {
    let Ok(CharacterText::Attribute(id)) = labels.get(out.entity) else {
        return;
    };
    for (detail, mut visibility) in &mut details {
        if detail.0 == *id {
            *visibility = Visibility::Hidden;
        }
    }
}

fn show_rank_up_details(
    over: On<PointerOver>,
    buttons: Query<&SpendButton>,
    ranks: Single<&AttributeRanks, With<Player>>,
    progress: Single<&CharacterProgress, With<Player>>,
    mut details: Query<(&AttributeDetails, &mut Text, &mut Visibility)>,
) {
    let Ok(button) = buttons.get(over.entity) else {
        return;
    };
    let id = button.0;
    for (detail, mut text, mut visibility) in &mut details {
        if detail.0 == id {
            let rank = ranks.rank(id);
            let enabled = progress.unspent > 0 && rank < MAX_RANK;
            let value = if enabled {
                let mut next = (*ranks).clone();
                next.0[id as usize] += 1;
                format!("Next rank: {}", next.effects().describe(id))
            } else if rank >= MAX_RANK {
                "Maximum rank".to_string()
            } else {
                "No unspent attribute points".to_string()
            };
            text.0 = value;
            *visibility = Visibility::Visible;
        }
    }
}

fn hide_rank_up_details(
    out: On<PointerOut>,
    buttons: Query<&SpendButton>,
    mut details: Query<(&AttributeDetails, &mut Visibility)>,
) {
    let Ok(button) = buttons.get(out.entity) else {
        return;
    };
    for (detail, mut visibility) in &mut details {
        if detail.0 == button.0 {
            *visibility = Visibility::Hidden;
        }
    }
}

fn spend_click(
    click: On<PointerClick>,
    buttons: Query<&SpendButton>,
    player: Single<Entity, (With<Player>, Without<Dead>)>,
    menu: Res<PlayerMenu>,
    mut spends: MessageWriter<SpendAttribute>,
) {
    if click.button == PointerButton::Primary
        && menu.accepts(MenuTab::Character)
        && let Ok(button) = buttons.get(click.entity)
    {
        spends.write(SpendAttribute {
            player: *player,
            id: button.0,
        });
    }
}

fn refresh_character(
    menu: Res<PlayerMenu>,
    players: Query<
        (
            Ref<AttributeRanks>,
            Ref<CharacterProgress>,
            Ref<Experience>,
            &crate::player::PlayerName,
        ),
        (With<Player>, Without<Dead>),
    >,
    mut texts: Query<(&CharacterText, &mut Text)>,
    mut buttons: Query<(&SpendButton, &mut BackgroundColor)>,
) {
    use crate::ui::menu_ui::{BUTTON, DISABLED};
    let Ok((ranks, progress, xp, name)) = players.single() else {
        return;
    };
    if !menu.is_on(MenuTab::Character)
        || !(menu.is_changed() || ranks.is_changed() || progress.is_changed() || xp.is_changed())
    {
        return;
    }
    for (label, mut text) in &mut texts {
        let value = match *label {
            CharacterText::Name => name.0.clone(),
            CharacterText::Progress => {
                let experience = Experience::required(progress.level).map_or_else(
                    || "Max level".into(),
                    |required| format!("{} / {required} XP", xp.0),
                );
                format!(
                    "Level {} | {experience} | {} unspent points",
                    progress.level.get(),
                    progress.unspent
                )
            }
            CharacterText::Attribute(id) => format!("{} {}", id.name(), ranks.rank(id)),
        };
        text.set_if_neq(Text::new(value));
    }
    for (button, mut color) in &mut buttons {
        let enabled = progress.unspent > 0 && ranks.rank(button.0) < MAX_RANK;
        color.set_if_neq(BackgroundColor(if enabled { BUTTON } else { DISABLED }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::pointer_click;
    use bevy::state::app::StatesPlugin;

    #[test]
    fn derived_values_match_baselines_and_caps() {
        let baseline = AttributeRanks::default().effects();
        assert_eq!((baseline.health, baseline.health_per_second), (100, 10.0));
        assert_eq!(
            (baseline.stamina, baseline.stamina_per_second),
            (100.0, 25.0)
        );
        assert_eq!((baseline.costs.jump, baseline.costs.dodge), (20.0, 25.0));
        assert_eq!(
            (baseline.carry.capacity_kg, baseline.carry.movement_limit_kg),
            (60, 72)
        );
        assert_eq!(baseline.melee.scale(7), 7);

        let minimum = AttributeRanks([1; AttributeId::VARIANTS.len()]).effects();
        assert_eq!((minimum.health, minimum.stamina), (84, 84.0));
        assert_eq!(
            (minimum.carry.capacity_kg, minimum.carry.movement_limit_kg),
            (56, 67)
        );

        let maximum = AttributeRanks([MAX_RANK; AttributeId::VARIANTS.len()]).effects();
        assert_eq!((maximum.health, maximum.health_per_second), (480, 48.0));
        assert_eq!(
            (maximum.stamina, maximum.stamina_per_second),
            (480.0, 120.0)
        );
        assert_eq!(
            (maximum.carry.capacity_kg, maximum.carry.movement_limit_kg),
            (155, 186)
        );
        assert_eq!((maximum.melee.scale(10), maximum.melee.scale(7)), (20, 14));

        for rank in 1..MAX_RANK {
            let current = MovementCosts::for_rank(rank);
            let next = MovementCosts::for_rank(rank + 1);
            assert!(current.jump > next.jump && current.dodge > next.dodge);
        }
        assert_eq!((maximum.costs.jump, maximum.costs.dodge), (0.0, 0.0));
    }

    #[test]
    fn level_gains_and_character_clicks_apply_once_without_refilling_health() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, AttributesPlugin))
            .insert_state(GameState::Playing)
            .init_resource::<PlayerMenu>();
        let world = app.world_mut();
        world.resource_mut::<PlayerMenu>().show(MenuTab::Character);
        let player = world.spawn(Player::default()).id();
        world
            .get_mut::<Health>(player)
            .expect("player has health")
            .set_current(50);
        let button = world
            .spawn(SpendButton(AttributeId::Vitality))
            .observe(spend_click)
            .id();
        let window = world.spawn_empty().id();
        let camera = world.spawn_empty().id();
        let mut progress = world
            .get_mut::<CharacterProgress>(player)
            .expect("player progress");
        assert_eq!(
            progress.advance_to(Level::new(3).expect("valid test level")),
            6
        );
        assert_eq!(
            progress.advance_to(Level::new(3).expect("valid test level")),
            0
        );
        assert_eq!(
            progress.advance_to(Level::new(2).expect("valid test level")),
            0
        );
        for _ in 0..7 {
            world.trigger(pointer_click(button, window, camera));
        }
        app.update();
        let world = app.world();
        assert_eq!(
            world.get::<CharacterProgress>(player),
            Some(&CharacterProgress {
                level: Level::new(3).expect("valid test level"),
                unspent: 0
            })
        );
        assert_eq!(
            world
                .get::<AttributeRanks>(player)
                .expect("player ranks")
                .rank(AttributeId::Vitality),
            11
        );
        let health = world.get::<Health>(player).expect("player has health");
        assert_eq!((health.current(), health.maximum()), (50, 124));
        assert_eq!(
            world
                .get::<HealthRecovery>(player)
                .expect("player recovery")
                .rate(),
            12.4
        );
    }

    #[test]
    fn leaving_the_character_tab_discards_pending_spends() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, AttributesPlugin))
            .insert_state(GameState::Playing)
            .init_resource::<PlayerMenu>();
        let world = app.world_mut();
        world.resource_mut::<PlayerMenu>().show(MenuTab::Inventory);
        let player = world
            .spawn((
                Player::default(),
                CharacterProgress {
                    level: Level::new(2).expect("valid test level"),
                    unspent: 3,
                },
            ))
            .id();
        world.write_message(SpendAttribute {
            player,
            id: AttributeId::Endurance,
        });
        app.update();
        app.world_mut()
            .resource_mut::<PlayerMenu>()
            .show(MenuTab::Character);
        app.update();
        assert_eq!(
            app.world()
                .get::<AttributeRanks>(player)
                .expect("player ranks")
                .rank(AttributeId::Endurance),
            DEFAULT_RANK,
        );
        assert_eq!(
            app.world()
                .get::<CharacterProgress>(player)
                .expect("progress")
                .unspent,
            3
        );
    }

    #[test]
    fn a_capped_attribute_keeps_the_point_available_for_another_rank() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, AttributesPlugin))
            .insert_state(GameState::Playing)
            .init_resource::<PlayerMenu>();
        let world = app.world_mut();
        world.resource_mut::<PlayerMenu>().show(MenuTab::Character);
        let player = world
            .spawn((
                Player::default(),
                AttributeRanks::default().with_rank(AttributeId::Agility, MAX_RANK),
                CharacterProgress {
                    level: Level::new(2).expect("valid test level"),
                    unspent: 1,
                },
            ))
            .id();
        world.write_message(SpendAttribute {
            player,
            id: AttributeId::Agility,
        });
        app.update();
        assert_eq!(
            app.world()
                .get::<CharacterProgress>(player)
                .expect("progress")
                .unspent,
            1
        );
        assert_eq!(
            app.world()
                .get::<MovementCosts>(player)
                .expect("movement costs")
                .dodge,
            0.0
        );
    }
}
