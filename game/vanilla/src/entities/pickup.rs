pub mod pickup {
    use crate::{
        entities::character::character::{BuffProps, Character, CharacterPool, WeaponsExt},
        reusable::{CloneWithCopyableElements, ReusableCore},
    };
    use base::linked_hash_map_view::FxLinkedHashMap;
    use game_interface::{
        events::{
            GameBuffNinjaEventSound, GameBuffSoundEvent, GameCharacterSoundEvent,
            GameGrenadeEventSound, GameLaserEventSound, GamePickupArmorEventSound,
            GamePickupHeartEventSound, GamePickupSoundEvent, GamePullerEventSound,
            GameShotgunEventSound, GameWorldEntitySoundEvent,
        },
        types::{
            id_types::PickupId, pickup::PickupType, render::character::CharacterBuff,
            render::character::CharacterDebuff, weapons::WeaponType,
        },
    };
    use hiarc::Hiarc;
    use math::math::{lerp, vector::vec2};
    use pool::{datatypes::PoolFxLinkedHashMap, pool::Pool, recycle::Recycle, traits::Recyclable};
    use serde::{Deserialize, Serialize};

    use crate::state::state::TICKS_PER_SECOND;
    use crate::{
        config::config::ConfigGameType,
        entities::entity::entity::{DropMode, Entity, EntityInterface, EntityTickResult},
        events::events::PickupEvent,
        simulation_pipe::simulation_pipe::{
            GameWorldPendingEvents, SimulationEventWorldEntityType, SimulationPipePickup,
            SimulationWorldEvents,
        },
        weapons::definitions::weapon_def::Weapon,
        world::world::GameWorld,
    };

    #[derive(Debug, Hiarc, Default, Serialize, Deserialize)]
    pub struct PickupReusableCore {}

    impl Recyclable for PickupReusableCore {
        fn new() -> Self {
            Self {}
        }

        fn reset(&mut self) {}
    }

    impl CloneWithCopyableElements for PickupReusableCore {
        fn copy_clone_from(&mut self, _other: &Self) {}
    }

    impl ReusableCore for PickupReusableCore {}

    pub type PoolPickupReusableCore = Recycle<PickupReusableCore>;

    #[derive(Debug, Hiarc, Copy, Clone, Serialize, Deserialize)]
    pub struct PickupCore {
        pub pos: vec2,
        pub ty: PickupType,
    }

    #[derive(Debug, Hiarc)]
    pub struct Pickup {
        pub(crate) base: Entity<PickupId>,
        pub(crate) core: PickupCore,
        pub(crate) reusable_core: PoolPickupReusableCore,

        game_pending_events: GameWorldPendingEvents,
        simulation_events: SimulationWorldEvents,
    }

    impl Pickup {
        pub fn new(
            game_el_id: &PickupId,
            pos: &vec2,
            ty: PickupType,
            pool: &PickupPool,
            game_pending_events: &GameWorldPendingEvents,
            simulation_events: &SimulationWorldEvents,
        ) -> Self {
            let spawn_effect = || {
                game_pending_events.push_sound(
                    None,
                    Some(*pos),
                    match ty {
                        PickupType::PowerupHealth => return,
                        PickupType::PowerupArmor => return,
                        PickupType::PowerupNinja => return,
                        PickupType::PowerupWeaponShield(_) => return,
                        PickupType::PowerupNinjaShield => return,
                        PickupType::PowerupWeapon(weapon_type) => match weapon_type {
                            WeaponType::Hammer => return,
                            WeaponType::Gun => return,
                            WeaponType::Shotgun => {
                                GameWorldEntitySoundEvent::Shotgun(GameShotgunEventSound::Spawn)
                            }
                            WeaponType::Puller => {
                                GameWorldEntitySoundEvent::Puller(GamePullerEventSound::Spawn)
                            }
                            WeaponType::Grenade => {
                                GameWorldEntitySoundEvent::Grenade(GameGrenadeEventSound::Spawn)
                            }
                            WeaponType::Laser => {
                                GameWorldEntitySoundEvent::Laser(GameLaserEventSound::Spawn)
                            }
                        },
                    },
                )
            };
            spawn_effect();

            Self {
                base: Entity::new(game_el_id),
                core: PickupCore { pos: *pos, ty },
                reusable_core: pool.pickup_reusable_cores_pool.new(),

                game_pending_events: game_pending_events.clone(),
                simulation_events: simulation_events.clone(),
            }
        }

        pub fn lerped_pos(pickup1: &Pickup, pickup2: &Pickup, ratio: f64) -> vec2 {
            lerp(&pickup1.core.pos, &pickup2.core.pos, ratio as f32)
        }

        fn collected_result(
            collected: bool,
            pickup_tick_result: EntityTickResult,
        ) -> EntityTickResult {
            if collected {
                pickup_tick_result
            } else {
                EntityTickResult::None
            }
        }

        fn emit_pickup(&self, char: &Character, ty: PickupType, sound: GameWorldEntitySoundEvent) {
            self.game_pending_events.push_sound(
                Some(char.base.game_element_id),
                Some(self.core.pos),
                sound,
            );
            self.simulation_events
                .push_world(SimulationEventWorldEntityType::Pickup {
                    id: self.base.game_element_id,
                    ev: PickupEvent::Pickup {
                        pos: self.core.pos,
                        by: char.base.game_element_id,
                        ty,
                    },
                });
        }

        fn emit_heart_pickup(&self, char: &Character) {
            self.emit_pickup(
                char,
                PickupType::PowerupHealth,
                GameWorldEntitySoundEvent::Pickup(GamePickupSoundEvent::Heart(
                    GamePickupHeartEventSound::Collect,
                )),
            );
        }

        fn emit_armor_pickup(&self, char: &Character, ty: PickupType) {
            self.emit_pickup(
                char,
                ty,
                GameWorldEntitySoundEvent::Pickup(GamePickupSoundEvent::Armor(
                    GamePickupArmorEventSound::Collect,
                )),
            );
        }

        fn pickup_health(
            &self,
            char: &mut Character,
            is_race: bool,
            pickup_tick_result: EntityTickResult,
        ) -> EntityTickResult {
            if is_race {
                let freeze = char
                    .reusable_core
                    .debuffs
                    .entry(CharacterDebuff::Freeze)
                    .or_insert_with_keep_order(|| BuffProps {
                        remaining_tick: 0.into(),
                        interact_tick: 0.into(),
                        interact_cursor_dir: Default::default(),
                        interact_val: 0.0,
                    });
                if freeze.interact_tick.is_none() {
                    freeze.remaining_tick = (3 * TICKS_PER_SECOND).into();
                    freeze.interact_tick = TICKS_PER_SECOND.into();
                    self.emit_heart_pickup(char);
                    pickup_tick_result
                } else {
                    EntityTickResult::None
                }
            } else if char.core.health < 10 {
                char.core.health += 1;
                self.emit_heart_pickup(char);
                pickup_tick_result
            } else {
                EntityTickResult::None
            }
        }

        fn pickup_armor(
            &self,
            char: &mut Character,
            is_race: bool,
            pickup_tick_result: EntityTickResult,
        ) -> EntityTickResult {
            if is_race {
                let mut collected = false;
                for weapon in [
                    WeaponType::Shotgun,
                    WeaponType::Puller,
                    WeaponType::Grenade,
                    WeaponType::Laser,
                ] {
                    collected |= char.reusable_core.weapons.remove(&weapon).is_some();
                }
                collected |= char
                    .reusable_core
                    .buffs
                    .remove(&CharacterBuff::Ninja)
                    .is_some();

                if char.core.active_weapon >= WeaponType::Shotgun {
                    char.core.prev_weapon = WeaponType::Gun;
                    char.core.active_weapon = WeaponType::Hammer;
                    char.core.queued_weapon = None;
                }

                if collected {
                    self.emit_armor_pickup(char, PickupType::PowerupArmor);
                }
                EntityTickResult::None
            } else if char.core.armor < 10 {
                char.core.armor += 1;
                self.emit_armor_pickup(char, PickupType::PowerupArmor);
                pickup_tick_result
            } else {
                EntityTickResult::None
            }
        }

        fn pickup_weapon(
            &self,
            char: &mut Character,
            char_pool: &CharacterPool,
            weapon: WeaponType,
            is_race: bool,
            pickup_tick_result: EntityTickResult,
        ) -> EntityTickResult {
            let ammo = (!is_race).then_some(10);
            let collected = if let Some(weapon) = char.reusable_core.weapons.get_mut(&weapon) {
                if weapon.cur_ammo.is_some_and(|val| val < 10) {
                    weapon.cur_ammo = ammo;
                    true
                } else {
                    false
                }
            } else {
                char.reusable_core.weapons.insert_sorted(
                    weapon,
                    Weapon {
                        cur_ammo: ammo,
                        next_ammo_regeneration_tick: 0.into(),
                        upgrades: char_pool.character_weapon_upgrade_pool.new(),
                    },
                );
                true
            };

            if collected {
                if let Some(ev) = match weapon {
                    WeaponType::Hammer | WeaponType::Gun => None,
                    WeaponType::Shotgun => Some(GameWorldEntitySoundEvent::Shotgun(
                        GameShotgunEventSound::Collect,
                    )),
                    WeaponType::Puller => Some(GameWorldEntitySoundEvent::Puller(
                        GamePullerEventSound::Collect,
                    )),
                    WeaponType::Grenade => Some(GameWorldEntitySoundEvent::Grenade(
                        GameGrenadeEventSound::Collect,
                    )),
                    WeaponType::Laser => Some(GameWorldEntitySoundEvent::Laser(
                        GameLaserEventSound::Collect,
                    )),
                } {
                    self.game_pending_events.push_sound(
                        Some(char.base.game_element_id),
                        Some(self.core.pos),
                        ev,
                    );
                }
                self.simulation_events
                    .push_world(SimulationEventWorldEntityType::Pickup {
                        id: self.base.game_element_id,
                        ev: PickupEvent::Pickup {
                            pos: self.core.pos,
                            by: char.base.game_element_id,
                            ty: PickupType::PowerupWeapon(weapon),
                        },
                    });
            }

            Self::collected_result(collected, pickup_tick_result)
        }

        fn pickup_weapon_shield(
            &self,
            char: &mut Character,
            weapon: WeaponType,
            pickup_tick_result: EntityTickResult,
        ) -> EntityTickResult {
            let collected = char.reusable_core.weapons.remove(&weapon).is_some();
            if collected {
                if char.core.active_weapon == weapon {
                    char.core.prev_weapon = char.core.active_weapon;
                    char.core.active_weapon = WeaponType::Hammer;
                    char.core.queued_weapon = None;
                }
                self.emit_armor_pickup(char, PickupType::PowerupWeaponShield(weapon));
            }

            Self::collected_result(collected, pickup_tick_result)
        }

        fn pickup_ninja(
            &self,
            char: &mut Character,
            pickup_tick_result: EntityTickResult,
        ) -> EntityTickResult {
            self.emit_pickup(
                char,
                PickupType::PowerupNinja,
                GameWorldEntitySoundEvent::Character(GameCharacterSoundEvent::Buff(
                    GameBuffSoundEvent::Ninja(GameBuffNinjaEventSound::Collect),
                )),
            );
            char.give_ninja();
            pickup_tick_result
        }

        fn pickup_ninja_shield(
            &self,
            char: &mut Character,
            pickup_tick_result: EntityTickResult,
        ) -> EntityTickResult {
            let collected = char
                .reusable_core
                .buffs
                .remove(&CharacterBuff::Ninja)
                .is_some();

            if collected {
                self.emit_armor_pickup(char, PickupType::PowerupNinjaShield);
            }

            Self::collected_result(collected, pickup_tick_result)
        }
    }

    impl EntityInterface<PickupCore, PickupReusableCore, SimulationPipePickup<'_>> for Pickup {
        fn pre_tick(&mut self, _pipe: &mut SimulationPipePickup) -> EntityTickResult {
            todo!()
        }

        fn tick(&mut self, pipe: &mut SimulationPipePickup) -> EntityTickResult {
            let intersection = GameWorld::intersect_character(
                pipe.field,
                pipe.characters.characters_mut(),
                &self.core.pos,
                20,
            );

            if let Some(char) = intersection {
                let is_race = matches!(pipe.game_options.game_ty(), ConfigGameType::Race);
                let pickup_tick_result = if is_race {
                    EntityTickResult::None
                } else {
                    EntityTickResult::RemoveEntity
                };
                // player picked us up, is someone was hooking us, let them go
                // TODO: magic constants
                match self.core.ty {
                    PickupType::PowerupHealth => {
                        self.pickup_health(char, is_race, pickup_tick_result)
                    }
                    PickupType::PowerupArmor => {
                        self.pickup_armor(char, is_race, pickup_tick_result)
                    }
                    PickupType::PowerupWeapon(weapon) => self.pickup_weapon(
                        char,
                        pipe.char_pool,
                        weapon,
                        is_race,
                        pickup_tick_result,
                    ),
                    PickupType::PowerupWeaponShield(weapon) => {
                        self.pickup_weapon_shield(char, weapon, pickup_tick_result)
                    }
                    PickupType::PowerupNinja => self.pickup_ninja(char, pickup_tick_result),
                    PickupType::PowerupNinjaShield => {
                        self.pickup_ninja_shield(char, pickup_tick_result)
                    }
                }
            } else {
                EntityTickResult::None
            }
        }

        fn tick_deferred(&mut self, _pipe: &mut SimulationPipePickup) -> EntityTickResult {
            EntityTickResult::None
        }

        fn drop_mode(&mut self, mode: DropMode) {
            self.base.drop_mode = mode;
        }
    }

    impl Drop for Pickup {
        fn drop(&mut self) {
            if matches!(self.base.drop_mode, DropMode::None) {
                self.simulation_events
                    .push_world(SimulationEventWorldEntityType::Pickup {
                        id: self.base.game_element_id,
                        ev: PickupEvent::Despawn {
                            pos: self.core.pos,
                            ty: self.core.ty,
                            respawns_in_ticks: 0.into(),
                        },
                    });
            }
        }
    }

    #[derive(Debug, Hiarc, Clone)]
    pub struct PickupPool {
        pub(crate) pickup_pool: Pool<PoolPickups>,
        pub(crate) pickup_reusable_cores_pool: Pool<PickupReusableCore>,
    }

    pub type PoolPickups = FxLinkedHashMap<PickupId, Pickup>;
    pub type Pickups = PoolFxLinkedHashMap<PickupId, Pickup>;
}
