use base::network_string::NetworkString;
use hiarc::Hiarc;
use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};

use crate::{
    chat_commands::ClientChatCommand,
    types::{id_types::CharacterId, render::game::game_match::MatchSide},
};

#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub enum ClientCameraMode {
    /// Go back to a non-free cam mode
    None,
    /// The client wants to join a normal freecam
    /// (similar to /pause in ddrace)
    FreeCam(FxHashSet<CharacterId>),
    /// The clients wants to join the freecam and make
    /// himself invisible/phased (similar to /spec in ddrace)
    PhasedFreeCam(FxHashSet<CharacterId>),
}

pub const MAX_TEAM_NAME_LEN: usize = 24;

#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub enum JoinStage {
    /// The default stage that a server usually puts a player in
    /// when first joining the server
    Default,
    Others(NetworkString<MAX_TEAM_NAME_LEN>),
    Own {
        /// The desired name of the stage
        name: NetworkString<MAX_TEAM_NAME_LEN>,
        /// The color of the stage (if the stage doesn't exist yet).
        color: [u8; 3],
    },
}

#[derive(Debug, Hiarc, Clone, Serialize, Deserialize)]
pub enum ClientCommand {
    /// The client requests that his character should respawn
    Kill,
    /// A chat-like command was used (/cmd)
    Chat(ClientChatCommand),
    /// The client wants to join a stage (a.k.a ddrace-team)
    JoinStage(JoinStage),
    /// The client wants to pick a side (red or blue vanilla team)
    JoinSide(MatchSide),
    /// The client wants to join the spectators
    JoinSpectator,
    /// The client requests to switch to a freecam mode
    SetCameraMode(ClientCameraMode),
    /// Local AI scenario reset into an independent world; positions are in tiles.
    AiReset { position: Option<[f32; 2]> },
}
