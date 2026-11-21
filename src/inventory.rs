use std::collections::BTreeMap;

use avian3d::prelude::*;
use bevy::{gltf::Gltf, prelude::*, world_serialization::WorldInstanceReady};

use crate::{
    GameState,
    equipment::Equipment,
    loading::{GameAssets, PendingScene},
    player::Player,
    world::{SpawnWorld, WorldLayout},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ItemKind {
    Battery,
    Scrap,
    MethanePouch,
    BeaconCore,
    FieldArmor,
    ReinforcedArmor,
    FieldHelmet,
    ReinforcedHelmet,
    FieldSpear,
    Trident,
    LargeBoulder,
}
impl ItemKind {
    pub const MATERIALS: [Self; 3] = [Self::Battery, Self::Scrap, Self::MethanePouch];
    pub const GEAR: [Self; 6] = [
        Self::FieldArmor,
        Self::ReinforcedArmor,
        Self::FieldHelmet,
        Self::ReinforcedHelmet,
        Self::FieldSpear,
        Self::Trident,
    ];
    pub const ALL: [Self; 11] = [
        Self::Battery,
        Self::Scrap,
        Self::MethanePouch,
        Self::FieldArmor,
        Self::ReinforcedArmor,
        Self::FieldHelmet,
        Self::ReinforcedHelmet,
        Self::FieldSpear,
        Self::Trident,
        Self::BeaconCore,
        Self::LargeBoulder,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Battery => "Battery",
            Self::Scrap => "Scrap",
            Self::MethanePouch => "Methane pouch",
            Self::BeaconCore => "Beacon power core",
            Self::FieldSpear => "Field spear",
            Self::Trident => "Trident",
            Self::LargeBoulder => "Large Boulder",
            Self::FieldArmor => "Field armor",
            Self::ReinforcedArmor => "Reinforced armor",
            Self::FieldHelmet => "Field helmet",
            Self::ReinforcedHelmet => "Reinforced helmet",
        }
    }
    pub fn asset(self) -> &'static str {
        match self {
            Self::Battery => "battery",
            Self::Scrap => "scrap",
            Self::MethanePouch => "methane_pouch",
            Self::BeaconCore => "core",
            Self::FieldSpear => "field_spear",
            Self::Trident => "trident",
            Self::LargeBoulder => "large_boulder",
            Self::FieldArmor => "field_armor",
            Self::ReinforcedArmor => "reinforced_armor",
            Self::FieldHelmet => "field_helmet",
            Self::ReinforcedHelmet => "reinforced_helmet",
        }
    }
    pub fn weight(self) -> u64 {
        match self {
            Self::Battery | Self::MethanePouch => 1,
            Self::Scrap | Self::FieldHelmet => 2,
            Self::FieldSpear | Self::ReinforcedHelmet => 3,
            Self::Trident => 4,
            Self::FieldArmor => 5,
            Self::BeaconCore => 6,
            Self::ReinforcedArmor => 8,
            Self::LargeBoulder => 40,
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::LargeBoulder => "A dense chunk of icebound rock.",
            Self::Battery => "A sealed cell holding a little warmth and charge.",
            Self::Scrap => "Salvaged metal from the outpost's machines.",
            Self::MethanePouch => "A reinforced pouch of condensed methane.",
            Self::BeaconCore => "Quest item. Return this core to Nemi.",
            Self::FieldSpear | Self::Trident => "Aim with the camera and left click to thrust.",
            Self::FieldArmor => "Light fitted plates used by outpost explorers.",
            Self::ReinforcedArmor => "A reinforced cuirass for crossing the deep ice.",
            Self::FieldHelmet => "An open-face helmet with space for sensory horns.",
            Self::ReinforcedHelmet => "A reinforced helmet with luminous identification marks.",
        }
    }
}
pub(crate) const CARRY_CAPACITY: u64 = 60;
pub(crate) const IMMOBILE_WEIGHT: u64 = CARRY_CAPACITY * 6 / 5;

#[derive(Resource, Default, Debug)]
pub struct Inventory(BTreeMap<ItemKind, u32>);
impl Inventory {
    pub(crate) fn carried_weight(&self, equipment: &Equipment) -> u64 {
        self.0
            .iter()
            .map(|(&item, &quantity)| item.weight() * u64::from(quantity))
            .sum::<u64>()
            + equipment.weight()
    }

