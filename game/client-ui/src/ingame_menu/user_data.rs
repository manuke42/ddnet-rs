use math::math::Rng;

use client_ui_utils::thumbnail_container::ThumbnailContainer;

use crate::main_menu;

use super::{
    account_info::AccountInfo, server_info::GameServerInfo, server_players::ServerPlayers,
    votes::Votes,
};

pub struct UserData<'a> {
    pub browser_menu: crate::main_menu::user_data::UserData<'a>,
    pub server_players: &'a ServerPlayers,
    pub votes: &'a Votes,
    pub game_server_info: &'a GameServerInfo,
    pub account_info: &'a AccountInfo,
    pub map_vote_thumbnail_container: &'a mut ThumbnailContainer,
    pub rng: &'a mut Rng,
}

impl<'a> AsMut<main_menu::user_data::UserData<'a>> for UserData<'a> {
    fn as_mut(&mut self) -> &mut main_menu::user_data::UserData<'a> {
        &mut self.browser_menu
    }
}
