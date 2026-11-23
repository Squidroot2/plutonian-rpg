//! The two outpost guards share an incident response and defend Nivalis.
use crate::{
    GameState,
    combat::{CombatSystems, ConfirmedHit, Defense},
    dialogue::Speaker,
    dungeon::{Area, HATCH},
    enemy::{self, Bite, EnemyBody, EnemyModel, Engagement, GroundMelee, GroundMovement},
    equipment::{EquipmentSlot, FixedOutfit, WeaponKind},
    inventory::ItemKind,
    loading::EnemyVisuals,
    nemi::Nemi,
    player::{PLAYER_COLLIDER_LENGTH, PLAYER_COLLIDER_RADIUS, PLAYER_GROUND_OFFSET, Player},
    save::EncounterId,
    terrain::ground_heading,
    world::{SpawnWorld, WorldLayout},
};
use avian3d::prelude::Collider;
use bevy::prelude::*;

const OUTPOST_CENTER_XZ: Vec2 = Vec2::ZERO;
const OUTPOST_RADIUS_M: f32 = 12.0;
const PURSUIT_SPEED_MPS: f32 = 5.5;
const ARMOR_DEFENSE: u16 = 8;
pub(crate) const HEALTH: u16 = 140;
const THRUST_DAMAGE: u16 = 58;
const THRUST_START_RANGE_M: f32 = 2.35;
const THRUST_RECOVERY_SECONDS: f32 = 0.6;
const EXPERIENCE: u16 = 100;
const OUTFIT: &[(EquipmentSlot, ItemKind)] = &[
    (EquipmentSlot::Body, ItemKind::ReinforcedArmor),
    (EquipmentSlot::Helmet, ItemKind::ReinforcedHelmet),
    (EquipmentSlot::MainHand, ItemKind::Trident),
];

pub(crate) const MODEL: EnemyModel = EnemyModel {
    path: "models/plutonian.glb",
    clips: ["SpearIdle", "SpearThrust", "NemiDeath"],
    locomotion: Some("SpearRun"),
    visual_offset: Vec3::new(0.0, -PLAYER_GROUND_OFFSET, 0.0),
};

const CONFIG: GroundMelee = GroundMelee {
    body: EnemyBody {
        name: "Plutonian guard",
        health: HEALTH,
        experience: EXPERIENCE,
        loot: ItemKind::Scrap,
    },
    movement: GroundMovement::new(PURSUIT_SPEED_MPS, PLAYER_GROUND_OFFSET),
    engagement: Engagement::DefendOutpost,
    bite: Bite {
        range: THRUST_START_RANGE_M,
        windup: WeaponKind::Spear.timing().0,
        recovery: THRUST_RECOVERY_SECONDS,
        damage: THRUST_DAMAGE,
    },
};

#[derive(
    Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, bitcode::Encode, bitcode::Decode,
)]
pub(crate) enum GuardPost {
    West,
    East,
}

impl GuardPost {
    pub(crate) const ALL: [Self; 2] = [Self::West, Self::East];

    pub(crate) const fn home(self) -> Vec3 {
        match self {
            // This post overlooks the Old Relay hatch from the west.
            Self::West => Vec3::new(-5.5, 0.0, 2.5),
            // The other post watches the ice trail out of Nivalis.
            Self::East => Vec3::new(3.5, 0.0, -8.75),
        }
    }

    const fn dialogue_profile(self) -> &'static str {
        match self {
            Self::West => "sella",
            Self::East => "orren",
        }
    }

    const fn interaction_height(self) -> f32 {
        0.35
    }

    pub(crate) fn facing(self) -> Quat {
        let heading = match self {
            Self::West => HATCH - self.home(),
            Self::East => Vec3::NEG_Z,
        };
        ground_heading(heading)
    }
}

#[derive(Resource, Default)]
pub(crate) struct OutpostSecurity {
    pub(crate) hostile: bool,
}

impl OutpostSecurity {
    pub(crate) fn defends(&self, area: Area, target: Vec3) -> bool {
        self.hostile
            && area == Area::Surface
            && target.xz().distance_squared(OUTPOST_CENTER_XZ)
                <= OUTPOST_RADIUS_M * OUTPOST_RADIUS_M
    }
}

