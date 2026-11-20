use crate::{
    GameState,
    player::Player,
    world::{CORE, KEEPER},
};
use avian3d::prelude::*;
use bevy::prelude::*;

pub struct QuestPlugin;
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Keeper,
    Core,
}
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quest {
    #[default]
    MeetKeeper,
    FindCore,
    ReturnCore,
    Complete,
}
impl Quest {
    pub fn interact(&mut self, target: Target) -> &'static str {
        match (*self, target) {
            (Self::MeetKeeper, Target::Keeper) => {
                *self = Self::FindCore;
                "NEMI / BEACON KEEPER\n\nYou made it home before the long dark. Our beacon has gone silent.\nThe families here depend on its signal to cross the ice.\n\nA charged core waits in the blue cavern, beyond the waystones.\nFollow their lights north. Bring it back, and we can guide our people home."
            }
            (Self::FindCore, Target::Keeper) => {
                "NEMI\n\nFollow the blue waystones beyond the outpost. The core is inside\nthe cavern at the end of the trail. I'll keep watch here."
            }
            (Self::FindCore, Target::Core) => {
                *self = Self::ReturnCore;
                "POWER CORE RECOVERED\n\nIts light pulses against your hands. There is still warmth inside.\n\nReturn to Nemi at the outpost to restore the beacon."
            }
            (Self::ReturnCore, Target::Keeper) => {
                *self = Self::Complete;
                "NEMI\n\nThere it is. I thought we had lost that light for good.\n\nLook up. The crossing is safe again. Somewhere out on the ice,\nsomeone has seen it, and knows there is still a home to return to.\n\nQUEST COMPLETE / THE LAST LIGHT"
            }
            (Self::Complete, Target::Keeper) => {
                "NEMI\n\nOur light reaches across the ice again. Stay a while, traveler.\nThere will always be a place for you here."
            }
            _ => "A sealed power core. Speak to the keeper before disturbing it.",
        }
    }
    pub fn objective(self) -> &'static str {
        match self {
            Self::MeetKeeper => "Speak to Nemi, the beacon keeper",
            Self::FindCore => "Follow the blue waystones | Recover the cavern core",
            Self::ReturnCore => "Return the power core to Nemi",
            Self::Complete => "Beacon restored | The outpost welcomes you",
        }
    }
    pub fn destination(self) -> Option<Vec3> {
        match self {
            Self::MeetKeeper | Self::ReturnCore => Some(KEEPER),
            Self::FindCore => Some(CORE),
            Self::Complete => None,
        }
    }
}
#[derive(Resource, Default)]
pub struct Dialogue {
    pub text: Option<&'static str>,
}
#[derive(Resource, Default)]
pub struct Nearby(pub Option<(Entity, Target)>);
#[derive(Component)]
pub struct Beacon;
#[derive(Component)]
struct BeaconLens;
#[derive(Event)]
pub struct SpawnBeaconLens;
impl Plugin for QuestPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Quest>()
            .init_resource::<Dialogue>()
            .init_resource::<Nearby>()
            .add_observer(spawn_lens)
            .add_systems(
                Update,
                (find_target, interact, effects)
                    .chain()
                    .run_if(in_state(GameState::Playing)),
            );
    }
}
fn find_target(
    players: Query<(Entity, &Transform), With<Player>>,
    targets: Query<(Entity, &Target, &Transform)>,
    quest: Res<Quest>,
    dialogue: Res<Dialogue>,
    spatial: SpatialQuery,
    mut nearby: ResMut<Nearby>,
) {
    nearby.0 = None;
    if dialogue.text.is_some() {
        return;
    }
    let Ok((player_entity, player)) = players.single() else {
        return;
    };
    let origin = player.translation + Vec3::Y * 0.45;
    let mut closest = 3.0;
    for (entity, target, transform) in &targets {
        if *target == Target::Core && *quest != Quest::FindCore {
            continue;
        }
        let point = transform.translation
            + if *target == Target::Keeper {
                Vec3::Y * 0.35
            } else {
                Vec3::ZERO
            };
        let delta = point - origin;
        let distance = delta.length();
        if distance >= closest {
            continue;
        }
        let Ok(direction) = Dir3::new(delta) else {
            continue;
        };
        if spatial
            .cast_ray(
                origin,
                direction,
                distance,
                true,
                &SpatialQueryFilter::from_excluded_entities([player_entity, entity]),
            )
            .is_none()
        {
            closest = distance;
            nearby.0 = Some((entity, *target));
        }
    }
}
fn interact(
    keys: Res<ButtonInput<KeyCode>>,
    nearby: Res<Nearby>,
    mut dialogue: ResMut<Dialogue>,
    mut quest: ResMut<Quest>,
    mut commands: Commands,
) {
    if !keys.just_pressed(KeyCode::KeyE) && !keys.just_pressed(KeyCode::Enter) {
        return;
    }
    if dialogue.text.take().is_some() {
        return;
    }
    if let Some((entity, target)) = nearby.0 {
        dialogue.text = Some(quest.interact(target));
        if target == Target::Core && *quest == Quest::ReturnCore {
            commands.entity(entity).despawn();
        }
    }
}
fn spawn_lens(
    _: On<SpawnBeaconLens>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        BeaconLens,
        Beacon,
        Mesh3d(meshes.add(Sphere::new(0.65))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.04, 0.1, 0.13),
            ..default()
        })),
        Transform::from_xyz(0.0, 7.5, 5.0),
    ));
}
fn effects(
    quest: Res<Quest>,
    mut beacons: Query<&mut PointLight, With<Beacon>>,
    lens: Query<&MeshMaterial3d<StandardMaterial>, With<BeaconLens>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !quest.is_changed() || *quest != Quest::Complete {
        return;
    }
    for mut light in &mut beacons {
        light.intensity = 450_000.0;
    }
    for handle in &lens {
        if let Some(mut material) = materials.get_mut(&handle.0) {
            material.emissive = LinearRgba::rgb(3.0, 15.0, 20.0);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_quest_in_order() {
        let mut q = Quest::default();
        q.interact(Target::Keeper);
        assert_eq!(q, Quest::FindCore);
        q.interact(Target::Core);
        assert_eq!(q, Quest::ReturnCore);
        q.interact(Target::Keeper);
        assert_eq!(q, Quest::Complete);
        assert!(q.destination().is_none());
    }
    #[test]
    fn premature_and_duplicate_interactions_do_not_skip_stages() {
        let mut q = Quest::default();
        q.interact(Target::Core);
        assert_eq!(q, Quest::MeetKeeper);
        q.interact(Target::Keeper);
        q.interact(Target::Keeper);
        assert_eq!(q, Quest::FindCore);
        q.interact(Target::Core);
        q.interact(Target::Core);
        assert_eq!(q, Quest::ReturnCore);
        q.interact(Target::Keeper);
        q.interact(Target::Keeper);
        q.interact(Target::Core);
        assert_eq!(q, Quest::Complete);
    }
}
