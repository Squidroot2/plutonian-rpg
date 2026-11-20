pub use crate::interaction::{Nearby, Target};
use crate::inventory::{Inventory, ItemKind};
use crate::{
    GameState,
    world::{CORE, KEEPER},
};
use bevy::prelude::*;

pub struct QuestPlugin;
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quest {
    #[default]
    MeetKeeper,
    FindCore,
    ReturnCore,
    Complete,
}
impl Quest {
    pub fn interact(&mut self, target: Target, inventory: &mut Inventory) -> &'static str {
        match (*self, target) {
            (Self::MeetKeeper, Target::Keeper) => {
                *self = Self::FindCore;
                "NEMI / BEACON KEEPER\n\nYou made it home before the long dark. Our beacon has gone silent.\nThe families here depend on its signal to cross the ice.\n\nA charged core waits in the blue cavern, beyond the waystones.\nFollow their lights north. Bring it back, and we can guide our people home."
            }
            (Self::FindCore, Target::Keeper) => {
                "NEMI\n\nFollow the blue waystones beyond the outpost. The core is inside\nthe cavern at the end of the trail. I'll keep watch here."
            }
            (Self::FindCore, Target::Core) if inventory.add(ItemKind::BeaconCore, 1) => {
                *self = Self::ReturnCore;
                "POWER CORE RECOVERED\n\nIts light pulses against your hands. There is still warmth inside.\n\nReturn to Nemi at the outpost to restore the beacon."
            }
            (Self::ReturnCore, Target::Keeper) if inventory.consume_core() => {
                *self = Self::Complete;
                "NEMI\n\nThere it is. I thought we had lost that light for good.\n\nLook up. The crossing is safe again. Somewhere out on the ice,\nsomeone has seen it, and knows there is still a home to return to.\n\nQUEST COMPLETE / THE LAST LIGHT"
            }
            (Self::Complete, Target::Keeper) => {
                "NEMI\n\nOur light reaches across the ice again. Stay a while, traveler.\nThere will always be a place for you here."
            }
            (Self::ReturnCore, Target::Keeper) => {
                "NEMI\n\nBring the beacon power core back to me first."
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
            .add_observer(spawn_lens)
            .add_systems(
                Update,
                effects
                    .run_if(in_state(GameState::Playing))
                    .after(crate::interaction::InteractionSystems),
            );
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
        let mut inventory = Inventory::default();
        q.interact(Target::Keeper, &mut inventory);
        assert_eq!(q, Quest::FindCore);
        q.interact(Target::Core, &mut inventory);
        assert_eq!(q, Quest::ReturnCore);
        q.interact(Target::Keeper, &mut inventory);
        assert_eq!(q, Quest::Complete);
        assert!(q.destination().is_none());
    }
    #[test]
    fn turn_in_requires_and_consumes_the_core() {
        let mut quest = Quest::ReturnCore;
        let mut inventory = Inventory::default();
        quest.interact(Target::Keeper, &mut inventory);
        assert_eq!(quest, Quest::ReturnCore);
        inventory.add(ItemKind::BeaconCore, 1);
        quest.interact(Target::Keeper, &mut inventory);
        assert_eq!(quest, Quest::Complete);
        assert_eq!(inventory.count(ItemKind::BeaconCore), 0);
    }
    #[test]
    fn premature_and_duplicate_interactions_do_not_skip_stages() {
        let mut q = Quest::default();
        let mut inventory = Inventory::default();
        q.interact(Target::Core, &mut inventory);
        assert_eq!(q, Quest::MeetKeeper);
        q.interact(Target::Keeper, &mut inventory);
        q.interact(Target::Keeper, &mut inventory);
        assert_eq!(q, Quest::FindCore);
        q.interact(Target::Core, &mut inventory);
        q.interact(Target::Core, &mut inventory);
        assert_eq!(q, Quest::ReturnCore);
        q.interact(Target::Keeper, &mut inventory);
        q.interact(Target::Keeper, &mut inventory);
        q.interact(Target::Core, &mut inventory);
        assert_eq!(q, Quest::Complete);
    }
}
