# Bevy crab networking

Multiplayer made easy! ...easier.

This is a [Bevy](https://bevyengine.org/) networking plugin based on [TCP](https://en.wikipedia.org/wiki/Transmission_Control_Protocol). It makes hosting a server, connecting to it, and sending packets of data to and from as easy as pie! No wait, that's Python. Crabcake? Now that you know that this Readme and the crate weren't written by ChatGPT, let's get down to business.

_If you don't like reading my incoherent ramblings, here are some examples_

## Setup

To setup the plugin, add the `BevyNetworkingPlugin` to your plugins and if you're making a server, add the `ServerConfig` resource, and if you're making a client, add the `ClientConfig` resource.
Keep in mind that it is highly advised that both your client and server depend on a single library to avoid confusion and duplicate code.


### Client
```rust
fn main() {
    App::new()
        .add_plugins((DefaultPlugins, BevyCrabNetworkingPlugin))
        .insert_resource(ClientConfig {
            server_address: "127.0.0.1:46393".parse().unwrap(),
            auto_reconnect: AutoReconnect::Auto {
                reconnection_time: 5.,
            },
        })
        .run();
}
```
### Server
```rust
fn main() {
    App::new()
        .add_plugins((MinimalPlugins, BevyCrabNetworkingPlugin))
        .insert_resource(ServerConfig { host_port: 46393 })
        .run();
}
```
What is `AutoReconnect`? Well, if the client gets disconnected from the server for any reason, or never even manages to connect in the first place, it will try again and again, until it succeeds.
You can disable this whenever you like with `AutoReconnect::None`

## Sending Data

### lib.rs

Create a struct or enum that implements the traits `Identify`, `Serialize`, `Deserialze` and `Debug`. Identify is provided by the crate and is used to "tag" the packet with an identifier, so that you can match with it later
```rust
#[derive(Serialize, Deserialize, Debug)]
pub enum Packet {
    Message(String),
}
impl Identify for Packet {
    fn get_identifier(&self) -> u32 {
        0
    }
}
```

### client.rs

You can use `ClientDataUploader` to upload things from the client to the server. This will return an error if the client is not connected to the server, so in order to prevent panics, you can use the `is_connected` function to check if it is connected to the server, or you can run the function only when is_connected_to_server returns true

```rust
fn send_messages(mut client_data_uploader: ResMut<ClientDataUploader>) {
    if client_data_uploader.is_connected() {
        if let Err(err) = client_data_uploader.upload(Packet::Message("hello".into())) {
            eprintln!("Failed to send packet: {err:?}");
        }
    }
}
```
```rust
fn main() {
    App::new()
        .add_systems(Update, send_messages.run_if(is_connected_to_server))
        .run();
}
```

### server.rs

Similarly, you can use `ServerDataUploader` to upload things from the server to the client. This time, you need to specify a Recipient, which can be `All`, `AllExcept {id : u32}` or `Single {id : u32}`. The recipient specifies to what clients the Packet will be sent.

```rust
fn send_messages(mut data_uploader: ResMut<ServerDataUploader>) {
    data_uploader.upload(
        Packet::Message(format!("Hello to you too!",)),
        Recipient::All,
    );
}
```
There is no need to check anything this time.

## Receiving Data

## client.rs

Now that identifier that we specified with the Identify trait will come in handy!
You match the struct or enum with the identifier in the `DataPacket` from the `ClientDataReadEvent`, and then in the case of the `Packet` enum you created, you can match with that to see what kind of data you received.

```rust
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

```

## server.rs

Instead of a `ClientDataReadEvent`, you get a `ServerDataReadEvent`. The only difference is, that in addition to the `DataPacket`, you also get an `id: u32` that you can use as an id to tell players apart. That is the id of the player that sent you the `Packet`. You can also, for example, use it with the `Recipient` in `ServerDataUploader` to forward a message to every player except the one that sent you it.

```rust
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
```

## Compatible Bevy versions

| Bevy version | `bevy_crab_networking` version |
|:-------------|:----------------------------|
| `0.14`       | `0.1.0`                     |

## License

Licensed under the MIT license ([LICENSE-MIT](/LICENSE-MIT) or https://opensource.org/licenses/MIT)