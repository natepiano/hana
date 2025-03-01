use std::time::Duration;

use bevy::prelude::*;
use error_stack::ResultExt;
use hana_network::{Endpoint, Instruction, TcpTransport, Visualization};
use tokio::net::TcpListener;
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
    /// we time out after 1 second
    async fn run_network(tx: mpsc::Sender<Instruction>) -> Result<()> {
        let mut listener = TcpListener::bind("127.0.0.1:3001")
            .await
            .change_context(Error::Io)
            .attach_printable("failed to bind to port")?;

        info!("checking for hana app on port 3001");

        match tokio::time::timeout(
            Duration::from_secs(1),
            Self::accept_connection(&mut listener),
        )
        .await
        {
            Ok(Ok(transport_endpoint)) => {
                info!("hana app connected successfully");
                Self::handle_messages(transport_endpoint, tx).await
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

    // New method to accept connection using TcpTransport directly
    async fn accept_connection(
        listener: &mut TcpListener,
    ) -> error_stack::Result<Endpoint<Visualization, TcpTransport>, hana_network::Error> {
        let (stream, _) = listener
            .accept()
            .await
            .change_context(hana_network::Error::Io)
            .attach_printable("Failed to accept connection")?;

        Ok(Endpoint::new(TcpTransport::new(stream)))
    }

    async fn handle_messages(
        mut transport_endpoint: Endpoint<Visualization, TcpTransport>,
        tx: mpsc::Sender<Instruction>,
    ) -> Result<()> {
        loop {
            match transport_endpoint
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
