pub mod game_objects {
    use game_interface::types::{emoticons::EnumCount, weapons::WeaponType};
    use hiarc::Hiarc;
    use legacy_map::mapdef_06::{DdraceEntityTiles, EntityTiles};
    use map::map::groups::layers::tiles::{TileBase, TileFlags};
    use math::math::vector::ivec2;

    #[derive(Debug, Hiarc, Default)]
    pub struct GameObjectsPickupDefinitions<V> {
        pub hearts: Vec<V>,
        pub shields: Vec<V>,

        pub red_flags: Vec<V>,
        pub blue_flags: Vec<V>,

        pub weapons: [Vec<V>; WeaponType::COUNT],

        pub ninjas: Vec<V>,
    }

    #[derive(Debug, Hiarc, Clone, Copy, PartialEq, Eq)]
    pub enum DdraceMapEntityKind {
        LaserFastCcw,
        LaserNormalCcw,
        LaserSlowCcw,
        LaserStop,
        LaserSlowCw,
        LaserNormalCw,
        LaserFastCw,
        LaserShort,
        LaserMedium,
        LaserLong,
        LaserCSlow,
        LaserCNormal,
        LaserCFast,
        LaserOSlow,
        LaserONormal,
        LaserOFast,
        PlasmaExplosive,
        PlasmaFreeze,
        PlasmaFreezeExplosive,
        Plasma,
        CrazyShotgunExplosive,
        CrazyShotgun,
        DraggerWeak,
        DraggerNormal,
        DraggerStrong,
        DraggerWeakNoWalls,
        DraggerNormalNoWalls,
        DraggerStrongNoWalls,
        Door,
    }

    impl DdraceMapEntityKind {
        fn from_tile_index(index: u8) -> Option<Self> {
            Some(match index {
                index if index == DdraceEntityTiles::LaserFastCcw as u8 => Self::LaserFastCcw,
                index if index == DdraceEntityTiles::LaserNormalCcw as u8 => Self::LaserNormalCcw,
                index if index == DdraceEntityTiles::LaserSlowCcw as u8 => Self::LaserSlowCcw,
                index if index == DdraceEntityTiles::LaserStop as u8 => Self::LaserStop,
                index if index == DdraceEntityTiles::LaserSlowCw as u8 => Self::LaserSlowCw,
                index if index == DdraceEntityTiles::LaserNormalCw as u8 => Self::LaserNormalCw,
                index if index == DdraceEntityTiles::LaserFastCw as u8 => Self::LaserFastCw,
                index if index == DdraceEntityTiles::LaserShort as u8 => Self::LaserShort,
                index if index == DdraceEntityTiles::LaserMedium as u8 => Self::LaserMedium,
                index if index == DdraceEntityTiles::LaserLong as u8 => Self::LaserLong,
                index if index == DdraceEntityTiles::LaserCSlow as u8 => Self::LaserCSlow,
                index if index == DdraceEntityTiles::LaserCNormal as u8 => Self::LaserCNormal,
                index if index == DdraceEntityTiles::LaserCFast as u8 => Self::LaserCFast,
                index if index == DdraceEntityTiles::LaserOSlow as u8 => Self::LaserOSlow,
                index if index == DdraceEntityTiles::LaserONormal as u8 => Self::LaserONormal,
                index if index == DdraceEntityTiles::LaserOFast as u8 => Self::LaserOFast,
                index if index == DdraceEntityTiles::PlasmaE as u8 => Self::PlasmaExplosive,
                index if index == DdraceEntityTiles::PlasmaF as u8 => Self::PlasmaFreeze,
                index if index == DdraceEntityTiles::Plasma as u8 => Self::PlasmaFreezeExplosive,
                index if index == DdraceEntityTiles::PlasmaU as u8 => Self::Plasma,
                index if index == DdraceEntityTiles::CrazyShotgunEx as u8 => {
                    Self::CrazyShotgunExplosive
                }
                index if index == DdraceEntityTiles::CrazyShotgun as u8 => Self::CrazyShotgun,
                index if index == DdraceEntityTiles::DraggerWeak as u8 => Self::DraggerWeak,
                index if index == DdraceEntityTiles::DraggerNormal as u8 => Self::DraggerNormal,
                index if index == DdraceEntityTiles::DraggerStrong as u8 => Self::DraggerStrong,
                index if index == DdraceEntityTiles::DraggerWeakNw as u8 => {
                    Self::DraggerWeakNoWalls
                }
                index if index == DdraceEntityTiles::DraggerNormalNw as u8 => {
                    Self::DraggerNormalNoWalls
                }
                index if index == DdraceEntityTiles::DraggerStrongNw as u8 => {
                    Self::DraggerStrongNoWalls
                }
                index if index == DdraceEntityTiles::Door as u8 => Self::Door,
                _ => return None,
            })
        }
    }

