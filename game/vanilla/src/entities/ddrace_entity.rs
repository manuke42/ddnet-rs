pub mod ddrace_entity {
    use base::linked_hash_map_view::FxLinkedHashMap;
    use game_interface::types::{
        game::{GameEntityId, GameTickCooldownAndLastActionCounter, GameTickType},
        render::character::CharacterDebuff,
    };
    use hiarc::Hiarc;
    use math::math::{
        closest_point_on_line, distance, length, normalize,
        vector::{ivec2, vec2},
    };
    use pool::{datatypes::PoolFxLinkedHashMap, pool::Pool, recycle::Recycle, traits::Recyclable};
    use serde::{Deserialize, Serialize};

    use crate::{
        collision::collision::{Collision, CollisionTile, CollisionTypes},
        entities::{
            character::{
                character::{BuffProps, Character},
                core::character_core,
            },
            entity::entity::{DropMode, Entity, EntityInterface, EntityTickResult},
        },
        game_objects::game_objects::{DdraceMapEntityDefinition, DdraceMapEntityKind},
        reusable::{CloneWithCopyableElements, ReusableCore},
        simulation_pipe::simulation_pipe::SimulationPipeDdraceEntity,
        state::state::TICKS_PER_SECOND,
    };

    #[derive(Debug, Hiarc, Default, Serialize, Deserialize)]
    pub struct DdraceEntityReusableCore {}

    impl Recyclable for DdraceEntityReusableCore {
        fn new() -> Self {
            Self {}
        }

        fn reset(&mut self) {}
    }

    impl CloneWithCopyableElements for DdraceEntityReusableCore {
        fn copy_clone_from(&mut self, _other: &Self) {}
    }

    impl ReusableCore for DdraceEntityReusableCore {}

    pub type PoolDdraceEntityReusableCore = Recycle<DdraceEntityReusableCore>;

    #[derive(Debug, Hiarc, Copy, Clone, Serialize, Deserialize)]
    pub struct DdraceEntityCore {
        pub pos: ivec2,
        pub kind: DdraceMapEntityKind,
        pub flags: map::map::groups::layers::tiles::TileFlags,
        pub number: Option<u8>,
        pub action_counter: GameTickCooldownAndLastActionCounter,
    }

    #[derive(Debug, Hiarc)]
    pub struct DdraceEntity {
        pub(crate) base: Entity<GameEntityId>,
        pub(crate) core: DdraceEntityCore,
        pub(crate) reusable_core: PoolDdraceEntityReusableCore,
    }

    impl DdraceEntity {
        pub fn new(
            game_el_id: &GameEntityId,
            definition: &DdraceMapEntityDefinition<ivec2>,
            pool: &DdraceEntityPool,
        ) -> Self {
            Self {
                base: Entity::new(game_el_id),
                core: DdraceEntityCore {
                    pos: definition.pos,
                    kind: definition.kind,
                    flags: definition.flags,
                    number: definition.number,
                    action_counter: GameTickCooldownAndLastActionCounter::new(1),
                },
                reusable_core: pool.ddrace_entity_reusable_cores_pool.new(),
            }
        }

        fn world_pos(&self) -> vec2 {
            vec2::new(
                self.core.pos.x as f32 * 32.0 + 16.0,
                self.core.pos.y as f32 * 32.0 + 16.0,
            )
        }

        fn freeze_character(character: &mut Character, ticks: GameTickType) {
            character.reusable_core.debuffs.insert(
                CharacterDebuff::Freeze,
                BuffProps {
                    remaining_tick: ticks.into(),
                    interact_tick: 0.into(),
                    interact_cursor_dir: Default::default(),
                    interact_val: 0.0,
                },
            );
        }

        fn clip_line(collision: &Collision, from: vec2, to: vec2) -> vec2 {
            let mut collision_pos = vec2::default();
            let mut before_collision_pos = to;
            if matches!(
                collision.intersect_line(
                    &from,
                    &to,
                    &mut collision_pos,
                    &mut before_collision_pos,
                    CollisionTypes::SOLID,
                ),
                CollisionTile::None
            ) {
                to
            } else {
                before_collision_pos
            }
        }

        fn entity_at(entities: &[DdraceEntityCore], pos: ivec2) -> Option<DdraceMapEntityKind> {
            entities
                .iter()
                .find(|entity| entity.pos == pos)
                .map(|entity| entity.kind)
        }

        fn line_touches_character(pos: &vec2, from: &vec2, to: &vec2, radius: f32) -> bool {
            let mut closest = vec2::default();
            closest_point_on_line(from, to, pos, &mut closest)
                && distance(pos, &closest) <= radius + character_core::PHYSICAL_SIZE / 2.0
        }

        fn active_for_character(&self, character: &Character) -> bool {
            self.core
                .number
                .is_none_or(|number| character.switch_active(number))
        }

        fn tile_direction(index: usize) -> (ivec2, vec2, f32) {
            let angle = std::f32::consts::FRAC_PI_4 * index as f32;
            let tile_dir = match index {
                0 => ivec2::new(0, 1),
                1 => ivec2::new(1, 1),
                2 => ivec2::new(1, 0),
                3 => ivec2::new(1, -1),
                4 => ivec2::new(0, -1),
                5 => ivec2::new(-1, -1),
                6 => ivec2::new(-1, 0),
                _ => ivec2::new(-1, 1),
            };
            (tile_dir, vec2::new(angle.sin(), angle.cos()), angle)
        }

        pub(crate) fn crazy_shotgun_direction(
            flags: map::map::groups::layers::tiles::TileFlags,
        ) -> vec2 {
            use map::map::groups::layers::tiles::{
                ROTATION_0, ROTATION_90, TileFlags, rotation_180,
            };

            let flags = flags & (TileFlags::XFLIP | TileFlags::YFLIP | TileFlags::ROTATE);
            match flags {
                ROTATION_0 => vec2::new(0.0, 1.0),
                ROTATION_90 => vec2::new(1.0, 0.0),
                flags if flags == rotation_180() => vec2::new(0.0, -1.0),
                _ => vec2::new(-1.0, 0.0),
            }
        }

        fn tick_impl(&mut self, pipe: &mut SimulationPipeDdraceEntity) -> EntityTickResult {
            let _ = &self.reusable_core;
            let pos = self.world_pos();
            let ticks = self.core.action_counter.action_ticks().unwrap_or_default();
            let tick = ticks as f32;

            if let Some(strength) = self.core.kind.dragger_strength() {
                for character in pipe.characters.values_mut() {
                    if !self.active_for_character(character) {
                        continue;
                    }
                    let diff = pos - *character.pos.pos();
                    let dist = length(&diff);
                    if dist >= 700.0 || dist <= 0.0 {
                        continue;
                    }
                    let blocked = !matches!(
                        if self.core.kind.dragger_ignores_walls() {
                            pipe.collision.intersect_no_laser_no_walls(
                                &pos,
                                character.pos.pos(),
                                &mut vec2::default(),
                                &mut vec2::default(),
                            )
                        } else {
                            pipe.collision.intersect_no_laser(
                                &pos,
                                character.pos.pos(),
                                &mut vec2::default(),
                                &mut vec2::default(),
                            )
                        },
                        CollisionTile::None
                    );
                    if !blocked {
                        character.core.core.vel += normalize(&diff) * strength;
                    }
                }
                return EntityTickResult::None;
            }

            if self.core.kind.laser_angular_speed().is_some() {
                for dir_index in 0..8 {
                    let (tile_dir, base_dir, base_angle) = Self::tile_direction(dir_index);
                    let Some(length_kind) =
                        Self::entity_at(pipe.entities, self.core.pos + tile_dir)
                    else {
                        continue;
                    };
                    let Some(mut laser_length) = length_kind.laser_length() else {
                        continue;
                    };

                    if let Some(curve_kind) =
                        Self::entity_at(pipe.entities, self.core.pos + tile_dir * 2)
                        && let Some(curve_speed) = curve_kind.laser_curve_speed()
                    {
                        let period = (TICKS_PER_SECOND as f32 * 4.0 / curve_speed).max(1.0);
                        let phase = (tick % period) / period;
                        let triangle = if phase < 0.5 {
                            phase * 2.0
                        } else {
                            (1.0 - phase) * 2.0
                        };
                        let factor = if curve_kind.laser_curve_starts_open() {
                            1.0 - triangle
                        } else {
                            triangle
                        };
                        laser_length *= factor;
                    }

                    let angular_speed = self.core.kind.laser_angular_speed().unwrap();
                    let angle = base_angle + angular_speed * tick;
                    let dir = if angular_speed == 0.0 {
                        base_dir
                    } else {
                        vec2::new(angle.sin(), angle.cos())
                    };
                    let to = Self::clip_line(pipe.collision, pos, pos + dir * laser_length);
                    for character in pipe.characters.values_mut() {
                        if !self.active_for_character(character) {
                            continue;
                        }
                        if Self::line_touches_character(character.pos.pos(), &pos, &to, 0.0) {
                            Self::freeze_character(character, TICKS_PER_SECOND);
                        }
                    }
                }
                return EntityTickResult::None;
            }

            if matches!(self.core.kind, DdraceMapEntityKind::Door) {
                for dir_index in 0..8 {
                    let (tile_dir, dir, _) = Self::tile_direction(dir_index);
                    let Some(length_kind) =
                        Self::entity_at(pipe.entities, self.core.pos + tile_dir)
                    else {
                        continue;
                    };
                    let Some(door_length) = length_kind.laser_length() else {
                        continue;
                    };
                    let to = Self::clip_line(pipe.collision, pos, pos + dir * door_length);
                    for character in pipe.characters.values_mut() {
                        if !self.active_for_character(character) {
                            continue;
                        }
                        if Self::line_touches_character(character.pos.pos(), &pos, &to, 2.0) {
                            let mut closest = vec2::default();
                            closest_point_on_line(&pos, &to, character.pos.pos(), &mut closest);
                            let push = *character.pos.pos() - closest;
                            if push != vec2::default() {
                                character
                                    .pos
                                    .move_pos(*character.pos.pos() + normalize(&push) * 4.0);
                            }
                            character.core.core.vel = vec2::default();
                        }
                    }
                }
                return EntityTickResult::None;
            }

            if self.core.kind.plasma_freezes()
                || self.core.kind.plasma_unfreezes()
                || self.core.kind.plasma_explodes()
            {
                if !ticks.is_multiple_of(7) {
                    return EntityTickResult::None;
                }
                let mut closest_char = None;
                let mut closest_dist = f32::MAX;
                for (id, character) in pipe.characters.iter() {
                    if !self.active_for_character(character) {
                        continue;
                    }
                    let dist = distance(&pos, character.pos.pos());
                    if dist < closest_dist && dist < 700.0 {
                        let blocked = !matches!(
                            pipe.collision.intersect_no_laser(
                                &pos,
                                character.pos.pos(),
                                &mut vec2::default(),
                                &mut vec2::default(),
                            ),
                            CollisionTile::None
                        );
                        if !blocked {
                            closest_char = Some(*id);
                            closest_dist = dist;
                        }
                    }
                }
                if let Some(id) = closest_char
                    && let Some(character) = pipe.characters.get_mut(&id)
                {
                    if self.core.kind.plasma_freezes() {
                        Self::freeze_character(character, TICKS_PER_SECOND);
                    }
                    if self.core.kind.plasma_unfreezes() {
                        character
                            .reusable_core
                            .debuffs
                            .remove(&CharacterDebuff::Freeze);
                    }
                    if self.core.kind.plasma_explodes() {
                        let diff = *character.pos.pos() - pos;
                        if diff != vec2::default() {
                            character.core.core.vel += normalize(&diff) * 10.0;
                        }
                    }
                }
                return EntityTickResult::None;
            }

            if self.core.kind.is_crazy_shotgun() {
                let dir = Self::crazy_shotgun_direction(self.core.flags);
                if self.core.kind.crazy_shotgun_explodes() {
                    return EntityTickResult::RemoveEntity;
                }

                if !ticks.is_multiple_of(22) {
                    return EntityTickResult::None;
                }
                let to = Self::clip_line(pipe.collision, pos, pos + dir * 900.0);
                for character in pipe.characters.values_mut() {
                    if !self.active_for_character(character) {
                        continue;
                    }
                    if Self::line_touches_character(character.pos.pos(), &pos, &to, 6.0) {
                        Self::freeze_character(character, TICKS_PER_SECOND);
                    }
                }
            }

            EntityTickResult::None
        }
    }

    impl EntityInterface<DdraceEntityCore, DdraceEntityReusableCore, SimulationPipeDdraceEntity<'_>>
        for DdraceEntity
    {
        fn pre_tick(&mut self, _pipe: &mut SimulationPipeDdraceEntity) -> EntityTickResult {
            EntityTickResult::None
        }

        fn tick(&mut self, pipe: &mut SimulationPipeDdraceEntity) -> EntityTickResult {
            let res = self.tick_impl(pipe);
            self.core.action_counter.tick();
            res
        }

        fn tick_deferred(&mut self, _pipe: &mut SimulationPipeDdraceEntity) -> EntityTickResult {
            EntityTickResult::None
        }

        fn drop_mode(&mut self, mode: DropMode) {
            self.base.drop_mode = mode;
        }
    }

    #[derive(Debug, Hiarc, Clone)]
    pub struct DdraceEntityPool {
        pub(crate) ddrace_entity_pool: Pool<PoolDdraceEntities>,
        pub(crate) ddrace_entity_reusable_cores_pool: Pool<DdraceEntityReusableCore>,
    }

    pub type PoolDdraceEntities = FxLinkedHashMap<GameEntityId, DdraceEntity>;
    pub type DdraceEntities = PoolFxLinkedHashMap<GameEntityId, DdraceEntity>;
}
