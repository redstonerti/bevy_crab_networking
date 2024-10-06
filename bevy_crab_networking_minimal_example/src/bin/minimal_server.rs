use bevy::{ecs::system::RunSystemOnce, prelude::*};
use bevy_crab_networking::{host_server, BevyCrabNetworkingPlugin, Recipient};
use bevy_crab_networking::{ServerConfig, ServerDataReadEvent, ServerDataUploader};
use bevy_crab_networking_minimal_example::{
    BevyNetworkingTestLibPlugin, MessageUploadTimer, Packet,
};

struct BevyNetworkingTestServerPlugin;
impl Plugin for BevyNetworkingTestServerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ServerConfig::default())
            .add_systems(Startup, setup)
            .add_systems(Update, (send_messages, handle_incoming_data));
    }
}
fn main() {
    App::new()
        .add_plugins((
            MinimalPlugins,
            BevyNetworkingTestServerPlugin,
            BevyNetworkingTestLibPlugin,
            BevyCrabNetworkingPlugin,
        ))
        .run();
}
fn send_messages(
    mut data_uploader: ResMut<ServerDataUploader>,
    mut message_upload_timer: ResMut<MessageUploadTimer>,
    time: Res<Time>,
) {
    if message_upload_timer.0.tick(time.delta()).just_finished() {
        data_uploader.upload(
            Packet::Message(format!(
                "It has been {:.2} seconds since the start of the program.",
                time.elapsed_seconds()
            )),
            Recipient::All,
        );
    }
}
fn setup(world: &mut World) {
    world.run_system_once(host_server).unwrap();
}
fn handle_incoming_data(
    mut server_data_reader: EventReader<ServerDataReadEvent>,
    mut data_uploader: ResMut<ServerDataUploader>,
) {
    for event in server_data_reader.read() {
        match event.data_packet.identifier {
            0 => {
                match bincode::deserialize::<Packet>(&event.data_packet.bytes)
                    .expect("Failed to deserialize packet")
                {
                    Packet::Message(message) => {
                        println!(
                            " Received a message from id: {}. Message: {}",
                            event.id,
                            message.clone()
                        );
                        data_uploader.upload(
                            Packet::Message(message),
                            Recipient::AllExcept { id: event.id },
                        );
                    }
                }
            }
            _ => {}
        }
    }
}
