pub mod entry;
pub mod frame;

#[derive(Clone, Copy)]
pub enum ServerListControl {
    Prev,
    Next,
    PrevPage,
    NextPage,
}

pub fn server_list_control_id() -> egui::Id {
    egui::Id::new("main-menu-browser-server-list-navigation")
}
