pub mod stage {
    use std::{num::NonZeroU16, rc::Rc};

    use base::{linked_hash_map_view::FxLinkedHashMap, network_string::NetworkString};
    use game_interface::{
        client_commands::MAX_TEAM_NAME_LEN,
        types::{id_gen::IdGenerator, id_types::StageId},
    };
    use hiarc::Hiarc;
    use math::math::vector::ubvec4;

    use crate::{
        entities::character::pos::character_pos::CharacterPositionPlayfield,
        game_objects::game_objects::GameObjectDefinitions,
        match_manager::match_manager::MatchManager,
        match_state::match_state::MatchState,
        simulation_pipe::simulation_pipe::{GameStagePendingEventsRaii, SimulationStageEvents},
        spawns::GameSpawns,
        types::types::GameOptions,
    };

    use super::super::{
        simulation_pipe::simulation_pipe::SimulationPipeStage,
        world::world::{GameWorld, WorldPool},
    };

    /// The game stage represents a well split state of a complete world, which is useful to
    /// have multiple people being able to play on the same server without touching each other.
    ///
    /// It's there to implement ddrace teams.
    #[derive(Debug, Hiarc)]
    pub struct GameStage {
        pub world: GameWorld,
        pub match_manager: MatchManager,
        pub stage_name: NetworkString<MAX_TEAM_NAME_LEN>,
        pub stage_color: ubvec4,
        pub stage_locked: bool,
        pub team0_mode: bool,

        pub(crate) game_pending_events: GameStagePendingEventsRaii,
        pub(crate) simulation_events: SimulationStageEvents,

        game_object_definitions: Rc<GameObjectDefinitions>,
        pub game_element_id: StageId,
    }

    impl GameStage {
        pub fn new(
            stage_name: NetworkString<MAX_TEAM_NAME_LEN>,
            stage_color: ubvec4,
            game_element_id: StageId,
            world_pool: &WorldPool,
            game_object_definitions: &Rc<GameObjectDefinitions>,
            spawns: &Rc<GameSpawns>,
            width: NonZeroU16,
            height: NonZeroU16,
            id_gen: Option<&IdGenerator>,
            game_options: GameOptions,
            game_pending_events: GameStagePendingEventsRaii,
            spawn_default_entities: bool,
        ) -> Self {
            let simulation_events = SimulationStageEvents::default();
            Self {
                world: GameWorld::new(
                    world_pool,
                    game_object_definitions,
                    spawns,
                    id_gen,
                    game_pending_events.clone_evs(),
                    simulation_events.clone_evs(),
                    game_options.clone(),
                    Default::default(),
                    CharacterPositionPlayfield::new(width, height),
                    Default::default(),
                    Default::default(),
                    spawn_default_entities,
                ),
                match_manager: MatchManager::new(game_options, &simulation_events),
                stage_name,
                stage_color,
                stage_locked: false,
                team0_mode: false,
                game_pending_events,
                simulation_events,

                game_object_definitions: game_object_definitions.clone(),

                game_element_id,
            }
        }

        pub fn tick(&mut self, pipe: &mut SimulationPipeStage) {
            self.match_manager
                .game_match
                .tick(&self.match_manager.game_options, &self.world.scores);

            if let MatchState::Running { .. } | MatchState::SuddenDeath { .. } =
                self.match_manager.game_match.state
            {
                self.world.tick(pipe);
            }
            if !pipe.is_prediction && self.match_manager.tick(&mut self.world) {
                let characters = std::mem::replace(
                    &mut self.world.characters,
                    self.world.world_pool.character_pool.character_pool.new(),
                );
                self.world = GameWorld::new(
                    &self.world.world_pool,
                    &self.game_object_definitions,
                    &self.world.spawns,
                    self.world.id_generator.as_ref(),
                    self.game_pending_events.clone_evs(),
                    self.simulation_events.clone_evs(),
                    self.match_manager.game_options.clone(),
                    self.world.phased_characters.clone(),
                    self.world.play_field.clone(),
                    self.world.hooks.clone(),
                    self.world.scores.clone(),
                    true,
                );
                self.world.characters = characters;
                let game_options = self.match_manager.game_options.clone();
                self.match_manager = MatchManager::new(game_options, &self.simulation_events);
            }

            self.simulation_events.clear();
        }
    }

    pub type Stages = FxLinkedHashMap<StageId, GameStage>;
}
