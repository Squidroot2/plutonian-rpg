//! Player progression built on the shared actor level type.
use std::collections::BTreeMap;

use bevy::prelude::*;
use strum::VariantArray;

use crate::{
    attributes::{AttributeId, AttributeRanks, DEFAULT_RANK, MAX_RANK},
    level::Level,
};

const POINTS_PER_LEVEL: u32 = 3;

/// Player progression state that is saved, restored, and advanced together.
///
/// Level gains and their unspent attribute points share an owner and lifecycle,
/// so keeping them together avoids coordinating separate components.
#[derive(
    Component,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    bitcode::Encode,
    bitcode::Decode,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[schemars(deny_unknown_fields)]
pub(crate) struct CharacterProgress {
    pub(crate) level: Level,
    pub(crate) unspent: u32,
}

impl Default for CharacterProgress {
    fn default() -> Self {
        Self {
            level: Level::FIRST,
            unspent: 0,
        }
    }
}

impl CharacterProgress {
    /// Record an absolute level and grant points for each newly reached level.
    pub(crate) fn advance_to(&mut self, level: Level) -> u32 {
        if level <= self.level || !level.is_valid() {
            return 0;
        }
        let previous_points = self.unspent;
        let points = (level.get() - self.level.get()).saturating_mul(POINTS_PER_LEVEL);
        self.level = level;
        self.unspent = self.unspent.saturating_add(points);
        self.unspent - previous_points
    }
}

#[derive(
    Clone,
    Default,
    bitcode::Encode,
    bitcode::Decode,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[schemars(deny_unknown_fields)]
pub(crate) struct SavedProgress {
    ranks: BTreeMap<String, u8>,
    progress: CharacterProgress,
}

impl SavedProgress {
    pub(crate) fn level(&self) -> Level {
        self.progress.level
    }

    pub(crate) fn capture(ranks: &AttributeRanks, progress: CharacterProgress) -> Self {
        Self {
            ranks: AttributeId::VARIANTS
                .iter()
                .map(|id| (id.key().into(), ranks.rank(*id)))
                .collect(),
            progress,
        }
    }

    pub(crate) fn ranks(&self) -> AttributeRanks {
        let mut rank_array = [0; AttributeId::VARIANTS.len()];
        for (i, rank) in rank_array.iter_mut().enumerate() {
            let attr = AttributeId::VARIANTS
                .get(i)
                .expect("Index is always within variant bounds");
            *rank = self.ranks.get(attr.key()).copied().unwrap_or(DEFAULT_RANK);
        }
        AttributeRanks::from_array(rank_array)
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.progress.level.is_valid()
            && self
                .ranks
                .values()
                .all(|rank| (1..=MAX_RANK).contains(rank))
    }

    /// Construct all player capabilities before the first gameplay tick.
    pub(crate) fn player_components(&self) -> impl Bundle {
        (self.ranks().actor_components(), self.progress)
    }
}
