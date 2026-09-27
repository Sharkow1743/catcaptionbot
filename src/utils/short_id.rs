use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct ShortId(u32);

impl ShortId {
    /// Generates a unique, collision-resistant ShortId combining UUIDv7 timestamp and entropy
    pub fn now_v7() -> Self {
        let u = Uuid::now_v7();
        let (d1, d2, d3, d4) = u.as_fields();
        let rand_u32 = u32::from_be_bytes([d4[0], d4[1], d4[2], d4[3]]);
        let val = d1 ^ ((d2 as u32) << 16 | (d3 as u32)) ^ rand_u32;
        Self(if val == 0 { 1 } else { val })
    }

    /// Generates a purely random ShortId using UUIDv4
    pub fn new_v4() -> Self {
        let (rand_part, _, _, _) = Uuid::new_v4().as_fields();
        Self(if rand_part == 0 { 1 } else { rand_part })
    }

    /// Access the raw underlying u32
    pub const fn as_u32(&self) -> u32 {
        self.0
    }

    /// Access raw 4 bytes
    pub const fn to_bytes(&self) -> [u8; 4] {
        self.0.to_be_bytes()
    }
}

impl fmt::Display for ShortId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:08x}", self.0)
    }
}

impl FromStr for ShortId {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let val = u32::from_str_radix(s, 16)?;
        Ok(Self(val))
    }
}

impl From<u32> for ShortId {
    fn from(val: u32) -> Self {
        Self(val)
    }
}

impl From<ShortId> for u32 {
    fn from(id: ShortId) -> Self {
        id.0
    }
}