pub(crate) struct GuardPlugin;

impl Plugin for GuardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OutpostSecurity>()
            .add_observer(spawn_guards)
            .add_systems(
                Update,
                react_to_player_hit
                    .after(CombatSystems::Resolve)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

fn spawn_guards(
    _: On<SpawnWorld>,
    mut commands: Commands,
    layout: Res<WorldLayout>,
    visuals: Res<EnemyVisuals>,
) {
    if *layout != WorldLayout::Outpost {
        return;
    }

    for post in GuardPost::ALL {
        let guard = enemy::spawn_ground_melee(
            &mut commands,
            post,
            visuals.get(&MODEL),
            Transform::from_translation(post.home()).with_rotation(post.facing()),
            Area::Surface,
            CONFIG,
            Collider::capsule(PLAYER_COLLIDER_RADIUS, PLAYER_COLLIDER_LENGTH),
        );
        commands.entity(guard).insert((
            EncounterId::Guard(post),
            Defense(ARMOR_DEFENSE),
            FixedOutfit(OUTFIT),
            Speaker::new(
                post.dialogue_profile(),
                Area::Surface,
                post.interaction_height(),
            ),
        ));
    }
}

pub(crate) fn react_to_player_hit(
    mut hits: MessageReader<ConfirmedHit>,
    players: Query<(), With<Player>>,
    protected: Query<(), Or<(With<GuardPost>, With<Nemi>)>>,
    mut security: ResMut<OutpostSecurity>,
) {
    for hit in hits.read() {
        if !security.hostile && players.contains(hit.source) && protected.contains(hit.target) {
            security.hostile = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        combat::{CombatPlugin, Dead, Health, Hit},
        dialogue::Dialogue,
    };
    use bevy::state::app::StatesPlugin;

    #[test]
    fn only_confirmed_player_hits_on_the_keeper_or_either_guard_raise_the_alarm() {
        for target_index in 0..3 {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, StatesPlugin, CombatPlugin, GuardPlugin))
                .insert_state(GameState::Playing)
                .init_resource::<Dialogue>();
            let world = app.world_mut();
            let player = world.spawn(Player::default()).id();
            let stranger = world.spawn_empty().id();
            let other_target = world.spawn(Health::new(10)).id();
            let protected = [
                world.spawn(Nemi::default()).id(),
                world.spawn((GuardPost::West, Health::new(HEALTH))).id(),
                world.spawn((GuardPost::East, Health::new(HEALTH))).id(),
            ];
            let target = protected[target_index];
            for (source, target, damage) in [
                (player, target, 0),
                (stranger, target, 1),
                (player, other_target, 1),
            ] {
                world.write_message(Hit {
                    source,
                    target,
                    damage,
                    contact: None,
                });
            }
            app.update();
            assert!(!app.world().resource::<OutpostSecurity>().hostile);

            app.world_mut().write_message(Hit {
                source: player,
                target,
                damage: HEALTH,
                contact: None,
            });
            app.update();
            let world = app.world();
            assert!(world.resource::<OutpostSecurity>().hostile);
            assert!(world.get::<Dead>(target).is_some());
            assert!(
                protected
                    .iter()
                    .any(|&entity| world.get::<GuardPost>(entity).is_some()
                        && world.get::<Dead>(entity).is_none())
            );
        }
    }

    #[test]
    fn the_alarm_survives_leaving_the_outpost_and_applies_again_on_return() {
        let mut security = OutpostSecurity::default();
        let center = Vec3::new(OUTPOST_CENTER_XZ.x, 0.0, OUTPOST_CENTER_XZ.y);
        assert!(!security.defends(Area::Surface, center));
        security.hostile = true;
        for post in GuardPost::ALL {
            assert!(security.defends(Area::Surface, post.home()));
        }
        assert!(security.defends(Area::Surface, WorldLayout::Outpost.keeper()));
        assert!(security.defends(Area::Surface, center + Vec3::X * OUTPOST_RADIUS_M));
        assert!(!security.defends(Area::Surface, center + Vec3::X * (OUTPOST_RADIUS_M + 0.1)));
        assert!(!security.defends(Area::OldRelay, center));
        assert!(security.defends(Area::Surface, center));
    }
}
