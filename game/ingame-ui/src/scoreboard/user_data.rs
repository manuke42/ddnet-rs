use base::linked_hash_map_view::FxLinkedHashMap;
use client_containers::{flags::FlagsContainer, skins::SkinContainer};
use client_render_base::render::tee::RenderTee;
use game_interface::types::{
    id_types::CharacterId,
    render::{character::CharacterInfo, scoreboard::Scoreboard},
};
use graphics::handles::{
    canvas::canvas::GraphicsCanvasHandle, stream::stream::GraphicsStreamHandle,
};

pub struct UserData<'a> {
    pub scoreboard: &'a Scoreboard,
    pub character_infos: &'a FxLinkedHashMap<CharacterId, CharacterInfo>,
    pub stream_handle: &'a GraphicsStreamHandle,
    pub canvas_handle: &'a GraphicsCanvasHandle,
    pub skin_container: &'a mut SkinContainer,
    pub render_tee: &'a RenderTee,
    pub flags_container: &'a mut FlagsContainer,

    pub own_character_id: &'a CharacterId,
}
