/// A level constrained to `1..=MAX_LEVEL`.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    bitcode::Encode,
    bitcode::Decode,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
pub(crate) struct Level(u32);
pub(crate) const MAX_LEVEL: u32 = 100;
impl Level {
    pub(crate) const FIRST: Self = Self(1);
    pub(crate) const MAX: Self = Self(MAX_LEVEL);
    pub(crate) const fn new(value: u32) -> Option<Self> {
        if value >= 1 && value <= MAX_LEVEL {
            Some(Self(value))
        } else {
            None
        }
    }
    pub(crate) const fn get(self) -> u32 {
        self.0
    }
    pub(crate) const fn is_valid(self) -> bool {
        self.0 >= 1 && self.0 <= MAX_LEVEL
    }
    pub(crate) fn advance(&mut self) -> bool {
        let Some(next) = Self::new(self.0.saturating_add(1)) else {
            return false;
        };
        *self = next;
        true
    }
}
