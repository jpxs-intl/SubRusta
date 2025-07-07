use crate::{app_state::AppState, connection::ClientConnection, packets::Team};

pub fn handle_lobby_menu_action(menu_button: u32, connection: &mut ClientConnection, state: &AppState) {
    let mut lobby = state.game_state.write().unwrap();

    if menu_button == 5 {
        lobby.ready[connection.client_id as usize] = !lobby.ready[connection.client_id as usize];

        return;
    }

    lobby.ready[connection.client_id as usize] = false;

    connection.team = match menu_button {
        1 => Team::Goldmen,
        2 => Team::Monsota,
        3 => Team::OXS,
        _ => Team::Spectator
    };

    connection.update_player(state);
}