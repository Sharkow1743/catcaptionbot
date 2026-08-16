use crate::text::WordSpec;

use super::Rarity;

pub struct Common;
impl Rarity for Common {
    fn name(&self) -> String {
        "Common".to_string()
    }
    fn template(&self) -> impl AsRef<[WordSpec]> {
        vec![WordSpec::new()]
    }
}
