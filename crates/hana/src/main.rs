mod error;
mod visualization_process;

use error_stack::ResultExt;
use hana_network::Command;

use std::path::PathBuf;

use std::time::Duration;
use visualization_process::VisualizationProcess;

use error::{Error, Result};

fn main() -> Result<()> {
    let visualization_path = PathBuf::from("./target/debug/basic-visualization");
    let visualization = VisualizationProcess::new(visualization_path)?;

    let mut stream = visualization.connect()?;

    println!("Connected to visualization!");

    println!("Starting count command flood...");

    for i in 0..10000 {
        hana_network::write_command(&mut stream, &Command::Count(i))
            .change_context(Error::Network)
            .attach_printable_lazy(|| format!("Failed to send count command {}", i))?;
    }

    println!("Finished sending counts");

    std::thread::sleep(Duration::from_secs(5));

    // the shutdown asks the process to exit - this is a cooperative shutdown request
    hana_network::write_command(&mut stream, &Command::Shutdown)
        .change_context(Error::Network)
        .attach_printable("Failed to send stop command")?;

    // now we wait for shutdown but do we know that's cool?
    visualization.wait(Duration::from_secs(5))?;

    Ok(())
}
