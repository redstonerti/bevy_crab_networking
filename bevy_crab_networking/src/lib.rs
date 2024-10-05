use bevy::{ecs::system::RunSystemOnce, prelude::*, tasks::IoTaskPool, utils::HashMap};
use bevy_crossbeam_event::{CrossbeamEventApp, CrossbeamEventSender};
use bincode::ErrorKind;
use serde::{Deserialize, Serialize};
use std::{
    fmt::Debug,
    io::Write,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tokio::{io::AsyncReadExt, runtime::Builder};
const CHUNK_SIZE: usize = 128;
#[derive(Debug)]
pub enum ConnectionError {
    MissingClientConfig,
    MissingServerConfig,
    NotConnectedToServer,
    AlreadyConnectedToServer,
    AlreadyHosting,
    ConnectionRefused,
    ConnectionReset,
    FailedToSerializeData(Box<ErrorKind>),
    FailedToSendData(std::io::Error),
    TcpErr(std::io::Error),
    BincodeErr(bincode::Error),
}
#[derive(Event, Clone)]
pub struct ServerConnectionChangeEvent {
    pub connection_change: ConnectionChange,
}
#[derive(Clone)]
pub enum ConnectionChange {
    Connected,
    Disconnected,
}
#[derive(Event, Clone, Debug)]
pub struct ClientDataReadEvent {
    pub data_packet: DataPacket,
}
#[derive(Event, Clone, Debug)]
pub struct ServerDataReadEvent {
    pub data_packet: DataPacket,
    pub id: u32,
}
#[derive(Event, Clone)]
pub struct PlayerIntergressEvent {
    pub id: u32,
    pub intergress_type: IntergressType,
}
#[derive(Resource)]
pub struct ServerConfig {
    pub host_port: u16,
}
impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig { host_port: 46393 }
    }
}
#[derive(Resource)]
pub struct ClientConfig {
    pub server_address: SocketAddr,
    pub auto_reconnect: AutoReconnect,
}
impl Default for ClientConfig {
    fn default() -> Self {
        ClientConfig {
            server_address: "127.0.0.1:46393".parse().unwrap(),
            auto_reconnect: AutoReconnect::Auto {
                reconnection_time: 5.,
            },
        }
    }
}
pub enum AutoReconnect {
    Auto { reconnection_time: f32 },
    None,
}
#[derive(Clone, Debug)]
pub struct DataPacket {
    pub identifier: u32,
    pub bytes: Vec<u8>,
}
#[derive(Resource)]
pub struct ServerStreams {
    pub streams: Arc<Mutex<HashMap<u32, TcpStream>>>,
}
#[derive(Resource)]
pub struct ClientStream {
    pub stream: Arc<Mutex<Option<TcpStream>>>,
}
#[derive(Resource, Debug)]
pub struct ClientDataUploader {
    pub list: Option<Vec<DataPacket>>,
}
#[derive(Resource)]
pub struct ServerDataUploader {
    pub list: Vec<(Recipient, DataPacket)>,
}
#[derive(Resource)]
struct ReconnectTimer(Timer);
#[derive(Resource)]
pub struct ClientReadStopFlag(Arc<AtomicBool>);

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum IntergressType {
    Joined,
    Left,
}
pub enum Recipient {
    All,
    AllExcept { id: u32 },
    Single { id: u32 },
}
enum StreamEndpoint {
    Client {
        client_data_read_sender: CrossbeamEventSender<ClientDataReadEvent>,
        server_connection_change_sender: CrossbeamEventSender<ServerConnectionChangeEvent>,
        client_stream: Arc<Mutex<Option<TcpStream>>>,
        stop_flag: Arc<AtomicBool>,
    },
    Server {
        id: u32,
        server_streams: Arc<Mutex<HashMap<u32, TcpStream>>>,
        server_data_read_sender: CrossbeamEventSender<ServerDataReadEvent>,
        player_intergress_sender: CrossbeamEventSender<PlayerIntergressEvent>,
    },
}
pub trait Identify {
    fn get_identifier(&self) -> u32;
}
impl ClientDataUploader {
    pub fn upload<T: Serialize + Identify + Debug>(
        &mut self,
        data: T,
    ) -> Result<(), ConnectionError> {
        let bytes = bincode::serialize(&data);
        match bytes {
            Ok(bytes) => match &mut self.list {
                Some(list) => {
                    list.push(DataPacket {
                        identifier: data.get_identifier(),
                        bytes,
                    });
                    return Ok(());
                }
                None => {
                    println!("There is no connection to a server, and thus no data can be sent");
                    return Err(ConnectionError::NotConnectedToServer);
                }
            },
            Err(err) => {
                println!("Serialization failed: {err:#?}. Didn't send {data:#?}");
                return Err(ConnectionError::BincodeErr(err));
            }
        };
    }
    pub fn is_connected(&self) -> bool {
        self.list.is_some()
    }
}
impl ServerDataUploader {
    pub fn upload<T: Serialize + Identify + Debug>(&mut self, data: T, recipient: Recipient) {
        let bytes = bincode::serialize(&data);
        match bytes {
            Ok(bytes) => {
                self.list.push((
                    recipient,
                    DataPacket {
                        identifier: data.get_identifier(),
                        bytes,
                    },
                ));
            }
            Err(err) => {
                println!("Serialization failed: {err:#?}. Didn't send {data:#?}");
            }
        }
    }
}
pub struct BevyCrabNetworkingPlugin;
impl Plugin for BevyCrabNetworkingPlugin {
    fn build(&self, app: &mut App) {
        app.add_crossbeam_event::<ClientDataReadEvent>()
            .add_crossbeam_event::<ServerDataReadEvent>()
            .add_crossbeam_event::<PlayerIntergressEvent>()
            .add_crossbeam_event::<ServerConnectionChangeEvent>()
            .add_systems(PreStartup, client_specific_setup.run_if(has_client_config))
            .add_systems(PreStartup, server_specific_setup.run_if(has_server_config))
            .add_systems(Startup, setup)
            .add_systems(Update, reconnect.run_if(has_client_config))
            .add_systems(Update, send_data_to_server.run_if(is_connected_to_server))
            .add_systems(Update, send_data_to_clients.run_if(has_server_config));
    }
}
fn client_specific_setup(mut commands: Commands) {
    commands.insert_resource(ClientDataUploader { list: None });
    commands.insert_resource(ClientStream {
        stream: Arc::new(Mutex::new(None)),
    });
}
fn server_specific_setup(mut commands: Commands) {
    commands.insert_resource(ServerDataUploader { list: vec![] });
    commands.insert_resource(ServerStreams {
        streams: Arc::new(Mutex::new(HashMap::new())),
    });
}
fn has_client_config(resource: Option<Res<ClientConfig>>) -> bool {
    resource.is_some()
}
fn has_server_config(resource: Option<Res<ServerConfig>>) -> bool {
    resource.is_some()
}
pub fn is_connected_to_server(data_uploader: Option<Res<ClientDataUploader>>) -> bool {
    match data_uploader {
        Some(data_uploader) => {
            return data_uploader.list.is_some();
        }
        None => false,
    }
}
pub fn disconnect_from_server(
    client_config: Option<Res<ClientConfig>>,
    client_read_stop_flag: Option<ResMut<ClientReadStopFlag>>,
) -> Result<(), ConnectionError> {
    if let None = client_config {
        return Err(ConnectionError::MissingClientConfig);
    }
    if let Some(client_read_stop_flag) = client_read_stop_flag {
        client_read_stop_flag.0.store(true, Ordering::Relaxed);
    }
    Ok(())
}
fn reconnect(
    mut commands: Commands,
    mut server_connection_change_reader: EventReader<ServerConnectionChangeEvent>,
    mut client_data_uploader: ResMut<ClientDataUploader>,
    reconnect_timer: Option<ResMut<ReconnectTimer>>,
    client_config: Res<ClientConfig>,
    time: Res<Time>,
) {
    for connection_change_event in server_connection_change_reader.read() {
        match connection_change_event.connection_change {
            ConnectionChange::Connected => {
                commands.remove_resource::<ReconnectTimer>();
            }
            ConnectionChange::Disconnected => {
                client_data_uploader.list = None;
                if let AutoReconnect::Auto { reconnection_time } = client_config.auto_reconnect {
                    println!(
                        "Server connection has been reset. Reconnection attempt in {} seconds...",
                        reconnection_time
                    );
                    commands.insert_resource(ReconnectTimer(Timer::from_seconds(
                        reconnection_time,
                        TimerMode::Repeating,
                    )));
                }
            }
        }
    }
    if let Some(mut reconnect_timer) = reconnect_timer {
        if reconnect_timer.0.tick(time.delta()).just_finished() {
            println!("Attempting to reconnect...");
            commands.add(|w: &mut World| {
                let connection_error = w.run_system_once(connect_to_server);
                match connection_error{
                    Err(ConnectionError::ConnectionRefused)=>{println!("Unable to connect to the server because it refused the connection attempt.")},
                    Err(err)=>{eprintln!("Encountered an error while trying to connect to the server!: {err:#?}")},
                    _=>{}
                }
            });
        }
    }
}
fn setup(client_config: Option<Res<ClientConfig>>, server_config: Option<Res<ServerConfig>>) {
    if let Some(_) = client_config {
        if let Some(_) = server_config {
            panic!("Both client and server configs have been found! The Application cannot be both a client and a server at the same time!");
        }
    }
    if let None = client_config {
        if let None = server_config {
            panic!("Both the Client Config and Server Config have not been set! Please insert one as a resource at the build stage of your app");
        }
    }
}
fn send_data_to_clients(
    server_streams: Res<ServerStreams>,
    mut server_data_uploader: ResMut<ServerDataUploader>,
) {
    let mut owned_list = vec![];
    std::mem::swap(&mut server_data_uploader.list, &mut owned_list);
    for (recipient, data_packet) in owned_list {
        match recipient {
            Recipient::All => {
                for (_, stream) in server_streams.streams.lock().unwrap().iter() {
                    if let Err(err) = send_data(data_packet.clone(), stream) {
                        println!("Received an error trying to send data packet: {err:#?}");
                    }
                }
            }
            Recipient::AllExcept { id } => {
                for (key, stream) in server_streams.streams.lock().unwrap().iter() {
                    if *key != id {
                        if let Err(err) = send_data(data_packet.clone(), stream) {
                            println!("Received an error trying to send data packet: {err:#?}");
                        }
                    }
                }
            }
            Recipient::Single { id } => match server_streams.streams.lock().unwrap().get(&id) {
                Some(stream) => {
                    if let Err(err) = send_data(data_packet, stream) {
                        println!("Received an error trying to send data packet: {err:#?}");
                    }
                }
                None => {
                    eprintln!("Couldn't find the id {id}'s corresponding stream to send the data!");
                }
            },
        }
    }
}
fn send_data_to_server(
    mut client_data_uploader: ResMut<ClientDataUploader>,
    client_stream: Res<ClientStream>,
) {
    let stream = client_stream.stream.lock().unwrap();
    let stream = stream.as_ref();
    if let Some(stream) = stream {
        let owned_list = client_data_uploader.list.take().unwrap();
        client_data_uploader.list = Some(vec![]);
        for data_packet in owned_list {
            if let Err(err) = send_data(data_packet, &stream) {
                println!("Received an error trying to send data packet: {err:#?}");
            }
        }
    }
}
pub fn connect_to_server(
    mut client_stream: ResMut<ClientStream>,
    mut client_data_uploader: ResMut<ClientDataUploader>,
    mut commands: Commands,
    client_config: Option<Res<ClientConfig>>,
    client_data_read_sender: Res<CrossbeamEventSender<ClientDataReadEvent>>,
    server_connection_change_sender: Res<CrossbeamEventSender<ServerConnectionChangeEvent>>,
) -> Result<(), ConnectionError> {
    match client_config {
        Some(client_config) => match TcpStream::connect(client_config.server_address) {
            Ok(stream) => {
                if client_stream.stream.lock().unwrap().is_some() {
                    return Err(ConnectionError::AlreadyConnectedToServer);
                } else {
                    client_stream.stream = Arc::new(Mutex::new(Some(stream.try_clone().unwrap())));
                    let client_data_read_sender = client_data_read_sender.clone();
                    let server_connection_change_sender = server_connection_change_sender.clone();
                    let task_pool = IoTaskPool::get();
                    let client_stream = client_stream.stream.clone();
                    if let None = client_data_uploader.list {
                        client_data_uploader.list = Some(vec![]);
                    }
                    let stop_flag = Arc::new(AtomicBool::new(false));
                    let stop_flag_clone = stop_flag.clone();
                    commands.insert_resource(ClientReadStopFlag(stop_flag));
                    server_connection_change_sender.send(ServerConnectionChangeEvent {
                        connection_change: ConnectionChange::Connected,
                    });
                    task_pool
                        .spawn(async move {
                            let runtime =
                                Builder::new_current_thread().enable_all().build().unwrap();
                            runtime.block_on(async move {
                                println!("Successfully established connection with server!");
                                read_stream(
                                    stream,
                                    StreamEndpoint::Client {
                                        client_data_read_sender,
                                        client_stream,
                                        server_connection_change_sender,
                                        stop_flag: stop_flag_clone,
                                    },
                                )
                                .await;
                            });
                        })
                        .detach();
                }
                return Ok(());
            }
            Err(err) => {
                server_connection_change_sender.send(ServerConnectionChangeEvent {
                    connection_change: ConnectionChange::Disconnected,
                });
                return Err(ConnectionError::TcpErr(err));
            }
        },
        None => return Err(ConnectionError::MissingClientConfig),
    }
}
pub fn host_server(
    server_streams: Res<ServerStreams>,
    server_config: Option<Res<ServerConfig>>,
    server_data_read_sender: Res<CrossbeamEventSender<ServerDataReadEvent>>,
    player_intergress_sender: Res<CrossbeamEventSender<PlayerIntergressEvent>>,
) -> Result<(), ConnectionError> {
    match server_config {
        Some(server_config) => {
            match TcpListener::bind(format!("0.0.0.0:{}", server_config.host_port)) {
                Ok(listener) => {
                    let server_data_read_sender = server_data_read_sender.clone();
                    let player_intergress_sender = player_intergress_sender.clone();
                    let task_pool = IoTaskPool::get();
                    let server_streams = server_streams.streams.clone();
                    task_pool
                        .spawn(async move {
                            let mut current_id = 0u32;
                            let tokio_runtime =
                                Builder::new_multi_thread().enable_all().build().unwrap();
                            tokio_runtime.block_on(async move {
                                for stream in listener.incoming() {
                                    let stream = stream.unwrap();
                                    let server_data_read_sender = server_data_read_sender.clone();
                                    let player_intergress_sender = player_intergress_sender.clone();
                                    player_intergress_sender.send(PlayerIntergressEvent {
                                        id: current_id,
                                        intergress_type: IntergressType::Joined,
                                    });
                                    server_streams
                                        .lock()
                                        .unwrap()
                                        .insert(current_id, stream.try_clone().unwrap());
                                    let server_streams = server_streams.clone();
                                    tokio::spawn(async move {
                                        read_stream(
                                            stream,
                                            StreamEndpoint::Server {
                                                id: current_id,
                                                server_streams,
                                                server_data_read_sender,
                                                player_intergress_sender,
                                            },
                                        )
                                        .await;
                                    });
                                    current_id += 1;
                                }
                            });
                        })
                        .detach();
                    return Ok(());
                }
                Err(err) => return Err(ConnectionError::TcpErr(err)),
            }
        }
        None => return Err(ConnectionError::MissingServerConfig),
    }
}
fn handle_stream_error(
    error: std::io::Error,
    stream_endpoint: &StreamEndpoint,
) -> Result<(), ConnectionError> {
    match error.kind() {
        std::io::ErrorKind::ConnectionReset => {
            match stream_endpoint {
                StreamEndpoint::Client {
                    client_stream,
                    server_connection_change_sender,
                    ..
                } => {
                    *client_stream.lock().unwrap() = None;
                    server_connection_change_sender.send(ServerConnectionChangeEvent {
                        connection_change: ConnectionChange::Disconnected,
                    });
                }
                StreamEndpoint::Server {
                    id,
                    ref server_streams,
                    player_intergress_sender,
                    ..
                } => {
                    println!("Client with id {id}'s connection has been reset. Removing from stream hashmap");
                    match server_streams.lock().unwrap().remove_entry(id) {
                        Some(_) => println!("Removed stream with id: {id}"),
                        None => println!("No entry with id: {id} was found"),
                    }
                    player_intergress_sender.send(PlayerIntergressEvent {
                        id: *id,
                        intergress_type: IntergressType::Left,
                    });
                }
            }
            return Err(ConnectionError::ConnectionReset);
        }
        _ => {
            println!("Reader received an error: {error:#?}");
            return Ok(());
        }
    }
}
fn send_data_read_event(identifier: u32, bytes: Vec<u8>, stream_endpoint: &StreamEndpoint) {
    match stream_endpoint {
        StreamEndpoint::Server {
            id,
            ref server_data_read_sender,
            ..
        } => {
            server_data_read_sender.send(ServerDataReadEvent {
                data_packet: DataPacket { identifier, bytes },
                id: *id,
            });
        }
        StreamEndpoint::Client {
            ref client_data_read_sender,
            ..
        } => {
            client_data_read_sender.send(ClientDataReadEvent {
                data_packet: DataPacket { identifier, bytes },
            });
        }
    }
}
async fn read_stream(stream: TcpStream, stream_endpoint: StreamEndpoint) {
    stream
        .set_nonblocking(true)
        .expect("set_nonblocking call failed");
    let tokio_stream = tokio::net::TcpStream::from_std(stream).unwrap();
    let mut buf_reader = tokio::io::BufReader::new(tokio_stream);
    let mut header = [0u8; 8];
    let mut data_chunk = [0u8; CHUNK_SIZE];
    let mut bytes_left: i32;
    loop {
        if let StreamEndpoint::Client {
            ref stop_flag,
            ref server_connection_change_sender,
            ..
        } = stream_endpoint
        {
            if stop_flag.load(Ordering::Relaxed) {
                server_connection_change_sender.send(ServerConnectionChangeEvent {
                    connection_change: ConnectionChange::Disconnected,
                });
                break;
            }
        }
        let mut data: Vec<u8> = Vec::new();
        if let Err(err) = buf_reader.read_exact(&mut header).await {
            if let Err(err) = handle_stream_error(err, &stream_endpoint) {
                if let ConnectionError::ConnectionReset = err {
                    return;
                }
            }
            continue;
        }
        let (bytes_left_arr, identifier_arr) = header.split_at(4);
        bytes_left = combine_u8s_into_u32(bytes_left_arr.try_into().unwrap()) as i32;
        let identifier = combine_u8s_into_u32(identifier_arr.try_into().unwrap());
        loop {
            let bytes = if bytes_left >= CHUNK_SIZE as i32 {
                match buf_reader.read_exact(&mut data_chunk).await {
                    Ok(_) => CHUNK_SIZE,
                    Err(err) => {
                        if let Err(err) = handle_stream_error(err, &stream_endpoint) {
                            if let ConnectionError::ConnectionReset = err {
                                return;
                            }
                        }
                        continue;
                    }
                }
            } else {
                match buf_reader
                    .read_exact(&mut data_chunk[0..bytes_left as usize])
                    .await
                {
                    Ok(bytes) => bytes,
                    Err(err) => {
                        if let Err(err) = handle_stream_error(err, &stream_endpoint) {
                            if let ConnectionError::ConnectionReset = err {
                                return;
                            }
                        }
                        continue;
                    }
                }
            };
            data.extend_from_slice(&data_chunk[0..bytes]);
            bytes_left -= bytes as i32;
            if bytes_left == 0 {
                break;
            } else if bytes_left < 0 {
                panic!("Data bytes left was under 0: {}", bytes_left);
            }
        }
        send_data_read_event(identifier, data, &stream_endpoint);
    }
}
pub fn send_data(data_packet: DataPacket, mut stream: &TcpStream) -> Result<(), ConnectionError> {
    let bytes = data_packet.bytes;
    let identifier = data_packet.identifier;
    let packet_size = bytes.len() as u32;
    let packet_size_bytes = split_u32_into_u8s(packet_size);
    let mut packet: Vec<u8> = vec![];
    for byte in packet_size_bytes {
        packet.push(byte);
    }
    let identifier_bytes = split_u32_into_u8s(identifier);
    for byte in identifier_bytes {
        packet.push(byte);
    }
    packet.extend_from_slice(&bytes[..]);
    if let Err(err) = stream.write_all(&packet) {
        return Err(ConnectionError::FailedToSendData(err));
    }
    Ok(())
}
fn split_u32_into_u8s(input: u32) -> [u8; 4] {
    let byte1 = (input >> 24) as u8;
    let byte2 = ((input >> 16) & 0xFF) as u8;
    let byte3 = ((input >> 8) & 0xFF) as u8;
    let byte4 = (input & 0xFF) as u8;
    [byte1, byte2, byte3, byte4]
}
fn combine_u8s_into_u32(bytes: [u8; 4]) -> u32 {
    let byte1 = (bytes[0] as u32) << 24;
    let byte2 = (bytes[1] as u32) << 16;
    let byte3 = (bytes[2] as u32) << 8;
    let byte4 = bytes[3] as u32;
    byte1 | byte2 | byte3 | byte4
}
