use bevy::prelude::*;
use hana_network::{Command, Result};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

fn main() {
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

    println!("Starting Bevy app...");
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_resource(CommandReceiver(rx))
        .add_systems(Update, handle_commands)
        .add_systems(Startup, setup)
        .run();
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

/// set up a simple 3D scene
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // circular base
    commands.spawn((
        Mesh3d(meshes.add(Circle::new(4.0))),
        MeshMaterial3d(materials.add(Color::WHITE)),
        Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
    ));
    // cube
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
        MeshMaterial3d(materials.add(Color::srgb_u8(124, 144, 255))),
        Transform::from_xyz(0.0, 0.5, 0.0),
    ));
    // light
    commands.spawn((
        PointLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
    ));
    // camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-2.5, 4.5, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
