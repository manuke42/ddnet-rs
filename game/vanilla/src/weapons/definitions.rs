pub mod weapon_def {
    use game_interface::types::game::GameTickCooldown;
    use hiarc::Hiarc;
    use pool::{datatypes::PoolFxHashSet, pool::Pool};
    use rustc_hash::FxHashSet;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Hiarc, Copy, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
    pub enum WeaponUpgrade {
        Jetpack,
        Teleport,
    }

    pub type WeaponUpgradePool = Pool<FxHashSet<WeaponUpgrade>>;

    #[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
    pub struct Weapon {
        pub next_ammo_regeneration_tick: GameTickCooldown,
        /// A value of `None` here means unlimited ammo
        pub cur_ammo: Option<u32>,
        pub upgrades: PoolFxHashSet<WeaponUpgrade>,
    }
}
