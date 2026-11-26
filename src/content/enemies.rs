use super::{Catalog, ContentId, Items, positive};
use crate::{
    enemy::{Bite, EnemyBody, GroundMelee, GroundMovement, ranged::Spit},
    inventory::Pickup,
    level::{Level, MAX_LEVEL},
};
use bevy::prelude::*;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EnemyArchetype {
    Worm,
    Roach,
    Hushcap,
    Guard,
}

impl EnemyArchetype {
    const ALL: [Self; 4] = [Self::Worm, Self::Roach, Self::Hushcap, Self::Guard];

    pub(crate) const fn id(self) -> ContentId {
        ContentId::new(match self {
            Self::Worm => "ice_worm",
            Self::Roach => "relay_roach",
            Self::Hushcap => "hushcap",
            Self::Guard => "outpost_guard",
        })
    }
}

#[derive(Asset, TypePath, Resource, Clone)]
pub(crate) struct Enemies(BTreeMap<ContentId, EnemyDefinition>);

impl<'de> Deserialize<'de> for Enemies {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        let entries = Vec::<EnemyDefinition>::deserialize(deserializer)?;
        let mut definitions = BTreeMap::new();
        for entry in entries {
            let id = entry.id;
            if definitions.insert(id, entry).is_some() {
                return Err(D::Error::custom(format!("Duplicate enemy ID {id}")));
            }
        }
        Ok(Self(definitions))
    }
}

impl Catalog for Enemies {
    const PATH: &'static str = "data://enemies/catalog.enemy.ron";
    const EXTENSION: &'static str = "enemy.ron";
}

impl Enemies {
    pub(crate) fn get(&self, id: ContentId) -> Result<&EnemyDefinition, String> {
        self.0
            .get(&id)
            .ok_or_else(|| format!("Missing enemy definition {id}"))
    }

    pub(crate) fn validate(&self, items: &Items) -> Result<(), String> {
        for archetype in EnemyArchetype::ALL {
            let id = archetype.id();
            let entry = self
                .get(id)
                .map_err(|error| format!("{}: {error}", Self::PATH))?;
            if entry.archetype() != archetype {
                return Err(format!("{} entry {id} must use {archetype:?}", Self::PATH));
            }
        }
        for (&id, entry) in &self.0 {
            entry
                .validate(items)
                .map_err(|error| format!("{} entry {id}: {error}", Self::PATH))?;
        }
        Ok(())
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EnemyDefinition {
    id: ContentId,
    #[serde(deserialize_with = "Level::deserialize_number")]
    level: Level,
    name: String,
    #[serde(default, deserialize_with = "present")]
    health: Option<u16>,
    xp: u16,
    #[serde(default, deserialize_with = "present")]
    loot: Option<Pickup>,
    behavior: Behavior,
}

fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
enum Behavior {
    Worm { speed_mps: f32, bite: Bite },
    Roach { speed_mps: f32, bite: Bite },
    Hushcap { spit: Spit },
    Guard { speed_mps: f32 },
}

impl EnemyDefinition {
    #[cfg(test)]
    pub(super) fn level(&self) -> Level {
        self.level
    }

    pub(crate) fn archetype(&self) -> EnemyArchetype {
        match self.behavior {
            Behavior::Worm { .. } => EnemyArchetype::Worm,
            Behavior::Roach { .. } => EnemyArchetype::Roach,
            Behavior::Hushcap { .. } => EnemyArchetype::Hushcap,
            Behavior::Guard { .. } => EnemyArchetype::Guard,
        }
    }

    pub(super) fn loot(&self) -> Pickup {
        self.loot.expect("creature loot must be validated at boot")
    }

    pub(crate) fn body(&self) -> EnemyBody {
        EnemyBody {
            name: self.name.clone(),
            level: self.level,
            health: self
                .health
                .expect("creature health must be validated at boot"),
            experience: self.xp,
            loot: self.loot(),
        }
    }

    pub(crate) fn melee(&self, ground_offset: f32) -> GroundMelee {
        let (speed, bite) = match self.behavior {
            Behavior::Worm { speed_mps, bite } | Behavior::Roach { speed_mps, bite } => {
                (speed_mps, bite)
            }
            _ => unreachable!("only biting creatures use this attack configuration"),
        };
        GroundMelee {
            body: self.body(),
            movement: GroundMovement::new(speed, ground_offset),
            bite,
        }
    }

    pub(crate) fn guard_role(&self) -> (&str, f32, u16) {
        match self.behavior {
            Behavior::Guard { speed_mps } => (&self.name, speed_mps, self.xp),
            _ => unreachable!("the placement must assign a validated guard role"),
        }
    }

    pub(crate) fn spit(&self) -> Spit {
        match self.behavior {
            Behavior::Hushcap { spit } => spit,
            _ => unreachable!("only Hushcaps use spit attacks"),
        }
    }

    fn validate(&self, items: &Items) -> Result<(), String> {
        if !self.level.is_valid() {
            return Err(format!("Enemy level must be in 1..={MAX_LEVEL}"));
        }
        if self.name.trim().is_empty() || self.xp == 0 {
            return Err("Name and positive XP are required".into());
        }
        if self.archetype() == EnemyArchetype::Guard {
            if self.health.is_some() || self.loot.is_some() {
                return Err(
                    "Guard health and loot come from the NPC's attributes and loadout".into(),
                );
            }
        } else {
            if self.health.is_none_or(|health| health == 0) {
                return Err("Creatures need positive health".into());
            }
            items.validate_stack(self.loot.ok_or("Creatures need a loot stack")?)?;
        }
        let valid = match self.behavior {
            Behavior::Worm { speed_mps, bite } | Behavior::Roach { speed_mps, bite } => {
                positive(speed_mps) && bite.is_valid()
            }
            Behavior::Guard { speed_mps } => positive(speed_mps),
            Behavior::Hushcap { spit } => spit.is_valid(),
        };
        valid
            .then_some(())
            .ok_or_else(|| "Invalid movement or attack tuning".into())
    }
}

#[cfg(test)]
pub(crate) fn test_enemies() -> &'static Enemies {
    static ENEMIES: std::sync::OnceLock<Enemies> = std::sync::OnceLock::new();
    ENEMIES.get_or_init(|| {
        let enemies: Enemies = ron::from_str(include_str!("../../data/enemies/catalog.enemy.ron"))
            .expect("shipped enemy catalog must parse");
        enemies
            .validate(super::test_items())
            .expect("shipped enemies must validate");
        enemies
    })
}
