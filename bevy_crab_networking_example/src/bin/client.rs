//NOTE

//This example is not well written
//It is convoluted and more confusing than it needs to be
//It still remains useful in terms of how it uses parts of bevy_crab_networking

use bevy::{
    app::AppExit,
    color::palettes::css::{RED, YELLOW},
    ecs::system::RunSystemOnce,
    prelude::*,
    sprite::{MaterialMesh2dBundle, Mesh2dHandle},
};
use bevy_crab_networking::{
    connect_to_server, disconnect_from_server, is_connected_to_server, AutoReconnect,
    BevyCrabNetworkingPlugin, ClientConfig, ClientDataReadEvent, ClientDataUploader,
};
use bevy_crab_networking_example::{Packet, PlayerType, Players, DATA_UPLOAD_SPEED};
#[derive(Event)]
struct UpdateText(String);
#[derive(Event)]
struct SpawnPlayer {
    player_type: PlayerType,
    is_main: bool,
}
#[derive(Component)]
struct MainPlayer;
#[derive(Component)]
struct MainText;
#[derive(Resource)]
struct HasChoice(bool);
#[derive(Resource)]
struct DataUploadTimer(Timer);
fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(BevyCrabNetworkingPlugin)
        .add_event::<UpdateText>()
        .add_event::<SpawnPlayer>()
        .insert_resource(ClientConfig {
            server_address: "127.0.0.1:2942".parse().unwrap(),
            auto_reconnect: AutoReconnect::None,
        })
        .insert_resource(DataUploadTimer(Timer::from_seconds(
            1. / DATA_UPLOAD_SPEED,
            TimerMode::Repeating,
        )))
        .insert_resource(HasChoice(false))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (handle_incoming_data, handle_input, send_position).run_if(is_connected_to_server),
        )
        .add_systems(Update, (update_text, spawn_players))
        .run();
}
fn setup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
    commands.spawn((
        Text2dBundle {
            text: Text::from_section(
                "",
                TextStyle {
                    font_size: 30.,
                    ..Default::default()
                },
            ),
            ..Default::default()
        },
        MainText,
    ));
    commands.add(|w: &mut World| {
        if let Err(connection_error) = w.run_system_once(connect_to_server){
            let error_text = format!("Encountered an error while trying to connect to the server!: {connection_error:#?}");
            eprintln!("{}",error_text.clone());
            let mut events = w.resource_mut::<Events<UpdateText>>();
            events.send(UpdateText(error_text));
        }
    });
}
fn update_text(
    mut update_text_reader: EventReader<UpdateText>,
    mut text_query: Query<&mut Text, With<MainText>>,
) {
    let mut text = text_query.single_mut();
    let text_string = &mut text.sections[0].value;
    for event in update_text_reader.read() {
        *text_string = event.0.clone();
        break;
    }
    text.sections[0].style = TextStyle {
        font_size: 60. / (text_string.len() as f32).log10(),
        ..Default::default()
    };
}
fn spawn_players(
    mut spawn_player_reader: EventReader<SpawnPlayer>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for event in spawn_player_reader.read() {
        let spawned_player = commands
            .spawn(MaterialMesh2dBundle {
                mesh: Mesh2dHandle(meshes.add(Rectangle::new(100.0, 100.0))),
                material: materials.add(match event.player_type {
                    PlayerType::Red => Color::from(RED),
                    PlayerType::Yellow => Color::from(YELLOW),
                }),
                transform: Transform::from_xyz(0., 0., 0.),
                ..default()
            })
            .with_children(|parent| {
                parent.spawn(Text2dBundle {
                    text: Text::from_section(
                        &format!(
                            "Player {}",
                            match event.player_type {
                                PlayerType::Yellow => "1",
                                PlayerType::Red => "2",
                            }
                        ),
                        TextStyle {
                            font_size: 35.,
                            ..Default::default()
                        },
                    ),
                    transform: Transform::from_xyz(0., 70., 1.),
                    ..Default::default()
                });
            })
            .id();
        if event.is_main {
            commands.entity(spawned_player).insert(MainPlayer);
            println!("Spawned main_player");
        } else {
            println!("Spawned other player");
        }
    }
}
fn handle_input(
    mut main_player_transform: Query<&mut Transform, With<MainPlayer>>,
    mut exit: EventWriter<AppExit>,
    mut spawn_player_writer: EventWriter<SpawnPlayer>,
    mut update_text_writer: EventWriter<UpdateText>,
    mut has_choice: ResMut<HasChoice>,
    mut data_uploader: ResMut<ClientDataUploader>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
) {
    if has_choice.0 {
        let mut player_choice: Option<PlayerType> = None;
        if keyboard_input.just_pressed(KeyCode::Digit1) {
            player_choice = Some(PlayerType::Yellow);
        }
        if keyboard_input.just_pressed(KeyCode::Digit2) {
            player_choice = Some(PlayerType::Red);
        }
        if let Some(player_type) = player_choice {
            spawn_player_writer.send(SpawnPlayer {
                player_type: player_type.clone(),
                is_main: true,
            });
            update_text_writer.send(UpdateText("".into()));
            has_choice.0 = false;
            if let Err(err) = data_uploader.upload(Packet::PlayerChoice(player_type)) {
                eprintln!("Failed to send choice: {err:#?}");
            }
        }
    }
    let mut main_player_transform = match main_player_transform.get_single_mut() {
        Ok(main_player_transform) => main_player_transform,
        Err(_) => return,
    };
    let mut x_axis = 0;
    let mut y_axis = 0;
    if keyboard_input.pressed(KeyCode::KeyA) {
        x_axis -= 1;
    }
    if keyboard_input.pressed(KeyCode::KeyD) {
        x_axis += 1;
    }
    if keyboard_input.pressed(KeyCode::KeyW) {
        y_axis += 1;
    }
    if keyboard_input.pressed(KeyCode::KeyS) {
        y_axis -= 1;
    }
    main_player_transform.translation.x += x_axis as f32 * time.delta_seconds() * 100.;
    main_player_transform.translation.y += y_axis as f32 * time.delta_seconds() * 100.;
    if keyboard_input.just_pressed(KeyCode::Escape) {
        exit.send(AppExit::Success);
    }
}
fn send_position(
    mut data_uploader: ResMut<ClientDataUploader>,
    mut data_upload_timer: ResMut<DataUploadTimer>,
    main_player: Query<&Transform, With<MainPlayer>>,
    time: Res<Time>,
) {
    if data_upload_timer.0.tick(time.delta()).just_finished() {
        if let Ok(main_player) = main_player.get_single() {
            if let Err(err) = data_uploader.upload(Packet::PlayerPosition(Vec2 {
                x: main_player.translation.x,
                y: main_player.translation.y,
            })) {
                eprintln!("Failed to send position data because of {err:#?}");
            }
        }
    }
}
fn handle_incoming_data(
    mut client_data_read_reader: EventReader<ClientDataReadEvent>,
    mut update_text_writer: EventWriter<UpdateText>,
    mut spawn_player_writer: EventWriter<SpawnPlayer>,
    mut has_choice: ResMut<HasChoice>,
    mut commands: Commands,
    mut other_player: Query<(Entity, &Mesh2dHandle, &mut Transform), Without<MainPlayer>>,
    main_player: Query<(Entity, &Mesh2dHandle), With<MainPlayer>>,
) {
    for event in client_data_read_reader.read() {
        if event.data_packet.identifier != 0 {
            continue;
        }
        match bincode::deserialize::<Packet>(&event.data_packet.bytes)
            .expect("Failed to deserialize Packet")
        {
            Packet::PlayersConnectedToServer(players) => {
                match players {
                    Players::Both => {
                        commands.add(|w: &mut World| {
                            match w.run_system_once(disconnect_from_server) {
                                Ok(()) => {
                                    println!("Successfully disconnected from server!")
                                }
                                Err(err) => {
                                    eprintln!("Couldn't disconnect from server: {err:#?}")
                                }
                            };
                        });
                        update_text_writer.send(UpdateText("Server is full".into()));
                        has_choice.0 = false;
                        if let Ok(other_player) = other_player.get_single() {
                            commands.entity(other_player.0).despawn_recursive();
                        }
                        if let Ok(main_player) = main_player.get_single() {
                            commands.entity(main_player.0).despawn_recursive();
                        }
                    }
                    Players::Single(player_type) => {
                        spawn_player_writer.send(SpawnPlayer {
                            player_type: player_type.opposite(),
                            is_main: true,
                        });
                        spawn_player_writer.send(SpawnPlayer {
                            player_type: player_type,
                            is_main: false,
                        });
                    }
                    Players::None => {
                        update_text_writer.send(UpdateText("Press 1 or 2 to pick a player".into()));
                        has_choice.0 = true;
                    }
                };
            }
            Packet::PlayerConnected {
                player_type,
                is_server_full,
            } => {
                if is_server_full {
                    match main_player.get_single() {
                        Ok(_) => {
                            spawn_player_writer.send(SpawnPlayer {
                                player_type: player_type,
                                is_main: false,
                            });
                        }
                        Err(_) => {
                            update_text_writer.send(UpdateText("Server is full".into()));
                            has_choice.0 = false;
                            if let Ok(other_player) = other_player.get_single() {
                                commands.entity(other_player.0).despawn_recursive();
                            }
                        }
                    }
                } else {
                    spawn_player_writer.send(SpawnPlayer {
                        player_type: player_type,
                        is_main: false,
                    });
                }
            }
            Packet::PlayerDisconnected => {
                match other_player.get_single() {
                    Ok(other_player) => {
                        commands.entity(other_player.0).despawn_recursive();
                    }
                    Err(err) => println!("Getting other player failed: {err:#?}"),
                };
            }
            Packet::PlayerPosition(position) => {
                if let Ok((_, _, mut other_player_transform)) = other_player.get_single_mut() {
                    other_player_transform.translation.x = position.x;
                    other_player_transform.translation.y = position.y;
                }
            }
            _ => {}
        }
    }
}