    pub fn count(&self, item: ItemKind) -> u32 {
        self.0.get(&item).copied().unwrap_or(0)
    }
    pub fn add(&mut self, item: ItemKind, quantity: u32) -> bool {
        let Some(total) = self.count(item).checked_add(quantity) else {
            return false;
        };
        if quantity == 0 || (item == ItemKind::BeaconCore && total > 1) {
            return false;
        }
        self.0.insert(item, total);
        true
    }
    pub(crate) fn remove(&mut self, item: ItemKind, quantity: u32) -> bool {
        let count = self.count(item);
        if quantity == 0 || quantity > count {
            return false;
        }
        if quantity == count {
            self.0.remove(&item);
        } else {
            self.0.insert(item, count - quantity);
        }
        true
    }
    pub fn consume_core(&mut self) -> bool {
        self.remove(ItemKind::BeaconCore, 1)
    }
    pub fn drop_stack(
        &mut self,
        item: ItemKind,
        quantity: u32,
        placement: Option<Vec3>,
    ) -> Option<Pickup> {
        if item == ItemKind::BeaconCore || placement.is_none() || !self.remove(item, quantity) {
            return None;
        }
        Some(Pickup { item, quantity })
    }
}
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pickup {
    pub item: ItemKind,
    pub quantity: u32,
}
#[derive(Event)]
pub struct DropRequest {
    pub item: ItemKind,
    pub quantity: u32,
}
#[derive(Resource, Default)]
pub struct InventoryNotice(pub String);

