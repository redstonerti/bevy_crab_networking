use bevy::{math::Vec2, prelude::Component};
use bevy_crab_networking::Identify;
use serde::{Deserialize, Serialize};
pub const DATA_UPLOAD_SPEED: f32 = 60.;
#[derive(Component, Clone, Serialize, Deserialize, Debug)]
pub enum PlayerType {
    Yellow,
    Red,
}
impl PlayerType {
    pub fn opposite(&self) -> PlayerType {
        match self {
            PlayerType::Red => PlayerType::Yellow,
            PlayerType::Yellow => PlayerType::Red,
        }
    }
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Players {
    Both,
    Single(PlayerType),
    None,
}
#[derive(Serialize, Deserialize, Debug)]
pub enum Packet {
    PlayerChoice(PlayerType),
    PlayerConnected {
        player_type: PlayerType,
        is_server_full: bool,
    },
    PlayerDisconnected,
    PlayerPosition(Vec2),
    PlayersConnectedToServer(Players),
}
impl Identify for Packet {
    fn get_identifier(&self) -> u32 {
        0
    }
}
