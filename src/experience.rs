//! Combat and quest XP, thresholds, and player level gains.
use bevy::prelude::*;

use crate::{
    GameState,
    combat::{ActorDefeated, CombatSystems, ConfirmedHit, Dead, Health},
    interaction::InteractionSystems,
    level::Level,
    nemi::react_to_player_hit,
    player::{CharacterProgress, Player},
    quest::QuestJournal,
};

const FIRST_LEVEL_XP: u64 = 70;
const XP_INCREASE_PER_LEVEL: u64 = 50;
const XP_NOTICE_SECONDS: f32 = 3.0;

/// The player's XP toward the next level, rather than a lifetime total.
///
/// Quest completions and defeated enemies the player damaged award XP. Reaching
/// a level's threshold advances `CharacterProgress` and carries surplus XP
/// toward subsequent levels. This component is saved with the player's progress.
#[derive(
    Component,
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    bitcode::Encode,
    bitcode::Decode,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
pub(crate) struct Experience(pub(crate) u64);

impl Experience {
    pub(crate) fn required(level: Level) -> Option<u64> {
        if !level.is_valid() || level == Level::MAX {
            return None;
        }

        Some(FIRST_LEVEL_XP + XP_INCREASE_PER_LEVEL * u64::from(level.get() - 1))
    }

    pub(crate) fn is_valid(self, level: Level) -> bool {
        self.0 < Self::required(level).unwrap_or(1)
    }

    fn gain(&mut self, amount: u64, progress: &mut CharacterProgress) -> u32 {
        let mut level = progress.level;
        let mut xp = self.0.saturating_add(amount);
        while let Some(required) = Self::required(level) {
            if xp < required {
                break;
            }
            xp -= required;
            if !level.advance() {
                break;
            }
        }
        self.0 = xp;
        progress.advance_to(level)
    }
}

#[derive(Component)]
pub(crate) struct ExperienceReward {
    amount: u16,
    pub(crate) player_contributed: bool,
}

impl ExperienceReward {
    pub(crate) const fn new(amount: u16) -> Self {
        assert!(amount > 0, "killable NPCs must award positive XP");
        Self {
            amount,
            player_contributed: false,
        }
    }
}

#[derive(Resource, Default)]
pub(crate) struct ExperienceNotice {
    pub(crate) text: Option<String>,
    remaining_seconds: f32,
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ExperienceUpdates;

pub(crate) struct ExperiencePlugin;

impl Plugin for ExperiencePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ExperienceNotice>().add_systems(
            Update,
            (age_notice, award_experience)
                .chain()
                .in_set(ExperienceUpdates)
                .after(CombatSystems::Resolve)
                .after(InteractionSystems)
                .after(react_to_player_hit)
                .run_if(in_state(GameState::Playing)),
        );
    }
}

fn age_notice(time: Res<Time>, mut notice: ResMut<ExperienceNotice>) {
    if notice.text.is_some() {
        notice.remaining_seconds -= time.delta_secs();
        if notice.remaining_seconds <= 0.0 {
            *notice = ExperienceNotice::default();
        }
    }
}

