mod error;

use error_stack::ResultExt;
use hana_network::Command;
use hana_process::Process;
use std::path::PathBuf;

use std::time::Duration;

use error::{Error, Result};

fn main() -> Result<()> {
    // todo : TypeState for Process for "unstarted", "running", "connected" i think shutdown is implicit
    //        move this into some helper function so main is pure
    let visualization_path = PathBuf::from("./target/debug/basic-visualization");
    let visualization = Process::run(visualization_path)
        .change_context(Error::Process)
        .attach_printable("Failed to start visualization process")?;

    let mut stream = visualization
        .connect()
        .change_context(Error::Network)
        .attach_printable("Failed to connect to visualization process")?;

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

    visualization
        .ensure_shutdown(Duration::from_secs(5))
        .change_context(Error::Process)
        .attach_printable("Failed to shutdown visualization process")?;

    Ok(())
}
