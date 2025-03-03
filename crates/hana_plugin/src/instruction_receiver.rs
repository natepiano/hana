use std::time::Duration;

use bevy::prelude::*;
use error_stack::ResultExt;
use hana_network::{Instruction, VisualizationEndpoint};
use tokio::sync::mpsc;
use tracing::debug;

use crate::error::{Error, Result};

#[derive(Resource)]
pub struct InstructionReceiver {
    instruction_rx: mpsc::Receiver<Instruction>,
}

impl InstructionReceiver {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel(32);

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create tokio runtime");

            rt.block_on(async {
                match Self::run_network(tx).await {
                    Ok(()) => debug!("network connection closed normally"),
                    Err(report) => info!("Visualization running without hana network: {report}"),
                }
            });
        });

        Self { instruction_rx: rx }
    }

    /// bind to the port and attempt to connect to listen for a hana app
    async fn run_network(tx: mpsc::Sender<Instruction>) -> Result<()> {
        info!("checking for hana app on port 3001");
        match tokio::time::timeout(
            Duration::from_secs(1),
            VisualizationEndpoint::listen_for_hana(),
        )
        .await
        {
            Ok(Ok(endpoint)) => {
                info!("hana app connected successfully");
                Self::handle_messages(endpoint, tx).await
            }
            Ok(Err(e)) => Err(e
                .change_context(Error::Network)
                .attach_printable("failed to accept hana app connection")),
            Err(_) => {
                info!("no hana app detected - visualization running standalone");
                Ok(())
            }
        }
    }

    async fn handle_messages(
        mut endpoint: VisualizationEndpoint,
        tx: mpsc::Sender<Instruction>,
    ) -> Result<()> {
        loop {
            match endpoint
                .receive()
                .await
                .change_context(Error::Network)
                .attach_printable("Failed to receive instruction")?
            {
                Some(instruction) => match instruction {
                    Instruction::Shutdown => {
                        debug!("Received shutdown instruction");
                        std::process::exit(0);
                    }
                    instruction => {
                        tx.send(instruction)
                            .await
                            .change_context(Error::Channel)
                            .attach_printable("Failed to forward instruction")?;
                    }
                },
                None => {
                    debug!("Controller disconnected");
                    return Ok(());
                }
            }
        }
    }

    pub fn try_recv(&mut self) -> Option<Instruction> {
        self.instruction_rx.try_recv().ok()
    }
}
