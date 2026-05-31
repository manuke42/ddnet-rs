use std::time::Duration;

use egui::Color32;
use graphics::{
    graphics::graphics::Graphics,
    handles::{
        backend::backend::GraphicsBackendHandle, canvas::canvas::GraphicsCanvasHandle,
        stream::stream::GraphicsStreamHandle, texture::texture::GraphicsTextureHandle,
    },
};
use ingame_ui::motd::{page::MotdUi, user_data::UserData};
use ui_base::{
    types::UiRenderPipe,
    ui::{UiContainer, UiCreator},
};
use ui_generic::generic_ui_renderer;

pub struct MotdRenderPipe<'a> {
    pub cur_time: &'a Duration,
}

pub struct MotdRender {
    pub ui: UiContainer,
    motd_ui: MotdUi,

    backend_handle: GraphicsBackendHandle,
    canvas_handle: GraphicsCanvasHandle,
    stream_handle: GraphicsStreamHandle,
    texture_handle: GraphicsTextureHandle,

    pub msg: String,
    pub started_at: Option<Duration>,
}

impl MotdRender {
    pub fn new(graphics: &Graphics, creator: &UiCreator) -> Self {
        let mut ui = UiContainer::new(creator);
        ui.set_main_panel_color(&Color32::TRANSPARENT);
        Self {
            ui,
            motd_ui: MotdUi::new(),

            backend_handle: graphics.backend_handle.clone(),
            canvas_handle: graphics.canvas_handle.clone(),
            stream_handle: graphics.stream_handle.clone(),
            texture_handle: graphics.texture_handle.clone(),

            msg: Default::default(),
            started_at: None,
        }
    }

    pub fn render(&mut self, pipe: &mut MotdRenderPipe) {
        if self.started_at.is_none_or(|started_at| {
            pipe.cur_time.saturating_sub(started_at) > Duration::from_secs(10)
        }) {
            return;
        }

        let mut user_data = UserData { msg: &self.msg };
        let mut dummy_pipe = UiRenderPipe::new(*pipe.cur_time, &mut user_data);

        generic_ui_renderer::render(
            &self.backend_handle,
            &self.texture_handle,
            &self.stream_handle,
            &self.canvas_handle,
            &mut self.ui,
            &mut self.motd_ui,
            &mut dummy_pipe,
            Default::default(),
        );
    }
}