    #[derive(Debug, Hiarc, Clone, Copy)]
    pub struct DdraceMapEntityDefinition<V> {
        pub pos: V,
        pub kind: DdraceMapEntityKind,
        pub flags: TileFlags,
        pub number: Option<u8>,
    }

    /// definitions of game objects, like their spawn position or flags etc.
    #[derive(Debug, Hiarc)]
    pub struct GameObjectDefinitionsBase<V> {
        pub pickups: GameObjectsPickupDefinitions<V>,
        pub ddrace_entities: Vec<DdraceMapEntityDefinition<V>>,
    }

    impl GameObjectDefinitionsBase<ivec2> {
        pub fn new(game_layer_tiles: &[TileBase], width: u32, height: u32) -> Self {
            let mut pickups = GameObjectsPickupDefinitions::<ivec2>::default();
            let mut ddrace_entities = Vec::new();

            for y in 0..height {
                for x in 0..width {
                    let tiles = game_layer_tiles;
                    let index = (y * width + x) as usize;
                    if let Some(kind) = DdraceMapEntityKind::from_tile_index(tiles[index].index) {
                        ddrace_entities.push(DdraceMapEntityDefinition {
                            pos: ivec2::new(x as i32, y as i32),
                            kind,
                            flags: tiles[index].flags,
                            number: None,
                        });
                    }
                    match tiles[index].index {
                        i if i == EntityTiles::Health as u8 => {
                            pickups.hearts.push(ivec2::new(x as i32, y as i32));
                        }
                        i if i == EntityTiles::Armor as u8 => {
                            pickups.shields.push(ivec2::new(x as i32, y as i32));
                        }
                        i if i == EntityTiles::FlagSpawnRed as u8 => {
                            pickups.red_flags.push(ivec2::new(x as i32, y as i32));
                        }
                        i if i == EntityTiles::FlagSpawnBlue as u8 => {
                            pickups.blue_flags.push(ivec2::new(x as i32, y as i32));
                            // TODO remove all as i32, use u16 instead
                        }
                        i if i == EntityTiles::WeaponGrenade as u8 => {
                            pickups.weapons[WeaponType::Grenade as usize]
                                .push(ivec2::new(x as i32, y as i32));
                        }
                        i if i == EntityTiles::WeaponLaser as u8 => {
                            pickups.weapons[WeaponType::Laser as usize]
                                .push(ivec2::new(x as i32, y as i32));
                        }
                        i if i == EntityTiles::WeaponShotgun as u8 => {
                            pickups.weapons[WeaponType::Shotgun as usize]
                                .push(ivec2::new(x as i32, y as i32));
                        }
                        i if i == EntityTiles::PowerupNinja as u8 => {
                            pickups.ninjas.push(ivec2::new(x as i32, y as i32));
                        }
                        _ => {
                            // not handled
                        }
                    }
                }
            }
            Self {
                pickups,
                ddrace_entities,
            }
        }
    }

    pub type GameObjectDefinitions = GameObjectDefinitionsBase<ivec2>;
}
