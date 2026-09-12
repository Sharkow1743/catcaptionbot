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
    /// Generates a new time-ordered ShortId using UUIDv7
    pub fn now_v7() -> Self {
        let (time_high, _, _, _) = Uuid::now_v7().as_fields();
        Self(time_high)
    }

    /// Generates a purely random ShortId using UUIDv4
    pub fn new_v4() -> Self {
        let (rand_part, _, _, _) = Uuid::new_v4().as_fields();
        Self(rand_part)
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

// 1. Format as 8 hex characters: format!("{}", id) -> "0191bc34"
impl fmt::Display for ShortId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:08x}", self.0)
    }
}

// 2. Parse from 8 hex characters: "0191bc34".parse::<ShortId>()
impl FromStr for ShortId {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let val = u32::from_str_radix(s, 16)?;
        Ok(Self(val))
    }
}

// 3. Conversions from / into u32
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