fn award_experience(
    mut hits: MessageReader<ConfirmedHit>,
    mut defeats: MessageReader<ActorDefeated>,
    mut players: Query<
        (Entity, &mut Experience, &mut CharacterProgress),
        (With<Player>, Without<Dead>),
    >,
    mut rewards: Query<&mut ExperienceReward, (With<Health>, Without<Player>)>,
    mut journal: ResMut<QuestJournal>,
    mut notice: ResMut<ExperienceNotice>,
) {
    // Draining the transient reward does not change journal progress or world effects.
    let quest_xp = journal.bypass_change_detection().take_completion_xp();
    let Ok((player, mut xp, mut progress)) = players.single_mut() else {
        hits.clear();
        defeats.clear();
        return;
    };
    // Record every contribution before looking up rewards, including hits from this update.
    for hit in hits.read().filter(|hit| hit.source == player) {
        if let Ok(mut reward) = rewards.get_mut(hit.target)
            && !reward.player_contributed
        {
            reward.player_contributed = true;
        }
    }
    let combat_xp: u64 = defeats
        .read()
        .filter_map(|defeat| {
            let mut reward = rewards.get_mut(defeat.0).ok()?;
            std::mem::take(&mut reward.player_contributed).then_some(u64::from(reward.amount))
        })
        .sum();
    let earned = combat_xp + quest_xp;
    if earned == 0 || Experience::required(progress.level).is_none() {
        return;
    }
    let previous_level = progress.level;
    let points = xp.gain(earned, &mut progress);
    let text = format!("+{earned} XP");
    let text = if progress.level > previous_level {
        format!(
            "{text} | Level {} (+{points} attribute points)",
            progress.level.get()
        )
    } else {
        text
    };
    *notice = ExperienceNotice {
        text: Some(text),
        remaining_seconds: XP_NOTICE_SECONDS,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        combat::{CombatPlugin, Hit, TrainingTarget},
        dialogue::Dialogue,
        quest::{QuestAction, test_journal},
    };
    use bevy::state::app::StatesPlugin;

    #[test]
    fn thresholds_increase_and_surplus_can_grant_multiple_levels() {
        let mut xp = Experience::default();
        let mut progress = CharacterProgress::default();
        assert_eq!(xp.gain(69, &mut progress), 0);
        assert_eq!(xp, Experience(69));
        assert_eq!(xp.gain(1, &mut progress), 3);
        assert_eq!((progress.level.get(), xp), (2, Experience(0)));
        assert_eq!(xp.gain(300, &mut progress), 6);
        assert_eq!(
            (progress.level.get(), progress.unspent, xp),
            (4, 9, Experience(10))
        );
        assert_eq!(
            Experience::required(Level::new(4).expect("valid test level")),
            Some(220)
        );
    }

    fn hit(world: &mut World, source: Entity, target: Entity, damage: u16) {
        world.write_message(Hit {
            source,
            target,
            damage,
            contact: None,
        });
    }

    #[test]
    fn awards_only_player_contributed_deaths_and_groups_same_update_gains() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CombatPlugin, ExperiencePlugin))
            .insert_state(GameState::Playing)
            .init_resource::<QuestJournal>()
            .init_resource::<Dialogue>();
        let world = app.world_mut();
        let player = world.spawn(Player::default()).id();
        let other = world.spawn_empty().id();
        let worm_a = world
            .spawn((Health::new(30), ExperienceReward::new(35)))
            .id();
        let worm_b = world
            .spawn((Health::new(30), ExperienceReward::new(35)))
            .id();
        let roach = world
            .spawn((Health::new(20), ExperienceReward::new(25)))
            .id();
        let corpse = world
            .spawn((Health::new(0), ExperienceReward::new(25), Dead))
            .id();
        let dummy = world.spawn(TrainingTarget).id();

        hit(world, player, worm_a, 1);
        app.update();
        let world = app.world_mut();
        assert_eq!(world.get::<Experience>(player), Some(&Experience(0)));
        assert!(world.resource::<ExperienceNotice>().text.is_none());

        hit(world, other, worm_a, 40);
        hit(world, player, worm_a, 40);
        hit(world, player, worm_b, 1);
        hit(world, other, worm_b, 40);
        hit(world, player, roach, 0);
        hit(world, other, roach, 40);
        hit(world, player, corpse, 40);
        hit(world, player, dummy, 40);
        app.update();
        let world = app.world_mut();
        assert_eq!(world.get::<Experience>(player), Some(&Experience(0)));
        assert_eq!(
            world.get::<CharacterProgress>(player),
            Some(&CharacterProgress {
                level: Level::new(2).expect("valid test level"),
                unspent: 3
            }),
        );
        assert_eq!(
            world.resource::<ExperienceNotice>().text.as_deref(),
            Some("+70 XP | Level 2 (+3 attribute points)"),
        );
        hit(world, player, worm_b, 40);
        app.update();
        assert_eq!(app.world().get::<Experience>(player), Some(&Experience(0)));
    }

    #[test]
    fn a_quest_and_kill_in_one_update_share_level_progress_and_one_notice() {
        let mut journal = test_journal();
        journal.record(QuestAction::TalkedToNemi);
        journal.record(QuestAction::RecoveredCore);
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CombatPlugin, ExperiencePlugin))
            .insert_state(GameState::Playing)
            .insert_resource(journal)
            .init_resource::<Dialogue>()
            .add_systems(
                Update,
                (|mut journal: ResMut<QuestJournal>| journal.record(QuestAction::ReturnedCore))
                    .in_set(InteractionSystems),
            );
        let world = app.world_mut();
        let player = world.spawn(Player::default()).id();
        let worm = world
            .spawn((Health::new(30), ExperienceReward::new(35)))
            .id();
        hit(world, player, worm, 30);
        app.update();
        let world = app.world();
        assert_eq!(world.get::<Experience>(player), Some(&Experience(35)));
        assert_eq!(
            world.get::<CharacterProgress>(player),
            Some(&CharacterProgress {
                level: Level::new(2).expect("valid test level"),
                unspent: 3
            }),
        );
        assert_eq!(
            world.resource::<ExperienceNotice>().text.as_deref(),
            Some("+105 XP | Level 2 (+3 attribute points)"),
        );
        app.update();
        assert_eq!(app.world().get::<Experience>(player), Some(&Experience(35)));
    }
}
