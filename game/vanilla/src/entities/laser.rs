pub mod laser {
    use base::linked_hash_map_view::FxLinkedHashMap;
    use game_interface::events::{GameLaserEventSound, GameWorldEntitySoundEvent, KillFlags};
    use game_interface::types::game::{
        GameTickCooldownAndLastActionCounter, GameTickType, NonZeroGameTickType,
    };
    use game_interface::types::id_types::{CharacterId, LaserId};
    use game_interface::types::render::game::game_match::MatchSide;
    use game_interface::types::weapons::WeaponType;
    use hiarc::Hiarc;
    use math::math::vector::vec2;
    use math::math::{distance, normalize};
    use pool::datatypes::PoolFxLinkedHashMap;
    use pool::pool::Pool;
    use pool::{recycle::Recycle, traits::Recyclable};
    use serde::{Deserialize, Serialize};

    use crate::reusable::{CloneWithCopyableElements, ReusableCore};
    use game_interface::types::laser::LaserType;

    use crate::collision::{CollisionTile, CollisionTypes};
    use crate::entities::character::character::{Character, DamageBy, DamageTypes};
    use crate::entities::entity::entity::{DropMode, Entity, EntityInterface, EntityTickResult};
    use crate::events::events::LaserEvent;
    use crate::simulation_pipe::simulation_pipe::{
        GameWorldPendingEvents, SimulationEventWorldEntityType, SimulationPipeLaser,
        SimulationWorldEvents,
    };
    use crate::state::state::TICKS_PER_SECOND;
    use crate::world::world::GameWorld;

    #[derive(Debug, Hiarc, Default, Serialize, Deserialize)]
    pub struct LaserReusableCore {}

    impl Recyclable for LaserReusableCore {
        fn new() -> Self {
            Self {}
        }

        fn reset(&mut self) {}
    }

    impl CloneWithCopyableElements for LaserReusableCore {
        fn copy_clone_from(&mut self, _other: &Self) {}
    }

    impl ReusableCore for LaserReusableCore {}

    pub type PoolLaserReusableCore = Recycle<LaserReusableCore>;

    #[derive(Debug, Hiarc, Default, Copy, Clone, Serialize, Deserialize)]
    pub struct LaserCore {
        pub pos: vec2,
        pub from: vec2,
        pub dir: vec2,
        pub ty: LaserType,

        pub energy: f32,
        pub bounces: usize,
        pub next_eval_in: GameTickCooldownAndLastActionCounter,
        // TODO: int m_Owner;
        // TODO: int m_TeamMask;

        // can this entity hit players and own player
        pub can_hit_others: bool,
        pub can_hit_own: bool,

        pub side: Option<MatchSide>,
    }

    #[derive(Debug, Hiarc, Clone)]
    pub struct LaserPool {
        pub(crate) laser_pool: Pool<PoolLasers>,
        pub(crate) laser_reusable_cores_pool: Pool<LaserReusableCore>,
    }

    #[derive(Debug, Hiarc)]
    pub struct Laser {
        pub(crate) base: Entity<LaserId>,
        pub(crate) core: LaserCore,
        pub(crate) reusable_core: PoolLaserReusableCore,

        game_pending_events: GameWorldPendingEvents,
        simulation_events: SimulationWorldEvents,
    }

    impl Laser {
        pub fn new(
            game_el_id: &LaserId,
            pos: &vec2,
            dir: &vec2,
            ty: LaserType,
            start_energy: f32,

            can_hit_others: bool,
            can_hit_own: bool,

            side: Option<MatchSide>,

            pool: &LaserPool,
            game_pending_events: &GameWorldPendingEvents,
            simulation_events: &SimulationWorldEvents,
        ) -> Self {
            let core = LaserCore {
                pos: *pos,
                from: *pos,
                ty,
                bounces: 0,
                dir: *dir,
                energy: start_energy,
                next_eval_in: Default::default(),

                can_hit_others,
                can_hit_own,

                side,
            };

            Self {
                base: Entity::new(game_el_id),
                core,
                reusable_core: pool.laser_reusable_cores_pool.new(),
                game_pending_events: game_pending_events.clone(),
                simulation_events: simulation_events.clone(),
            }
        }

        pub fn from(other: &Self, pool: &mut LaserPool) -> Self {
            let mut reusable_core = pool.laser_reusable_cores_pool.new();
            reusable_core.copy_clone_from(&other.reusable_core);
            Self {
                base: Entity::new(&other.base.game_element_id),
                core: other.core,
                reusable_core,

                game_pending_events: other.game_pending_events.clone(),
                simulation_events: other.simulation_events.clone(),
            }
        }

        pub fn pos(&self) -> vec2 {
            self.core.pos
        }

        pub fn pos_from(&self) -> vec2 {
            self.core.from
        }

        pub fn eval_tick_ratio(&self) -> Option<(GameTickType, NonZeroGameTickType)> {
            self.core.next_eval_in.action_ticks_and_cooldown_len()
        }

        fn hit_character(
            &mut self,
            pipe: &mut SimulationPipeLaser,
            from: &vec2,
            to: &vec2,
        ) -> bool {
            let dont_hit_self = self.core.bounces == 0;

            let mut char_intersection = None;
            if self.core.can_hit_others {
                let intersection = if self.core.can_hit_own && !dont_hit_self {
                    GameWorld::intersect_character_on_line(
                        pipe.field,
                        pipe.characters_helper.get_characters(),
                        &self.core.pos,
                        to,
                        0.0,
                    )
                } else {
                    GameWorld::intersect_character_on_line(
                        pipe.field,
                        pipe.characters_helper.get_characters_except_owner(),
                        &self.core.pos,
                        to,
                        0.0,
                    )
                };
                char_intersection = intersection;
            } else if self.core.can_hit_own && !dont_hit_self {
                // check if owner was hit
                let intersection = GameWorld::intersect_character_on_line(
                    pipe.field,
                    pipe.characters_helper.get_owner_character_view(),
                    &self.core.pos,
                    to,
                    0.0,
                );
                char_intersection = intersection;
            }

            let Some((_, pos, char)) = char_intersection else {
                return false;
            };
            self.core.from = *from;
            self.core.pos = pos;
            self.core.energy = -1.0;

            if let LaserType::Puller = self.core.ty {
                let strength = pipe.collision.get_tune_at(&self.core.pos).shotgun_strength;
                let pull_dir = normalize(&(*from - *char.pos.pos()));
                char.core.core.vel += pull_dir * strength;
            } else if let LaserType::Rifle = self.core.ty {
                let dmg_amount = pipe.collision.get_tune_at(&self.core.pos).laser_damage;
                let hitted_char_id = char.base.game_element_id;
                Character::take_damage(
                    pipe.characters_helper.characters,
                    &hitted_char_id,
                    &Default::default(),
                    &Default::default(),
                    dmg_amount as u32,
                    match self.core.side {
                        Some(side) => DamageTypes::CharacterInMatchSide {
                            char_id: &pipe.characters_helper.owner_character,
                            side,
                        },
                        None => DamageTypes::Character(&pipe.characters_helper.owner_character),
                    },
                    DamageBy::Weapon {
                        weapon: WeaponType::Laser,
                        flags: if self.core.bounces > 0 {
                            KillFlags::WALLSHOT
                        } else {
                            KillFlags::empty()
                        },
                    },
                );
            }
            true
        }

        fn do_bounce(&mut self, pipe: &mut SimulationPipeLaser) -> bool {
            let tuning = pipe.collision.get_tune_at(&self.core.pos);
            let delay = tuning.laser_bounce_delay;
            self.core.next_eval_in =
                ((TICKS_PER_SECOND as f32 * delay / 1000.0).ceil() as GameTickType).into();

            if self.core.energy < 0.0 {
                return false;
            }
            //self.core.m_PrevPos = self.core.pos;
            let mut col_tile = vec2::default();

            let mut to = self.core.pos + self.core.dir * self.core.energy;

            let res = pipe.collision.intersect_line(
                &self.core.pos,
                &to.clone(),
                &mut col_tile,
                &mut to,
                CollisionTypes::SOLID | CollisionTypes::WEAPON_TELE,
            );

            if !matches!(res, CollisionTile::None) {
                let cur_pos = self.core.pos;
                if !self.hit_character(pipe, &cur_pos, &to) {
                    let core = &mut self.core;
                    // intersected
                    core.from = core.pos;
                    core.pos = to;

                    let mut tmp_pos = core.pos;
                    let mut tmp_dir = core.dir * 4.0;

                    // TODO: let mut f = 0;
                    // TODO: this looks like a hack, maybe remove it completely
                    /*if res == -1 {
                        // TODO: f = GameServer()->Collision()->GetTile(round_to_int(Coltile.x), round_to_int(Coltile.y));
                        // TODO: GameServer()->Collision()->SetCollisionAt(round_to_int(Coltile.x), round_to_int(Coltile.y), TILE_SOLID);
                    }*/
                    pipe.collision
                        .move_point(&mut tmp_pos, &mut tmp_dir, 1.0, &mut 0);
                    /*if res == -1 {
                        // TODO:   GameServer()->Collision()->SetCollisionAt(round_to_int(Coltile.x), round_to_int(Coltile.y), f);
                    }*/
                    core.pos = tmp_pos;
                    core.dir = normalize(&tmp_dir);

                    let d = distance(&core.from, &core.pos);
                    // Prevent infinite bounces
                    if core.bounces > 0 && d == 0.0 {
                        core.energy = -1.0;
                    } else {
                        let tuning = pipe.collision.get_tune_at(&core.pos);
                        core.energy -= d + tuning.laser_bounce_cost;
                    }

                    core.bounces += 1;

                    let tuning = pipe.collision.get_tune_at(&core.pos);
                    let bounce_num = tuning.laser_bounce_num as usize;

                    if core.bounces > bounce_num {
                        core.energy = -1.0;
                    }

                    self.game_pending_events.push_sound(
                        Some(pipe.characters_helper.owner_character),
                        Some(core.pos),
                        GameWorldEntitySoundEvent::Laser(GameLaserEventSound::Bounce),
                    );
                }
            } else {
                let cur_pos = self.core.pos;
                if !self.hit_character(pipe, &cur_pos, &to) {
                    self.core.from = self.core.pos;
                    self.core.pos = to;
                    self.core.energy = -1.0;
                }
            }

            true
        }

        pub fn lerped_pos(laser1: &Laser, _laser2: &Laser, _ratio: f64) -> vec2 {
            laser1.core.pos
        }
        pub fn lerped_from(laser1: &Laser, _laser2: &Laser, _ratio: f64) -> vec2 {
            laser1.core.from
        }
    }

    impl EntityInterface<LaserCore, LaserReusableCore, SimulationPipeLaser<'_>> for Laser {
        fn pre_tick(&mut self, _pipe: &mut SimulationPipeLaser) -> EntityTickResult {
            todo!()
        }

        fn tick(&mut self, pipe: &mut SimulationPipeLaser) -> EntityTickResult {
            if self
                .core
                .next_eval_in
                .tick()
                .cooldown_fell_to_zero_or_none()
            {
                if self.do_bounce(pipe) {
                    EntityTickResult::None
                } else {
                    EntityTickResult::RemoveEntity
                }
            } else {
                EntityTickResult::None
            }
        }

        fn tick_deferred(&mut self, _pipe: &mut SimulationPipeLaser) -> EntityTickResult {
            EntityTickResult::None
        }

        fn drop_mode(&mut self, mode: DropMode) {
            self.base.drop_mode = mode;
        }
    }

    impl Drop for Laser {
        fn drop(&mut self) {
            if matches!(self.base.drop_mode, DropMode::None) {
                self.simulation_events
                    .push_world(SimulationEventWorldEntityType::Laser {
                        id: self.base.game_element_id,
                        ev: LaserEvent::Despawn {
                            pos: self.core.pos,
                            respawns_in_ticks: 0.into(),
                        },
                    });
            }
        }
    }

    #[derive(Debug, Hiarc)]
    pub struct WorldLaser {
        pub character_id: CharacterId,
        pub laser: Laser,
    }

    pub type PoolLasers = FxLinkedHashMap<LaserId, WorldLaser>;
    pub type Lasers = PoolFxLinkedHashMap<LaserId, WorldLaser>;
}
