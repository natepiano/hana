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
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| {
                    error_stack::Report::new(Error::CantStartNetworkRuntime)
                        .attach_printable(format!("Failed to create tokio runtime: {e}"))
                }) {
                Ok(runtime) => runtime,
                Err(err) => {
                    error!("{err}");
                    return;
                }
            };

            rt.block_on(async {
                match Self::run_network(tx).await {
                    Ok(()) => debug!("network connection closed normally"),
                    Err(report) => info!("Visualization running without hana network: {report}"),
                }
            });
        });

        Self { instruction_rx: rx }
    }
    // pub fn spawn() -> Self {
    //     let (tx, rx) = mpsc::channel(32);

    //     std::thread::spawn(move || {
    //         let rt = tokio::runtime::Builder::new_current_thread()
    //             .enable_all()
    //             .build()
    //             .expect("Failed to create tokio runtime");

    //         rt.block_on(async {
    //             match Self::run_network(tx).await {
    //                 Ok(()) => debug!("network connection closed normally"),
    //                 Err(report) => info!("Visualization running without hana network: {report}"),
    //             }
    //         });
    //     });

    //     Self { instruction_rx: rx }
    // }

    /// bind to the port and attempt to connect to listen for a hana app
    async fn run_network(tx: mpsc::Sender<Instruction>) -> Result<()> {
        info!("checking for hana app");
        match tokio::time::timeout(
            Duration::from_secs(5),
            VisualizationEndpoint::listen_for_hana(),
        )
        .await
        {
            Ok(Ok(endpoint)) => {
                info!("hana app connected successfully");
                Self::forward_instruction_to_channel(endpoint, tx).await
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

    async fn forward_instruction_to_channel(
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
                    debug!("hana app disconnected");
                    return Ok(());
                }
            }
        }
    }

    pub fn try_recv(&mut self) -> Option<Instruction> {
        self.instruction_rx.try_recv().ok()
    }
    // leaving this here for the future as we'll need to figure out what to do when we get an
    // instruction receive error pub fn try_recv(&mut self) -> Result<Option<Instruction>> {
    //     match self.instruction_rx.try_recv() {
    //         Ok(instruction) => Ok(Some(instruction)),
    //         Err(tokio::sync::mpsc::error::TryRecvError::Empty) => Ok(None),
    //         Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
    //             Err(error_stack::Report::new(Error::Channel)
    //                 .attach_printable("Instruction channel disconnected unexpectedly"))
    //         }
    //     }
    // }
}
