//NOTE

//This example is also not well written
//See client.rs to see why i said "also"
//It is convoluted and more confusing than it needs to be
//It still remains useful in terms of how it uses parts of bevy_crab_networking

use bevy::{ecs::system::RunSystemOnce, prelude::*};
use bevy_crab_networking::{
    host_server, BevyCrabNetworkingPlugin, IntergressType, PlayerIntergressEvent, Recipient,
    ServerConfig, ServerDataReadEvent, ServerDataUploader,
};
use bevy_crab_networking_example::{Packet, PlayerType, Players};
#[derive(Event)]
struct SpawnPlayer(PlayerType, u32);
#[derive(Event)]
struct DespawnPlayer(u32);
#[derive(Component, Clone)]
struct PlayerId(u32);
#[derive(Resource)]
struct PlayerIds {
    yellow_player_id: Option<u32>,
    red_player_id: Option<u32>,
}
fn main() {
    App::new()
        .add_plugins(MinimalPlugins)
        .add_plugins(BevyCrabNetworkingPlugin)
        .add_event::<SpawnPlayer>()
        .add_event::<DespawnPlayer>()
        .insert_resource(ServerConfig { host_port: 2942 })
        .insert_resource(PlayerIds {
            yellow_player_id: None,
            red_player_id: None,
        })
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                handle_incoming_data,
                handle_player_intergress,
                spawn_players,
                despawn_players,
            ),
        )
        .run();
}
fn setup(mut commands: Commands) {
    commands.add(|w: &mut World| {
        if let Err(connection_error) = w.run_system_once(host_server) {
            eprintln!("Encountered an error while trying to host server!: {connection_error:#?}");
        }
    });
}
fn handle_player_intergress(
    mut despawn_player_writer: EventWriter<DespawnPlayer>,
    mut player_intergress_reader: EventReader<PlayerIntergressEvent>,
    mut data_uploader: ResMut<ServerDataUploader>,
    player_ids: Res<PlayerIds>,
    players_query: Query<(&Transform, &PlayerType)>,
) {
    for event in player_intergress_reader.read() {
        match event.intergress_type {
            IntergressType::Joined => {
                let player_len = players_query.iter().len();
                let players: Players;
                if player_len == 0 {
                    players = Players::None;
                } else if player_len == 1 {
                    let (_, player_type) = players_query.single();
                    players = Players::Single(player_type.clone());
                    data_uploader.upload(
                        Packet::PlayerConnected {
                            player_type: player_type.opposite(),
                            is_server_full: false,
                        },
                        Recipient::AllExcept { id: event.id },
                    );
                } else {
                    players = Players::Both;
                }
                data_uploader.upload(
                    Packet::PlayersConnectedToServer(players.clone()),
                    Recipient::Single { id: event.id },
                );
            }
            IntergressType::Left => {
                if let Some(id) = player_ids.yellow_player_id {
                    if event.id == id {
                        data_uploader.upload(
                            Packet::PlayerDisconnected,
                            Recipient::AllExcept { id: event.id },
                        );
                    }
                }
                if let Some(id) = player_ids.red_player_id {
                    if event.id == id {
                        data_uploader.upload(
                            Packet::PlayerDisconnected,
                            Recipient::AllExcept { id: event.id },
                        );
                    }
                }
                despawn_player_writer.send(DespawnPlayer(event.id));
            }
        }
    }
}
fn despawn_players(
    mut despawn_player_reader: EventReader<DespawnPlayer>,
    mut commands: Commands,
    mut player_ids: ResMut<PlayerIds>,
    players_query: Query<(Entity, &PlayerId)>,
) {
    for event in despawn_player_reader.read() {
        for (entity, player_id) in players_query.iter() {
            if event.0 == player_id.0 {
                commands.entity(entity).despawn_recursive();
                if let Some(id) = player_ids.yellow_player_id {
                    if player_id.0 == id {
                        player_ids.yellow_player_id = None;
                    }
                }
                if let Some(id) = player_ids.red_player_id {
                    if player_id.0 == id {
                        player_ids.red_player_id = None;
                    }
                }
            }
        }
    }
}
fn spawn_players(
    mut spawn_player_reader: EventReader<SpawnPlayer>,
    mut commands: Commands,
    mut player_ids: ResMut<PlayerIds>,
) {
    for event in spawn_player_reader.read() {
        commands.spawn((Transform::default(), event.0.clone(), PlayerId(event.1)));
        match event.0 {
            PlayerType::Yellow => {
                player_ids.yellow_player_id = Some(event.1);
            }
            PlayerType::Red => {
                player_ids.red_player_id = Some(event.1);
            }
        }
    }
}
fn handle_incoming_data(
    mut server_data_read_reader: EventReader<ServerDataReadEvent>,
    mut spawn_player_writer: EventWriter<SpawnPlayer>,
    mut data_uploader: ResMut<ServerDataUploader>,
    players_query: Query<(&Transform, &PlayerType)>,
) {
    for event in server_data_read_reader.read() {
        if event.data_packet.identifier != 0 {
            continue;
        }
        match bincode::deserialize::<Packet>(&event.data_packet.bytes)
            .expect("Failed to deserialize Packet")
        {
            Packet::PlayerChoice(player_choice) => {
                println!("Received player choice: {player_choice:?}");
                spawn_player_writer.send(SpawnPlayer(player_choice.clone(), event.id));
                let is_server_full = players_query.iter().len() > 0;
                data_uploader.upload(
                    Packet::PlayerConnected {
                        player_type: player_choice.clone(),
                        is_server_full,
                    },
                    Recipient::AllExcept { id: event.id },
                );
            }
            Packet::PlayerPosition(position) => {
                data_uploader.upload(
                    Packet::PlayerPosition(position),
                    Recipient::AllExcept { id: event.id },
                );
            }
            _ => {}
        }
    }
}
