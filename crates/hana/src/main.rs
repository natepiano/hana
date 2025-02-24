mod error;

use std::path::PathBuf;
use std::time::Duration;

use error::{Error, Result};
use error_stack::ResultExt;
use hana_controller::{Unstarted, Visualization};
use hana_network::Instruction;

fn main() -> Result<()> {
    let viz_path = PathBuf::from("./target/debug/basic-visualization");

    // Create and connect visualization using typestate pattern
    let viz = Visualization::<Unstarted>::start(viz_path).change_context(Error::Controller)?;

    let mut viz = viz.connect().change_context(Error::Controller)?;

    // Send commands
    for i in 0..10000 {
        viz.send_instruction(&Instruction::Count(i))
            .change_context(Error::Controller)?;
    }

    std::thread::sleep(Duration::from_secs(5));

    // Shutdown
    viz.shutdown(Duration::from_secs(5))
        .change_context(Error::Controller)?;

    Ok(())
}
