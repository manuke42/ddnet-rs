use ui_base::types::{UiRenderPipe, UiState};
use ui_generic::traits::UiPageInterface;

use super::{main_frame, user_data::UserData};

pub struct VoteUi {}

impl Default for VoteUi {
    fn default() -> Self {
        Self::new()
    }
}

impl VoteUi {
    pub fn new() -> Self {
        Self {}
    }
}

impl UiPageInterface<UserData<'_>> for VoteUi {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        pipe: &mut UiRenderPipe<UserData>,
        ui_state: &mut UiState,
    ) {
        main_frame::render(ui, pipe, ui_state)
    }
}
