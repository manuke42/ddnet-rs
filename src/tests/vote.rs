use std::time::Duration;

use client_containers::utils::RenderGameContainers;
use client_render::vote::render::{VoteRender, VoteRenderPipe};
use client_render_base::render::tee::RenderTee;
use client_ui_utils::thumbnail_container::ThumbnailContainer;
use game_interface::votes::{
    MapCategoryVoteKey, MapVote, MapVoteDetails, MapVoteKey, VoteState, VoteType, Voted,
};
use graphics::graphics::graphics::Graphics;
use ingame_ui::vote::user_data::{VoteRenderData, VoteRenderPlayer, VoteRenderType};
use ui_base::ui::UiCreator;

use super::utils::render_helper;

pub fn test_vote(
    graphics: &Graphics,
    creator: &UiCreator,
    containers: &mut RenderGameContainers,
    map_vote_thumbnail_container: &mut ThumbnailContainer,
    render_tee: &RenderTee,
    save_screenshot: impl Fn(&str),
) {
    let mut vote = VoteRender::new(graphics, creator);

    let mut time_offset = Duration::ZERO;
    let mut render = |base_name: &str| {
        let render_internal = |_i: u64, time_offset: Duration| {
            vote.render(&mut VoteRenderPipe {
                cur_time: &time_offset,
                skin_container: &mut containers.skin_container,
                map_vote_thumbnail_container,
                tee_render: render_tee,
                vote_data: VoteRenderData {
                    ty: VoteRenderType::PlayerVoteKick(VoteRenderPlayer {
                        name: "nameless tee",
                        skin: &Default::default(),
                        skin_info: &Default::default(),
                        reason: "some reason why player should be kicked",
                    }),
                    data: &VoteState {
                        vote: VoteType::Map {
                            key: MapCategoryVoteKey {
                                category: "Auto".try_into().unwrap(),
                                map: MapVoteKey {
                                    name: "A_Map".try_into().unwrap(),
                                    hash: Default::default(),
                                },
                            },
                            map: MapVote {
                                thumbnail_resource: None,
                                details: MapVoteDetails::None,
                                is_default_map: true,
                            },
                        },
                        remaining_time: Duration::ZERO,
                        yes_votes: 5,
                        no_votes: 4,
                        allowed_to_vote_count: 10,
                    },
                    remaining_time: &Duration::from_secs(1),
                    voted: Some(Voted::Yes),
                },
                expects_player_vote_miniscreen: false,
            });
        };
        render_helper(
            graphics,
            render_internal,
            &mut time_offset,
            base_name,
            &save_screenshot,
        );
    };

    render("vote_broken");
}
