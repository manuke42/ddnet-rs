use ingame_ui::motd::user_data::UserData;
use ui_base::types::{UiRenderPipe, UiState};
use ui_generic::traits::UiPageInterface;

pub struct MotdPage {}

impl Default for MotdPage {
    fn default() -> Self {
        Self::new()
    }
}

impl MotdPage {
    pub fn new() -> Self {
        Self {}
    }

    fn render_impl(
        &mut self,
        ui: &mut egui::Ui,
        pipe: &mut UiRenderPipe<()>,
        ui_state: &mut UiState,
    ) {
        ingame_ui::motd::main_frame::render(
            ui,
            &mut UiRenderPipe {
                cur_time: pipe.cur_time,
                user_data: &mut UserData {
                    msg: "This is an example motd\n\
                        `commonmark attributes` should _just_ __work__.",
                },
            },
            ui_state,
        );
    }
}

impl UiPageInterface<()> for MotdPage {
    fn render(&mut self, ui: &mut egui::Ui, pipe: &mut UiRenderPipe<()>, ui_state: &mut UiState) {
        self.render_impl(ui, pipe, ui_state)
    }
}
