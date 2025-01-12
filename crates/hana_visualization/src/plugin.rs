use bevy::prelude::App;
use bevy::prelude::Local;
use bevy::prelude::Plugin;
use bevy::prelude::Res;
use bevy::prelude::Resource;
use bevy::prelude::Update;

use hana_network::{Command, Result};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct HanaPlugin;

impl Plugin for HanaPlugin {
    fn build(&self, app: &mut App) {
        // Channel for sending commands from network thread to Bevy app
        let (tx, rx) = channel();
        let rx = Arc::new(Mutex::new(rx));

        let listener = TcpListener::bind("127.0.0.1:3001").expect("Failed to bind to port");
        println!("Listening on port 3001");

        // Clone sender for the network thread
        let network_tx = tx.clone();

        thread::spawn(move || {
            println!("Network listener thread started");
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        if let Err(e) = handle_connection(stream, network_tx.clone()) {
                            eprintln!("Connection error: {}", e);
                        }
                    }
                    Err(e) => eprintln!("Connection failed: {}", e),
                }
            }
        });
        app.add_systems(Update, handle_commands)
            .insert_resource(CommandReceiver(rx));
    }
}
#[derive(Resource)]
struct CommandReceiver(Arc<Mutex<std::sync::mpsc::Receiver<Command>>>);

fn handle_commands(receiver: Res<CommandReceiver>, mut count: Local<u32>) {
    if let Ok(rx) = receiver.0.lock() {
        while let Ok(command) = rx.try_recv() {
            match command {
                Command::Ping => println!("rx ping!"),
                Command::Stop => {
                    println!("Final count received: {}", *count);
                    std::process::exit(0);
                }
                Command::Count(_) => {
                    *count += 1;
                    if *count % 100 == 0 {
                        println!("rx {} counts", *count);
                    }
                }
            }
        }
    }
}

fn handle_connection(mut stream: TcpStream, tx: Sender<Command>) -> Result<()> {
    println!("New connection established!");

    while let Ok(Some(command)) = hana_network::read_command(&mut stream) {
        match command {
            Command::Stop | Command::Ping => {
                println!("Received TCP: {:?}", command);
            }
            _ => {}
        }

        if let Err(e) = tx.send(command) {
            eprintln!("Failed to send command to app: {}", e);
            break;
        }
    }

    println!("Connection closed");
    Ok(())
}
