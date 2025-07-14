use rapier3d::{na::vector, prelude::*};

use crate::{app_state::{AppState, ChatType}, connection::{menu::menu_from_num, packets::{clientbound::initial_sync::ClientboundInitialSyncPacket, Encodable, GameState}, ClientConnection}, items::{item_types::ItemType, Item}, world::{quaternion::Quaternion, vector::Vector}};

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

            let mut write = state.game_state.state.write().unwrap();
            *write = new;
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

            state.broadcast_packet(event.encode(state));
        }

        "campos" => {
            state.send_chat(ChatType::PrivateMessage, &format!("{:?}", client.camera_pos), client.client_id as i32, 0);
        }

        "spawn" => {
            Item::create(ItemType::Watermelon, Some((ColliderBuilder::capsule_y(0.20, 0.24).mass(900.0).restitution(1.0).friction(0.2).build(), RigidBodyBuilder::dynamic().translation(vector![client.camera_pos.x, client.camera_pos.y, client.camera_pos.z]).build())), &state);
        }

        "delete @e" => {
            for item in state.items.items.iter() {
                let id = item.item_id;
                drop(item);

                Item::destroy(id, state);
            }
        }

        "debugplayer" => {
            println!("Con {client:?}");

            if let Some(event) = state.events.players.get(&client.client_id) {
                println!("Eve {:?}", *event);
            }

            state.send_chat(ChatType::PrivateMessage, "Printed player struct to terminal.", client.client_id as i32, 0);
        }

        "car" => {
            let x = args.first().unwrap_or(&"0".to_string()).parse::<f32>().unwrap_or(1.0);
            let y = args.get(1).unwrap_or(&"0".to_string()).parse::<f32>().unwrap_or(1.0);
            let z = args.get(2).unwrap_or(&"0".to_string()).parse::<f32>().unwrap_or(1.0);

            let mut car = state.vehicles.vehicles.get_mut(&0).unwrap();

            car.transform.rot = Quaternion::euler(x, y, z).normalized();
        }

        "carrot" => {
            let car = state.vehicles.vehicles.get(&0).unwrap();

            println!("ROt {:?}", car.transform.rot);

            state.send_chat(ChatType::Announce, &format!("{:?}", car.transform.rot), -1, 0);
        }

        "valid" => {
            let car = state.vehicles.vehicles.get(&0).unwrap();

            state.send_chat(ChatType::Announce, &format!("{:?}", car.transform.rot.is_valid()), -1, 0);
        }

        _ => return false
    }  

    true  
}