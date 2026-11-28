use std::collections::BTreeMap;

use avian3d::prelude::*;
use az::Az;
use bevy::{gltf::Gltf, prelude::*};

use crate::{
    GameState,
    content::{CollectedPickups, Items, PlacedPickup, Placements},
    equipment::Equipment,
    loading::{GameAssets, PendingScene, scene_ready},
    player::Player,
    player::PlayerWrite,
    save::RunSave,
    ui::menu_ui::{MenuTab, PlayerMenu},
    world::{SessionEntity, SpawnWorld, WorldLayout},
};

pub use crate::content::ItemKind;

#[derive(
    Component,
    Clone,
    Debug,
    bitcode::Encode,
    bitcode::Decode,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
pub struct Inventory<I: Ord = ItemKind>(pub(crate) BTreeMap<I, u32>);
impl<I: Ord> Default for Inventory<I> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl Inventory {
    pub(crate) fn valid_stacks(&self) -> bool {
        self.0.values().all(|&quantity| quantity > 0) && self.count(ItemKind::BeaconCore) <= 1
    }

    pub(crate) fn carried_weight(&self, equipment: &Equipment, items: &Items) -> u64 {
        self.0
            .iter()
            .map(|(&item, &quantity)| items[item].weight() * u64::from(quantity))
            .fold(equipment.weight(items), u64::saturating_add)
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
    pub(crate) fn transfer_to(
        &mut self,
        destination: &mut Self,
        item: ItemKind,
        quantity: u32,
    ) -> bool {
        if quantity > self.count(item) || !destination.add(item, quantity) {
            return false;
        }
        assert!(
            self.remove(item, quantity),
            "the source stack was validated"
        );
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
#[derive(
    Component,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    bitcode::Encode,
    bitcode::Decode,
    serde::Deserialize,
    serde::Serialize,
    schemars::JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct Pickup<I = ItemKind> {
    pub item: I,
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
        app.init_resource::<InventoryNotice>()
            .add_observer(spawn_loot)
            .add_observer(drop_items)
            .add_systems(OnExit(GameState::Loading), ground_pickups);
    }
}
#[derive(Component)]
struct NeedsPickupGrounding;

fn spawn_initial_pickup(
    items: &Items,
    commands: &mut Commands,
    assets: &GameAssets,
    gltfs: &Assets<Gltf>,
    item: ItemKind,
    position: Vec3,
) {
    let entity = spawn_pickup(
        items,
        commands,
        assets,
        gltfs,
        Pickup { item, quantity: 1 },
        position,
    );
    commands.entity(entity).insert(NeedsPickupGrounding);
}

fn spawn_shortswords(
    items: &Items,
    commands: &mut Commands,
    assets: &GameAssets,
    gltfs: &Assets<Gltf>,
) {
    for x in [3.25, 5.25] {
        spawn_initial_pickup(
            items,
            commands,
            assets,
            gltfs,
            ItemKind::Shortsword,
            Vec3::new(x, 2.0, 6.5),
        );
    }
}

fn spawn_loot(
    _: On<SpawnWorld>,
    items: Res<Items>,
    mut commands: Commands,
    assets: Res<GameAssets>,
    gltfs: Res<Assets<Gltf>>,
    layout: Res<WorldLayout>,
    run: Res<RunSave>,
    placements: Res<Placements>,
    collected: Res<CollectedPickups>,
) {
    if *layout == WorldLayout::Outpost {
        spawn_authored_pickups(
            &mut commands,
            &assets,
            &gltfs,
            &items,
            &placements,
            &collected,
        );
    }
    if let Some(save) = run.checkpoint() {
        for pickup in &save.pickups {
            spawn_pickup(
                &items,
                &mut commands,
                &assets,
                &gltfs,
                pickup.stack,
                pickup.position.into(),
            );
        }
        return;
    }
    if *layout == WorldLayout::SmokeArena {
        for (item, x, z) in crate::world::ARENA_LOOT {
            spawn_initial_pickup(
                &items,
                &mut commands,
                &assets,
                &gltfs,
                item,
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
                    region_z - index.az::<f32>() * 4.0 - if side > 0.0 { 1.5 } else { 0.0 },
                );
                spawn_initial_pickup(&items, &mut commands, &assets, &gltfs, item, position);
            }
        }
    }
    for (item, position) in [
        (ItemKind::FieldSpear, Vec3::new(6.0, 2.0, 5.0)),
        (ItemKind::Trident, Vec3::new(1.5, 2.0, 5.0)),
        (ItemKind::ReinforcedArmor, Vec3::new(-3.0, 2.0, -115.0)),
        (ItemKind::ReinforcedHelmet, Vec3::new(3.0, 2.0, -120.0)),
    ] {
        spawn_initial_pickup(&items, &mut commands, &assets, &gltfs, item, position);
    }
    spawn_shortswords(&items, &mut commands, &assets, &gltfs);
}
fn spawn_authored_pickups(
    commands: &mut Commands,
    assets: &GameAssets,
    gltfs: &Assets<Gltf>,
    items: &Items,
    placements: &Placements,
    collected: &CollectedPickups,
) {
    for placement in placements
        .pickups()
        .filter(|entry| !collected.0.contains(&entry.id))
    {
        let entity = spawn_pickup(
            items,
            commands,
            assets,
            gltfs,
            placement.stack,
            placement.position.into(),
        );
        commands.entity(entity).insert((
            PlacedPickup {
                id: placement.id,
                area: placement.area,
            },
            NeedsPickupGrounding,
        ));
    }
}

pub fn spawn_pickup(
    items: &Items,
    commands: &mut Commands,
    assets: &GameAssets,
    gltfs: &Assets<Gltf>,
    pickup: Pickup,
    position: Vec3,
) -> Entity {
    commands
        .spawn((
            SessionEntity,
            Name::new(items[pickup.item].name().to_owned()),
            pickup,
            PendingScene,
            WorldAssetRoot(gltfs.get(&assets.pickups[&pickup.item]).expect("pickup glTF assets must remain loaded after validation so initial and dropped pickups can be spawned").scenes[0].clone()),
            Transform::from_translation(position),
        ))
        .observe(scene_ready)
        .id()
}
fn ground_pickups(
    mut pickups: Query<(Entity, &mut Transform), With<NeedsPickupGrounding>>,
    spatial: SpatialQuery,
    mut commands: Commands,
) {
    for (entity, mut transform) in &mut pickups {
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
        commands.entity(entity).remove::<NeedsPickupGrounding>();
    }
}
pub fn find_drop_position(
    items: &Items,
    spatial: &SpatialQuery,
    player: Entity,
    transform: &Transform,
    item: ItemKind,
) -> Option<Vec3> {
    let size = items[item].pickup_size();
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
    items: Res<Items>,
    state: Res<State<GameState>>,
    menu: Res<PlayerMenu>,
    mut inventory: PlayerWrite<Inventory>,
    players: Query<(Entity, &Transform), With<Player>>,
    spatial: SpatialQuery,
    mut notice: ResMut<InventoryNotice>,
    mut commands: Commands,
    assets: Res<GameAssets>,
    gltfs: Res<Assets<Gltf>>,
) {
    if *state.get() != GameState::Playing || !menu.accepts(MenuTab::Inventory) {
        return;
    }
    let Ok((entity, transform)) = players.single() else {
        return;
    };
    let placement = find_drop_position(&items, &spatial, entity, transform, request.item);
    if let Some(pickup) = inventory.drop_stack(request.item, request.quantity, placement) {
        spawn_pickup(
            &items,
            &mut commands,
            &assets,
            &gltfs,
            pickup,
            placement.expect("a successful drop_stack call requires an accepted world placement"),
        );
        notice.0 = format!(
            "Dropped {} x{}",
            items[request.item].name(),
            request.quantity
        );
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
    fn storage_transfers_every_item_kind_in_both_directions() {
        let mut backpack = Inventory::default();
        let mut chest = Inventory::default();
        for item in crate::content::test_items().ids() {
            assert!(backpack.add(item, 1));
            assert!(backpack.transfer_to(&mut chest, item, 1));
            assert_eq!(backpack.count(item), 0);
            assert_eq!(chest.count(item), 1);
            assert!(chest.transfer_to(&mut backpack, item, 1));
            assert_eq!(backpack.count(item), 1);
            assert_eq!(chest.count(item), 0);
        }
    }

    #[test]
    fn rejected_storage_transfers_leave_both_inventories_intact() {
        let mut source = Inventory::default();
        let mut destination = Inventory::default();
        assert!(source.add(ItemKind::Scrap, 2));
        assert!(destination.add(ItemKind::Scrap, u32::MAX - 1));
        for quantity in [0, 2, 3] {
            assert!(!source.transfer_to(&mut destination, ItemKind::Scrap, quantity));
            assert_eq!(source.count(ItemKind::Scrap), 2);
            assert_eq!(destination.count(ItemKind::Scrap), u32::MAX - 1);
        }
        assert!(source.add(ItemKind::BeaconCore, 1));
        assert!(destination.add(ItemKind::BeaconCore, 1));
        assert!(!source.transfer_to(&mut destination, ItemKind::BeaconCore, 1));
        assert_eq!(source.count(ItemKind::BeaconCore), 1);
        assert_eq!(destination.count(ItemKind::BeaconCore), 1);
    }
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
