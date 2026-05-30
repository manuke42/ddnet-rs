pub mod ddrace_projectile {
    use base::linked_hash_map_view::FxLinkedHashMap;
    use game_interface::events::{
        GameGrenadeEventEffect, GameGrenadeEventSound, GameWorldEntityEffectEvent,
        GameWorldEntitySoundEvent,
    };
    use game_interface::types::game::{GameTickCooldownAndLength, GameTickType};
    use game_interface::types::id_types::{CharacterId, ProjectileId};
    use game_interface::types::render::character::CharacterDebuff;
    use game_interface::types::render::projectiles::WeaponWithProjectile;
    use hiarc::Hiarc;
    use math::math::vector::vec2;
    use math::math::{length, lerp, normalize};
    use pool::datatypes::{PoolFxLinkedHashMap, PoolVec};
    use pool::pool::Pool;
    use pool::{recycle::Recycle, traits::Recyclable};
    use serde::{Deserialize, Serialize};

    use crate::collision::collision::{Collision, CollisionTile, CollisionTypes};
    use crate::entities::character::character::BuffProps;
    use crate::entities::entity::entity::{
        DropMode, Entity, EntityInterface, EntityTickResult, calc_pos_and_vel,
    };
    use crate::events::events::ProjectileEvent;
    use crate::reusable::{CloneWithCopyableElements, ReusableCore};
    use crate::simulation_pipe::simulation_pipe::{
        GameWorldPendingEvents, SimulationEventWorldEntityType, SimulationPipeDdraceProjectile,
        SimulationWorldEvents,
    };
    use crate::state::state::TICKS_PER_SECOND;
    use crate::world::world::GameWorld;

    #[derive(Debug, Hiarc, Default, Serialize, Deserialize)]
    pub struct DdraceProjectileReusableCore {}

    impl Recyclable for DdraceProjectileReusableCore {
        fn new() -> Self {
            Self {}
        }

        fn reset(&mut self) {}
    }

    impl CloneWithCopyableElements for DdraceProjectileReusableCore {
        fn copy_clone_from(&mut self, _other: &Self) {}
    }

    impl ReusableCore for DdraceProjectileReusableCore {}

    pub type PoolDdraceProjectileReusableCore = Recycle<DdraceProjectileReusableCore>;

    #[derive(Debug, Hiarc, Copy, Clone, Serialize, Deserialize)]
    pub struct DdraceProjectileCore {
        pub pos: vec2,
        pub start_pos: vec2,
        pub vel: vec2,
        pub life_span: GameTickCooldownAndLength,
        pub damage: u32,
        pub force: f32,
        pub is_explosive: bool,
        pub no_damage_explosion: bool,
        pub can_hit_owner: bool,
        pub freeze: bool,
        pub bouncing: i32,
        pub ty: WeaponWithProjectile,
    }

    #[derive(Debug, Hiarc, Clone)]
    pub struct DdraceProjectilePool {
        pub(crate) projectile_pool: Pool<PoolDdraceProjectiles>,
        pub(crate) projectile_helper: Pool<Vec<(CharacterId, vec2)>>,
    }

    #[derive(Debug, Hiarc)]
    pub struct DdraceProjectile {
        pub(crate) base: Entity<ProjectileId>,
        pub(crate) core: DdraceProjectileCore,

        game_pending_events: GameWorldPendingEvents,
        simulation_events: SimulationWorldEvents,

        helper_ids: PoolVec<(CharacterId, vec2)>,
    }

    impl DdraceProjectile {
        pub fn new(
            game_el_id: &ProjectileId,
            pos: &vec2,
            direction: &vec2,
            life_span: GameTickType,
            damage: u32,
            force: f32,
            explosive: bool,
            no_damage_explosion: bool,
            can_hit_owner: bool,
            freeze: bool,
            bouncing: i32,
            ty: WeaponWithProjectile,
            pool: &DdraceProjectilePool,
            game_pending_events: &GameWorldPendingEvents,
            simulation_events: &SimulationWorldEvents,
        ) -> Self {
            let core = DdraceProjectileCore {
                pos: *pos,
                start_pos: *pos,
                vel: *direction,
                life_span: GameTickCooldownAndLength::new(life_span),
                damage,
                force,
                is_explosive: explosive,
                no_damage_explosion,
                can_hit_owner,
                freeze,
                bouncing,
                ty,
            };
            Self {
                base: Entity::new(game_el_id),
                core,
                game_pending_events: game_pending_events.clone(),
                simulation_events: simulation_events.clone(),
                helper_ids: pool.projectile_helper.new(),
            }
        }

        fn tune(collision: &Collision, core: &DdraceProjectileCore) -> (f32, f32) {
            let tuning = collision.get_tune_at(&core.pos);

            match core.ty {
                WeaponWithProjectile::Grenade => (tuning.grenade_curvature, tuning.grenade_speed),
                WeaponWithProjectile::Shotgun => (tuning.shotgun_curvature, tuning.shotgun_speed),
                WeaponWithProjectile::Gun => (tuning.gun_curvature, tuning.gun_speed),
            }
        }

        fn elapsed_ticks(core: &DdraceProjectileCore) -> GameTickType {
            core.life_span
                .length()
                .map(|length| {
                    length
                        .get()
                        .saturating_sub(core.life_span.get().map(|left| left.get()).unwrap_or(0))
                })
                .unwrap_or_default()
        }

        fn pos_at(collision: &Collision, core: &DdraceProjectileCore, ticks: GameTickType) -> vec2 {
            let (curvature, speed) = Self::tune(collision, core);
            let mut pos = core.start_pos;
            let mut vel = core.vel;
            calc_pos_and_vel(
                &mut pos,
                &mut vel,
                curvature,
                speed,
                ticks as f32 / TICKS_PER_SECOND as f32,
            );
            pos
        }

        fn create_explosion(&mut self, no_dmg: bool, pipe: &mut SimulationPipeDdraceProjectile) {
            let radius = 135;
            let inner_radius = 48.0;
            let intersections = GameWorld::intersect_characters(
                pipe.field,
                pipe.characters.characters_mut(),
                &self.core.pos,
                radius,
            );

            self.helper_ids.clear();
            self.helper_ids.extend(intersections.map(|character| {
                let diff = *character.pos.pos() - self.core.pos;
                (character.base.game_element_id, diff)
            }));

            let mut no_damage_hit_side_none = false;
            let mut no_damage_hit_side_red = false;
            let mut no_damage_hit_side_blue = false;
            for (id, diff) in self.helper_ids.drain(..) {
                let mut force_dir = vec2::new(0.0, 1.0);
                let mut l = length(&diff);
                if l > 0.0 {
                    force_dir = normalize(&diff);
                }
                l = 1.0 - ((l - inner_radius) / (radius as f32 - inner_radius)).clamp(0.0, 1.0);
                let strength = pipe
                    .collision
                    .get_tune_at(&self.core.pos)
                    .explosion_strength;

                let dmg = strength * l;
                if dmg <= 0.0 {
                    continue;
                }

                if no_dmg && let Some(character) = pipe.characters.characters_mut().get_mut(&id) {
                    let already_hit = match character.core.side {
                        Some(game_interface::types::render::game::game_match::MatchSide::Red) => {
                            &mut no_damage_hit_side_red
                        }
                        Some(game_interface::types::render::game::game_match::MatchSide::Blue) => {
                            &mut no_damage_hit_side_blue
                        }
                        None => &mut no_damage_hit_side_none,
                    };
                    if *already_hit {
                        continue;
                    }
                    *already_hit = true;
                    character.core.core.vel += force_dir * dmg * 2.0;
                }
            }
        }
    }

    impl
        EntityInterface<
            DdraceProjectileCore,
            DdraceProjectileReusableCore,
            SimulationPipeDdraceProjectile<'_>,
        > for DdraceProjectile
    {
        fn pre_tick(&mut self, _pipe: &mut SimulationPipeDdraceProjectile) -> EntityTickResult {
            todo!()
        }

        fn tick(&mut self, pipe: &mut SimulationPipeDdraceProjectile) -> EntityTickResult {
            let elapsed_ticks = Self::elapsed_ticks(&self.core);
            let prev_pos = Self::pos_at(pipe.collision, &self.core, elapsed_ticks);
            let mut cur_pos = Self::pos_at(pipe.collision, &self.core, elapsed_ticks + 1);
            let mut dummy_pos = Default::default();
            let collide = pipe.collision.intersect_line(
                &prev_pos,
                &cur_pos.clone(),
                &mut cur_pos,
                &mut dummy_pos,
                CollisionTypes::SOLID,
            );

            let life_span_expired = self.core.life_span.tick().unwrap_or(true);

            let intersection_radius = if self.core.freeze { 1.0 } else { 6.0 };
            let intersection = GameWorld::intersect_character_on_line(
                pipe.field,
                pipe.characters.characters_mut(),
                &prev_pos,
                &cur_pos,
                intersection_radius,
            );

            if intersection.is_some()
                || !matches!(collide, CollisionTile::None)
                || life_span_expired
                || Entity::<ProjectileId>::outside_of_playfield(&cur_pos, pipe.collision)
            {
                self.core.pos = cur_pos;
                let explosive_impact = self.core.is_explosive
                    && match intersection {
                        Some(_) => {
                            !self.core.freeze
                                || (self.core.ty == WeaponWithProjectile::Shotgun
                                    && !matches!(collide, CollisionTile::None))
                        }
                        None => true,
                    };
                if explosive_impact {
                    self.game_pending_events.push_sound(
                        None,
                        Some(self.core.pos),
                        GameWorldEntitySoundEvent::Grenade(GameGrenadeEventSound::Explosion),
                    );
                    self.game_pending_events.push_effect(
                        None,
                        self.core.pos,
                        GameWorldEntityEffectEvent::Grenade(GameGrenadeEventEffect::Explosion),
                    );
                    self.create_explosion(self.core.no_damage_explosion, pipe);
                } else if self.core.freeze {
                    for character in pipe
                        .characters
                        .characters_mut()
                        .iter_mut()
                        .map(|(_, character)| character)
                        .filter(|character| length(&(*character.pos.pos() - cur_pos)) <= 1.0)
                    {
                        character.reusable_core.debuffs.insert(
                            CharacterDebuff::Freeze,
                            BuffProps {
                                remaining_tick: TICKS_PER_SECOND.into(),
                                interact_tick: 0.into(),
                                interact_cursor_dir: Default::default(),
                                interact_val: 0.0,
                            },
                        );
                    }
                }
                if !matches!(collide, CollisionTile::None) && self.core.bouncing != 0 {
                    let mut dir = self.core.vel;
                    self.core.pos = dummy_pos - dir * 4.0;
                    if self.core.bouncing == 1 {
                        dir.x = -dir.x;
                    } else if self.core.bouncing == 2 {
                        dir.y = -dir.y;
                    }
                    if dir.x.abs() < 1e-6 {
                        dir.x = 0.0;
                    }
                    if dir.y.abs() < 1e-6 {
                        dir.y = 0.0;
                    }
                    self.core.pos += dir;
                    self.core.start_pos = self.core.pos;
                    self.core.vel = dir;
                    let remaining_ticks = self
                        .core
                        .life_span
                        .get()
                        .map(|ticks_left| ticks_left.get())
                        .unwrap_or_default();
                    self.core.life_span = GameTickCooldownAndLength::new_with_length(
                        remaining_ticks,
                        remaining_ticks,
                    );
                    EntityTickResult::None
                } else {
                    EntityTickResult::RemoveEntity
                }
            } else {
                self.core.pos = cur_pos;
                EntityTickResult::None
            }
        }

        fn tick_deferred(
            &mut self,
            _pipe: &mut SimulationPipeDdraceProjectile,
        ) -> EntityTickResult {
            EntityTickResult::None
        }

        fn drop_mode(&mut self, mode: DropMode) {
            self.base.drop_mode = mode;
        }
    }

    impl Drop for DdraceProjectile {
        fn drop(&mut self) {
            if matches!(self.base.drop_mode, DropMode::None) {
                self.simulation_events
                    .push_world(SimulationEventWorldEntityType::Projectile {
                        id: self.base.game_element_id,
                        ev: ProjectileEvent::Despawn {
                            pos: self.core.pos,
                            respawns_in_ticks: 0.into(),
                        },
                    });
            }
        }
    }

    pub fn lerped_pos(proj1: &DdraceProjectile, proj2: &DdraceProjectile, ratio: f64) -> vec2 {
        lerp(&proj1.core.pos, &proj2.core.pos, ratio as f32)
    }

    pub fn estimated_fly_direction(
        proj1: &DdraceProjectile,
        proj2: &DdraceProjectile,
        ratio: f64,
    ) -> vec2 {
        lerp(&proj1.core.vel, &proj2.core.vel, ratio as f32)
    }

    pub type PoolDdraceProjectiles = FxLinkedHashMap<ProjectileId, DdraceProjectile>;
    pub type DdraceProjectiles = PoolFxLinkedHashMap<ProjectileId, DdraceProjectile>;
}
