use crate::{app_state::AppState, connection::ClientConnection, packets::Team};

pub fn handle_lobby_menu_action(menu_button: u32, connection: &mut ClientConnection, state: &AppState) {
    {
        let mut ready = state.game_state.ready.lock().unwrap();

        if menu_button == 5 {
            ready[connection.client_id as usize] = !ready[connection.client_id as usize];

            return;
        }

        ready[connection.client_id as usize] = false;
    }

    connection.team = match menu_button {
        1 => Team::Goldmen,
        2 => Team::Monsota,
        3 => Team::OXS,
        _ => Team::Spectator,
    };

    connection.update_player(state);
}
