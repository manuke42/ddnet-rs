use hiarc::Hiarc;
use serde::{Deserialize, Serialize};

use super::weapons::WeaponType;

#[derive(Debug, Hiarc, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PickupType {
    PowerupHealth,
    PowerupArmor,
    PowerupNinja,
    PowerupWeapon(WeaponType),
    PowerupWeaponShield(WeaponType),
    PowerupNinjaShield,
}