pub struct InventoryPlugin;
impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Inventory>()
            .init_resource::<InventoryNotice>()
            .add_observer(spawn_loot)
            .add_observer(drop_items)
            .add_systems(OnExit(GameState::Loading), ground_pickups);
    }
}
pub fn scene_ready(event: On<WorldInstanceReady>, mut commands: Commands) {
    commands.entity(event.entity).remove::<PendingScene>();
}
fn spawn_loot(
    _: On<SpawnWorld>,
    mut commands: Commands,
    assets: Res<GameAssets>,
    gltfs: Res<Assets<Gltf>>,
    layout: Res<WorldLayout>,
) {
    if *layout == WorldLayout::SmokeArena {
        for (item, x, z) in crate::smoke::ARENA_LOOT {
            spawn_pickup(
                &mut commands,
                &assets,
                &gltfs,
                Pickup { item, quantity: 1 },
                Vec3::new(x, 2.0, z),
            );
        }
        return;
    }
    // Two of each material in each region; all positions stay inside the traversable corridor.
    for region_z in [-2.0, -55.0, -110.0] {
        for (index, item) in ItemKind::MATERIALS.into_iter().enumerate() {
            for side in [-1.0, 1.0] {
                let position = Vec3::new(
                    side * 2.0,
                    2.0,
                    region_z - index as f32 * 4.0 - if side > 0.0 { 1.5 } else { 0.0 },
                );
                spawn_pickup(
                    &mut commands,
                    &assets,
                    &gltfs,
                    Pickup { item, quantity: 1 },
                    position,
                );
            }
        }
    }
    for (item, position) in [
        (ItemKind::FieldSpear, Vec3::new(6.0, 2.0, 5.0)),
        (ItemKind::Trident, Vec3::new(1.5, 2.0, 5.0)),
        (ItemKind::FieldArmor, Vec3::new(-5.0, 2.0, -5.0)),
        (ItemKind::FieldHelmet, Vec3::new(4.0, 2.0, -9.0)),
        (ItemKind::ReinforcedArmor, Vec3::new(-3.0, 2.0, -115.0)),
        (ItemKind::ReinforcedHelmet, Vec3::new(3.0, 2.0, -120.0)),
    ] {
        spawn_pickup(
            &mut commands,
            &assets,
            &gltfs,
            Pickup { item, quantity: 1 },
            position,
        );
    }
}
pub fn spawn_pickup(
    commands: &mut Commands,
    assets: &GameAssets,
    gltfs: &Assets<Gltf>,
    pickup: Pickup,
    position: Vec3,
) {
    commands
        .spawn((
            Name::new(pickup.item.name()),
            pickup,
            PendingScene,
            WorldAssetRoot(gltfs.get(&assets.pickups[&pickup.item]).expect("pickup glTF assets must remain loaded after validation so initial and dropped pickups can be spawned").scenes[0].clone()),
            Transform::from_translation(position),
        ))
        .observe(scene_ready);
}
fn ground_pickups(mut pickups: Query<&mut Transform, With<Pickup>>, spatial: SpatialQuery) {
    for mut transform in &mut pickups {
        // Stay below the cave roof; the provisional height is 2 m across the whole map.
        let origin = transform.translation + Vec3::Y;
        if let Some(hit) = spatial.cast_ray(
            origin,
            Dir3::NEG_Y,
            15.0,
            true,
            &SpatialQueryFilter::default(),
        ) {
            transform.translation.y = origin.y - hit.distance + 0.012;
        }
    }
}
pub fn find_drop_position(
    spatial: &SpatialQuery,
    player: Entity,
    transform: &Transform,
    item: ItemKind,
) -> Option<Vec3> {
    let size = if item == ItemKind::LargeBoulder {
        Vec3::new(1.12, 0.85, 0.98)
    } else {
        item.gear()
            .map_or(Vec3::new(0.62, 0.6, 0.62), |gear| gear.pickup_size)
    };
    let shape = Collider::cuboid(size.x, size.y, size.z);
    let forward = transform.rotation * Vec3::Z;
    let right = transform.rotation * Vec3::X;
    let filter = SpatialQueryFilter::from_excluded_entities([player]);
    for offset in [
        forward * 1.2,
        forward * 1.7,
        forward + right * 0.8,
        forward - right * 0.8,
        -forward,
    ] {
        let origin = transform.translation + offset + Vec3::Y * 1.3;
        let Some(hit) = spatial.cast_ray(origin, Dir3::NEG_Y, 4.0, true, &filter) else {
            continue;
        };
        if hit.normal.y < 0.7 {
            continue;
        }
        let base = Vec3::new(origin.x, origin.y - hit.distance + 0.012, origin.z);
        // Prevent a drop through a wall or into a solid object. Pickups have no physics collider.
        let center = base + Vec3::Y * (size.y * 0.5 + 0.02);
        if !spatial
            .shape_intersections(&shape, center, Quat::IDENTITY, &filter)
            .is_empty()
        {
            continue;
        }
        let start = transform.translation + Vec3::Y * 0.15;
        let delta = center - start;
        let Ok(direction) = Dir3::new(delta) else {
            continue;
        };
        if spatial
            .cast_shape(
                &shape,
                start,
                Quat::IDENTITY,
                direction,
                &ShapeCastConfig::from_max_distance(delta.length()),
                &filter,
            )
            .is_none()
        {
            return Some(base);
        }
    }
    None
}
fn drop_items(
    request: On<DropRequest>,
    state: Res<State<GameState>>,
    mut inventory: ResMut<Inventory>,
    players: Query<(Entity, &Transform), With<Player>>,
    spatial: SpatialQuery,
    mut notice: ResMut<InventoryNotice>,
    mut commands: Commands,
    assets: Res<GameAssets>,
    gltfs: Res<Assets<Gltf>>,
) {
    if *state.get() != GameState::Inventory {
        return;
    }
    let Ok((entity, transform)) = players.single() else {
        return;
    };
    let placement = find_drop_position(&spatial, entity, transform, request.item);
    if let Some(pickup) = inventory.drop_stack(request.item, request.quantity, placement) {
        spawn_pickup(
            &mut commands,
            &assets,
            &gltfs,
            pickup,
            placement.expect("a successful drop_stack call requires an accepted world placement"),
        );
        notice.0 = format!("Dropped {} x{}", request.item.name(), request.quantity);
    } else {
        notice.0 = if placement.is_none() {
            "No clear ground nearby. Move to an open area and try again.".into()
        } else {
            "That quantity cannot be dropped.".into()
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stacks_and_partial_full_drops_conserve_items() {
        let mut bag = Inventory::default();
        assert!(bag.add(ItemKind::Battery, 2));
        assert!(bag.add(ItemKind::Battery, 3));
        let stack = bag
            .drop_stack(ItemKind::Battery, 2, Some(Vec3::ZERO))
            .expect("dropping two of five batteries onto a valid position must return a stack");
        assert_eq!(bag.count(ItemKind::Battery), 3);
        assert_eq!(stack.quantity, 2);
        assert!(bag.add(stack.item, stack.quantity));
        assert_eq!(bag.count(ItemKind::Battery), 5);
        bag.drop_stack(ItemKind::Battery, 5, Some(Vec3::ZERO))
            .expect(
                "dropping all five remaining batteries onto a valid position must return a stack",
            );
        assert_eq!(bag.count(ItemKind::Battery), 0);
        assert!(bag.0.is_empty());
    }
    #[test]
    fn rejects_invalid_quantities_and_unsafe_placement_without_loss() {
        let mut bag = Inventory::default();
        bag.add(ItemKind::Scrap, 3);
        for quantity in [0, 4, u32::MAX] {
            assert!(
                bag.drop_stack(ItemKind::Scrap, quantity, Some(Vec3::ZERO))
                    .is_none()
            );
        }
        assert!(bag.drop_stack(ItemKind::Scrap, 2, None).is_none());
        assert!(!bag.add(ItemKind::Scrap, u32::MAX));
        assert_eq!(bag.count(ItemKind::Scrap), 3);
    }
    #[test]
    fn core_cannot_be_duplicated_or_dropped() {
        let mut bag = Inventory::default();
        assert!(bag.add(ItemKind::BeaconCore, 1));
        assert!(!bag.add(ItemKind::BeaconCore, 1));
        assert!(
            bag.drop_stack(ItemKind::BeaconCore, 1, Some(Vec3::ZERO))
                .is_none()
        );
        assert!(bag.consume_core());
        assert!(!bag.consume_core());
    }
}
