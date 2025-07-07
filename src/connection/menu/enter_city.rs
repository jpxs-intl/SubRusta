use crate::{app_state::AppState, connection::{menu::MenuTypes, ClientConnection}};

pub fn handle_enter_city_menu_action(menu_button: u32, connection: &mut ClientConnection, _state: &AppState) {
    if menu_button == 1 {
        connection.menu = MenuTypes::Lobby;
    }
}