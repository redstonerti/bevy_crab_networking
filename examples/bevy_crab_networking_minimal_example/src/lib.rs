use bevy::prelude::*;
use bevy_crab_networking::Identify;
use serde::{Deserialize, Serialize};
const MESSAGE_UPLOAD_RATE: f32 = 2.;
#[derive(Serialize, Deserialize, Debug)]
pub enum Packet {
    Message(String),
}
impl Identify for Packet {
    fn get_identifier(&self) -> u32 {
        0
    }
}
#[derive(Resource)]
pub struct MessageUploadTimer(pub Timer);
pub struct BevyNetworkingTestLibPlugin;
impl Plugin for BevyNetworkingTestLibPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MessageUploadTimer(Timer::from_seconds(
            1. / MESSAGE_UPLOAD_RATE,
            TimerMode::Repeating,
        )));
    }
}
