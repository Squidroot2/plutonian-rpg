//! Authored export requirements without a gameplay counterpart.

use crate::{
    content::ItemKind,
    crossing::FLATS_MODEL,
    loading::{OUTPOST_MODEL, RELAY_MODEL, SOUTH_BASIN_MODEL},
};

pub(super) const DURATION_TOLERANCE_S: f32 = 0.001;
pub(super) const POSITION_TOLERANCE_M: f32 = 0.001;
pub(super) const ACTOR_DEATH_DURATION_S: f32 = 1.0;

pub(super) struct AreaRequirements {
    pub visual: &'static str,
    pub bodies: &'static [&'static str],
    pub homes: &'static [Home],
}

pub(super) struct Home {
    pub name: &'static str,
    pub beds: usize,
}

pub(super) const VILLAGE_PREFIX: &str = "Village solid ";
pub(super) const SOLID_NAME_MARKER: &str = " solid ";
pub(super) const WAYSTONE_PREFIX: &str = "Waystone";
pub(super) const WAYSTONE_LAMP_PREFIX: &str = "Waystone lamp";
pub(super) const RETIRED_OUTPOST_PREFIXES: &[&str] = &["Habitat ", "Lit doorway"];

pub(super) const AREAS: &[AreaRequirements] = &[
    AreaRequirements {
        visual: OUTPOST_MODEL,
        bodies: &[
            "Perimeter solid Shelf cliffs",
            "Perimeter solid Basin west cliffs",
            "Perimeter solid Cavern headwall",
            "Perimeter solid Gate lift housing",
            "Perimeter solid Gate header",
            "Perimeter solid Gate east buttress",
            "Perimeter solid Gate west buttress",
            "Perimeter solid Gate east infill",
            "Perimeter solid Gate west infill",
            "Perimeter solid Gate east jamb",
            "Perimeter solid Gate west jamb",
        ],
        homes: &[
            Home {
                name: "Family",
                beds: 2,
            },
            Home {
                name: "Workshop",
                beds: 1,
            },
            Home {
                name: "Orla",
                beds: 1,
            },
            Home {
                name: "Keeper",
                beds: 1,
            },
            Home {
                name: "Barracks",
                beds: 2,
            },
        ],
    },
    AreaRequirements {
        visual: FLATS_MODEL,
        bodies: &[
            "Flats solid Floor",
            "Flats solid Charon overlook",
            "Perimeter solid Listening Flats cliffs",
        ],
        homes: &[],
    },
    AreaRequirements {
        visual: RELAY_MODEL,
        bodies: &[
            "Archive service console",
            "Vestibule floor",
            "Vestibule ceiling",
            "Junction floor",
            "Junction ceiling",
            "Maintenance floor",
            "Maintenance ceiling",
            "Gate passage floor",
            "Gate passage ceiling",
            "Archive floor",
            "Archive ceiling",
        ],
        homes: &[],
    },
    AreaRequirements {
        visual: SOUTH_BASIN_MODEL,
        bodies: &[
            "South Basin shelf",
            "South Basin blind ridge",
            "Hushcap Caverns shell",
            "Entrance cover",
            "First chamber buttress",
            "Split chamber column",
            "West passage cover",
            "Gallery screen",
            "Gallery low cover",
            "Gallery vault pillar",
            "South Basin ice fin",
            "South Basin ice fin.001",
            "South Basin ice fin.002",
            "South Basin ice fin.003",
            "South Basin ice fin.004",
            "South Basin ice fin.005",
            "South Basin ice fin.006",
            "South Basin ice fin.007",
        ],
        homes: &[],
    },
];

pub(super) const HOME_BODY_SUFFIXES: &[&str] = &[
    "deck",
    "left jamb",
    "right jamb",
    "lintel",
    "threshold ramp",
    "crown",
];

pub(super) const GATE_LEAF: &str = "Nivalis gate leaf";
pub(super) const GATE_CHILDREN: usize = 7;
/// Gate trim protrudes behind the runtime cuboid.
pub(super) const GATE_BACK_Z_M: f32 = -0.74;
pub(super) const VELLIK_LEG_PREFIX: &str = "Vellik Leg ";
pub(super) const WAGON_WHEELS: usize = 4;

pub(super) const LOOSE_PICKUP_CENTER_TOLERANCES: &[(ItemKind, f32)] = &[
    (ItemKind::WormMeat, POSITION_TOLERANCE_M),
    (ItemKind::RoachMeat, POSITION_TOLERANCE_M),
    (ItemKind::Mycelium, 0.08),
];

pub(super) const LEGS: [&str; 2] = ["leg-1", "leg1"];
pub(super) const SPEAR_HANDS: [SpearHand; 2] = [
    SpearHand {
        wrap: "Rear hand wrap",
        rest_center: [-0.42, -0.05, 0.02],
    },
    SpearHand {
        wrap: "Forward hand wrap",
        rest_center: [0.42, -0.05, 0.02],
    },
];
pub(super) struct SpearHand {
    pub wrap: &'static str,
    pub rest_center: [f32; 3],
}

pub(super) const SWORD_HILT_WRAP: &str = "Hand wrap";
pub(super) const GRIP_HEIGHT_M: std::ops::RangeInclusive<f32> = 0.70..=0.95;
pub(super) const SHAFT_SIDE_LIMIT_M: f32 = -0.2;
pub(super) const SHAFT_OUTWARD_LIMIT: f32 = -0.2;
pub(super) const LEAD_SHOE_REST_M: [f32; 3] = [0.15, 0.10, 0.09];
pub(super) const LEAD_SHOE_ADVANCE_M: f32 = 0.22;
pub(super) const LEAD_SHOE_VERTICAL_TOLERANCE_M: f32 = 0.06;
pub(super) const ELBOW_FRONT_CLEARANCE_M: f32 = 0.38;
pub(super) const ELBOW_SIDE_CLEARANCE_M: f32 = 0.42;
pub(super) const HAND_WRAP_TOLERANCE_M: f32 = 0.015;
pub(super) const SPEAR_WINDUP_FRAME: u32 = 3;
pub(super) const SPEAR_WINDUP_TRAVEL_M: f32 = 0.25;
pub(super) const SPEAR_IMPACT_TRAVEL_M: f32 = 0.45;
pub(super) const SWORD_CHAMBER_FRAME: u32 = 2;
pub(super) const SWORD_IMPACT_TRAVEL_M: f32 = 0.4;
