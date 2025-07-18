use crate::{app_state::AppState, connection::{menu::MenuTypes, ClientConnection}, packets::Team};

pub fn handle_lobby_menu_action(menu_button: u32, connection: &mut ClientConnection, state: &AppState) {
    {
        let ready = state.game_state.get_player_ready(connection.client_id);

        if menu_button == 5 {
            state.game_state.set_player_ready(connection.client_id, !ready);

            if !ready {
                connection.menu = MenuTypes::Empty
            }

            return;
        }
    }

    connection.team = match menu_button {
        1 => Team::Goldmen,
        2 => Team::Monsota,
        3 => Team::OXS,
        _ => Team::Spectator,
    };

    connection.update_player(state);
}
