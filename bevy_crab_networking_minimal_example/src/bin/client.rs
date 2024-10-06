use bevy::{ecs::system::RunSystemOnce, prelude::*};
use bevy_crab_networking::{
    connect_to_server, is_connected_to_server, BevyCrabNetworkingPlugin, ClientConfig,
    ClientDataReadEvent, ClientDataUploader,
};
use bevy_crab_networking_minimal_example::{
    BevyNetworkingTestLibPlugin, MessageUploadTimer, Packet,
};
struct BevyNetworkingTestClientPlugin;

impl Plugin for BevyNetworkingTestClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClientConfig::default())
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (send_messages, handle_incoming_data).run_if(is_connected_to_server),
            );
    }
}
fn main() {
    App::new()
        .add_plugins((
            MinimalPlugins,
            BevyNetworkingTestClientPlugin,
            BevyNetworkingTestLibPlugin,
            BevyCrabNetworkingPlugin,
        ))
        .run();
}

fn setup(mut commands: Commands) {
    commands.add(|w: &mut World| {
        if let Err(err) = w.run_system_once(connect_to_server) {
            eprintln!("Couldn't connect to the server: {err:#?}");
        }
    });
}
fn send_messages(
    mut client_data_uploader: ResMut<ClientDataUploader>,
    mut message_upload_timer: ResMut<MessageUploadTimer>,
    time: Res<Time>,
) {
    if message_upload_timer.0.tick(time.delta()).just_finished() {
        client_data_uploader
            .upload(Packet::Message(format!(
                "It has been {:.2} seconds since the start of the program.",
                time.elapsed_seconds()
            )))
            .unwrap();
    }
}
fn handle_incoming_data(mut client_data_reader: EventReader<ClientDataReadEvent>) {
    for event in client_data_reader.read() {
        match event.data_packet.identifier {
            0 => match bincode::deserialize::<Packet>(&event.data_packet.bytes)
                .expect("Failed to deserialize packet")
            {
                Packet::Message(message) => {
                    println!("Received message: {}", message);
                }
            },
            _ => {}
        }
    }
}
