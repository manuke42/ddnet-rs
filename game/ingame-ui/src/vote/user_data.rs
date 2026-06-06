use std::time::Duration;

use client_containers::skins::SkinContainer;
use client_render_base::render::tee::RenderTee;
use egui::Rect;
use game_interface::{
    types::{character_info::NetworkSkinInfo, resource_key::ResourceKey},
    votes::{
        MapCategoryVoteKey, MapVote, MiscVote, MiscVoteCategoryKey, RandomUnfinishedMapKey,
        VoteState, Voted,
    },
};
use graphics::handles::{
    canvas::canvas::GraphicsCanvasHandle, stream::stream::GraphicsStreamHandle,
};

use client_ui_utils::thumbnail_container::ThumbnailContainer;

#[derive(Debug, Clone, Copy)]
pub struct VoteRenderPlayer<'a> {
    pub name: &'a str,
    pub skin: &'a ResourceKey,
    pub skin_info: &'a NetworkSkinInfo,
    pub reason: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub enum VoteRenderType<'a> {
    Map {
        key: &'a MapCategoryVoteKey,
        map: &'a MapVote,
    },
    RandomUnfinishedMap {
        key: &'a RandomUnfinishedMapKey,
    },
    PlayerVoteKick(VoteRenderPlayer<'a>),
    PlayerVoteSpec(VoteRenderPlayer<'a>),
    Misc {
        key: &'a MiscVoteCategoryKey,
        vote: &'a MiscVote,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct VoteRenderData<'a> {
    pub ty: VoteRenderType<'a>,
    pub data: &'a VoteState,
    pub remaining_time: &'a Duration,
    pub voted: Option<Voted>,
}

pub struct UserData<'a> {
    pub stream_handle: &'a GraphicsStreamHandle,
    pub canvas_handle: &'a GraphicsCanvasHandle,
    pub skin_container: &'a mut SkinContainer,
    pub map_vote_thumbnail_container: &'a mut ThumbnailContainer,
    pub render_tee: &'a RenderTee,

    pub vote_data: VoteRenderData<'a>,

    /// `true` if a miniscreen will be used, that
    /// shows the current player.
    pub player_vote_miniscreen: bool,
    pub player_vote_rect: &'a mut Option<Rect>,
}
