use crate::{app_state::AppState, connection::{menu::menu_from_num, packets::{clientbound::initial_sync::ClientboundInitialSyncPacket, Encodable, GameState}, ClientConnection}};

pub fn parse_command(client: &mut ClientConnection, message: String, state: &AppState) -> bool {
    if !message.starts_with('/') {
        return false
    }

    let command = message.split(' ').collect::<Vec<&str>>()[0].replace('/', "");
    let args = message.split(' ').collect::<Vec<&str>>()[1..].to_vec().iter().map(|s| s.to_string()).collect::<Vec<String>>();

    match command.as_str() {
        "nick" => {
            client.username = args.join(" ").to_string();

            client.update_player(state);
        }

        "money" => {
            client.money = args.first().unwrap_or(&"10000".to_string()).parse::<i32>().unwrap_or(10000);

            client.update_money(state);
        }

        "state" => {
            let new = match args.first().unwrap_or(&"".to_string()).to_lowercase().as_str() {
                "ingame" => GameState::InGame,
                "intermission" => GameState::Intermission,
                "idle" => GameState::Idle,
                "restarting" => GameState::Restarting,
                _ => state.game_state()
            };

            let mut write = state.game_state.write().unwrap();
            write.state = new;
        }

        "menu" => {
            let menu_type = menu_from_num(args.first().unwrap_or(&"0".to_string()).parse::<u8>().unwrap_or(0));

            client.menu = menu_type;
        }

        "loadmap" => {
            let d = "test2".to_string();
            let map = args.first().unwrap_or(&d);

            {
                let mut name = state.map_name.write().unwrap();
                *name = map.to_string();

                drop(name)
            }

            let event = ClientboundInitialSyncPacket {
                round_number: state.round_number(),
                sun_angle: 0,
                sun_axial_tilt: 0,
                versus_movedelay: None,
                weekday: 0,
                weekly_enabled: false
            };

            state.broadcast(event.encode(state));
        }

        _ => return false
    }  

    true  
}