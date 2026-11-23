use super::{Catalog, ContentId, Enemies, EnemyArchetype, EnemyDefinition, Items};
use crate::{
    dungeon::Area,
    enemy,
    inventory::{Inventory, ItemKind, Pickup},
    loading::EnemyVisuals,
    save::EncounterId,
    storage::{ChestId, ChestStorage},
    world::{SpawnWorld, WorldLayout},
};
use bevy::prelude::*;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Asset, TypePath, Resource, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Placements {
    starter_stock: Vec<Stock>,
    encounter_assignments: Vec<Assignment>,
    pickups: Vec<PickupPlacement>,
    encounters: Vec<PlacedEncounter>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stock {
    chest: ChestId,
    stacks: Vec<Pickup>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Assignment {
    encounter: ContentId,
    enemy: ContentId,
}

#[derive(Component, Clone, Copy)]
pub(crate) struct PlacedPickup {
    pub(crate) id: ContentId,
    pub(crate) area: Area,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PickupPlacement {
    pub(crate) id: ContentId,
    pub(crate) area: Area,
    pub(crate) position: [f32; 3],
    pub(crate) stack: Pickup,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlacedEncounter {
    id: ContentId,
    enemy: ContentId,
    area: Area,
    position: [f32; 3],
    #[serde(default)]
    yaw_degrees: f32,
}

#[derive(Resource, Default)]
pub(crate) struct CollectedPickups(pub(crate) BTreeSet<ContentId>);

impl Catalog for Placements {
    const PATH: &'static str = "data://world/catalog.placement.ron";
    const EXTENSION: &'static str = "placement.ron";
}

impl Placements {
    pub(crate) fn stock(&self) -> ChestStorage {
        ChestStorage(
            self.starter_stock
                .iter()
                .map(|entry| {
                    let contents = entry
                        .stacks
                        .iter()
                        .map(|stack| (stack.item, stack.quantity));
                    (entry.chest, Inventory(contents.collect()))
                })
                .collect(),
        )
    }

    pub(crate) fn enemy<'a>(
        &self,
        enemies: &'a Enemies,
        encounter: EncounterId,
    ) -> &'a EnemyDefinition {
        let key = encounter.key();
        let id = self
            .encounter_assignments
            .iter()
            .find(|entry| entry.encounter == key)
            .map_or_else(|| encounter.archetype().id(), |entry| entry.enemy);
        enemies
            .get(id)
            .expect("shipped encounters must reference a validated enemy definition")
    }

    pub(crate) fn encounter_ids(&self) -> impl Iterator<Item = EncounterId> + '_ {
        self.encounters
            .iter()
            .map(|entry| EncounterId::Added(entry.id))
    }

    pub(crate) fn pickups(&self) -> impl Iterator<Item = &PickupPlacement> {
        self.pickups.iter()
    }

    pub(crate) fn validate(&self, items: &Items, enemies: &Enemies) -> Result<(), String> {
        self.validate_stock(items)
            .and_then(|()| self.validate_assignments(enemies))
            .and_then(|()| self.validate_placements(items, enemies))
            .map_err(|error| format!("{}: {error}", Self::PATH))
    }

    fn validate_stock(&self, items: &Items) -> Result<(), String> {
        let mut chests = BTreeSet::new();
        for entry in &self.starter_stock {
            if !chests.insert(entry.chest) {
                return Err(format!("Duplicate starter stock for {:?}", entry.chest));
            }
            let mut ids = BTreeSet::new();
            for &stack in &entry.stacks {
                items
                    .validate_stack(stack)
                    .map_err(|error| format!("{:?} chest: {error}", entry.chest))?;
                if !ids.insert(stack.item) {
                    return Err(format!(
                        "Duplicate {} in {:?} chest",
                        stack.item, entry.chest
                    ));
                }
            }
        }
        if chests.len() != ChestId::ALL.len() {
            return Err(
                "Starter stock must list Lamp, Supply, and Gear, with empty lists allowed".into(),
            );
        }
        Ok(())
    }

    fn validate_assignments(&self, enemies: &Enemies) -> Result<(), String> {
        let shipped: BTreeMap<_, _> = EncounterId::shipped().map(|id| (id.key(), id)).collect();
        let mut assignments = BTreeSet::new();
        for entry in &self.encounter_assignments {
            let key = entry.encounter;
            if !assignments.insert(key) {
                return Err(format!("Duplicate assignment for {key}"));
            }
            let encounter = shipped
                .get(&key)
                .ok_or_else(|| format!("Unknown shipped encounter {key}"))?;
            let enemy = enemies
                .get(entry.enemy)
                .map_err(|error| format!("Encounter {key}: {error}"))?;
            if enemy.archetype() != encounter.archetype() {
                return Err(format!(
                    "Encounter {key} must use {:?}",
                    encounter.archetype()
                ));
            }
        }
        for (key, encounter) in shipped {
            let enemy = self.enemy(enemies, encounter);
            if enemy.archetype() == EnemyArchetype::Hushcap
                && enemy.loot().item != ItemKind::Mycelium
            {
                return Err(format!("Quest encounter {key} must drop Mycelium"));
            }
        }
        Ok(())
    }

    fn validate_placements(&self, items: &Items, enemies: &Enemies) -> Result<(), String> {
        let mut ids: BTreeSet<_> = EncounterId::shipped().map(EncounterId::key).collect();
        for entry in &self.encounters {
            let id = entry.id;
            let enemy = enemies
                .get(entry.enemy)
                .map_err(|error| format!("Encounter {id}: {error}"))?;
            if !ids.insert(id) {
                return Err(format!("Duplicate or reserved encounter ID {id}"));
            }
            if enemy.archetype() == EnemyArchetype::Guard {
                return Err(format!(
                    "Encounter {id} must use an archetype that can spawn outside a guard post"
                ));
            }
            if !Vec3::from_array(entry.position).is_finite() || !entry.yaw_degrees.is_finite() {
                return Err(format!(
                    "Encounter {id} has a non-finite position or rotation"
                ));
            }
        }
        let mut pickups = BTreeSet::new();
        for entry in &self.pickups {
            let id = entry.id;
            if !pickups.insert(id) || !Vec3::from_array(entry.position).is_finite() {
                return Err(format!(
                    "Pickup {id} has a duplicate ID or non-finite position"
                ));
            }
            items
                .validate_stack(entry.stack)
                .map_err(|error| format!("Pickup {id}: {error}"))?;
        }
        Ok(())
    }
}

pub(super) fn spawn_encounters(
    _: On<SpawnWorld>,
    layout: Res<WorldLayout>,
    placements: Res<Placements>,
    enemies: Res<Enemies>,
    visuals: Res<EnemyVisuals>,
    mut commands: Commands,
) {
    if *layout != WorldLayout::Outpost {
        return;
    }
    for entry in &placements.encounters {
        let home = Transform::from_translation(entry.position.into())
            .with_rotation(Quat::from_rotation_y(entry.yaw_degrees.to_radians()));
        let definition = enemies
            .get(entry.enemy)
            .expect("placed encounters must reference validated enemies");
        enemy::spawn_variant(
            &mut commands,
            &visuals,
            definition,
            home,
            entry.area,
            EncounterId::Added(entry.id),
        );
    }
}

#[cfg(test)]
pub(crate) fn test_placements() -> &'static Placements {
    static PLACEMENTS: std::sync::OnceLock<Placements> = std::sync::OnceLock::new();
    PLACEMENTS.get_or_init(|| {
        let placements: Placements =
            ron::from_str(include_str!("../../data/world/catalog.placement.ron"))
                .expect("shipped placement catalog must parse");
        placements
            .validate(super::test_items(), super::test_enemies())
            .expect("shipped placements must validate");
        placements
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        GameState,
        combat::{Health, Hit},
        content::tests::{variant_enemies, variant_items, variant_placements},
        enemy::tests::test_app_builder,
        experience::{ExperienceNotice, ExperiencePlugin},
        interaction::CorpseLoot,
        player::Player,
        quest::test_journal,
        roach,
    };

    #[test]
    fn ron_enemy_spawns_attacks_and_awards_its_configured_loot_and_experience() {
        let mut app = test_app_builder();
        let items = variant_items();
        let enemies = variant_enemies(&items);
        let placements = variant_placements(&items, &enemies);
        let id = placements
            .encounter_ids()
            .next()
            .expect("authored roach encounter");
        let loot = Pickup {
            item: ItemKind::parse("relay_pike").expect("authored item"),
            quantity: 2,
        };
        app.insert_resource(placements)
            .insert_resource(enemies)
            .insert_resource(EnemyVisuals::test_stub(&roach::MODEL))
            .insert_resource(test_journal())
            .add_observer(spawn_encounters)
            .add_plugins(ExperiencePlugin);
        app.finish();
        app.cleanup();
        let world = app.world_mut();
        world.trigger(SpawnWorld);
        world.flush();

        let (actor, home) = world
            .query::<(Entity, &EncounterId, &Transform)>()
            .iter(world)
            .find_map(|(entity, key, transform)| {
                (*key == id).then_some((entity, transform.translation))
            })
            .expect("the placement observer spawns the authored roach");
        assert_eq!(
            world.get::<Health>(actor).expect("roach health").maximum(),
            51
        );
        let player = world
            .spawn((
                Player::default(),
                Transform::from_translation(home + Vec3::Z * 1.2),
            ))
            .id();
        let initial = world
            .get::<Health>(player)
            .expect("player health")
            .current();
        world
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        for _ in 0..36 {
            app.update();
        }
        assert_eq!(
            app.world()
                .get::<Health>(player)
                .expect("player health")
                .current(),
            initial - 18
        );

        app.world_mut().write_message(Hit {
            source: player,
            target: actor,
            damage: 51,
            contact: None,
        });
        app.update();
        let world = app.world();
        assert_eq!(world.get::<Health>(actor).expect("dead roach").current(), 0);
        assert_eq!(
            world.get::<CorpseLoot>(actor).expect("corpse loot").stack,
            Some(loot)
        );
        assert!(
            world
                .resource::<ExperienceNotice>()
                .text
                .as_deref()
                .is_some_and(|notice| notice.starts_with("+82 XP"))
        );
    }
}
