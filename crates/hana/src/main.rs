use hana_network::{Command, Result};
use std::net::TcpStream;
use std::process::Command as ProcessCommand;
use std::time::Duration;

fn main() -> Result<()> {
    // Launch visualization
    let mut child = ProcessCommand::new("./target/debug/basic-visualization").spawn()?;

    // Try to connect with retries
    let mut attempts = 0;
    let max_attempts = 15;
    let mut stream = loop {
        match TcpStream::connect("127.0.0.1:3001") {
            Ok(stream) => break stream,
            Err(e) => {
                attempts += 1;
                if attempts >= max_attempts {
                    return Err(e.into());
                }
                println!("Connection attempt {} failed, retrying...", attempts);
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    };

    println!("Connected to visualization!");

    println!("Starting count command flood...");
    for i in 0..1000 {
        hana_network::write_command(&mut stream, &Command::Count(i))?;
    }
    println!("Finished sending counts");

    std::thread::sleep(std::time::Duration::from_secs(2));
    hana_network::write_command(&mut stream, &Command::Stop)?;

    child.wait()?;
    Ok(())
}
