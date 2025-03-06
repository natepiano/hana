//! # Hana Network Library
//!
//! (there are many things left to implement - a list can be found [here](https://natepiano.github.io/hana/architecture/network.html))
//!
//! `hana_network` is a type-safe, role-based networking library for Hana applications that
//! provides a robust abstraction over different transport mechanisms.
//!
//! ## Key Features
//!
//! - **Role-Based Endpoints**: Type-safe network endpoints based on application roles, ensuring
//!   that only appropriate messages can be sent or received by specific role types.
//! - **Message System**: A flexible message passing system where message capabilities are bound to
//!   specific roles through the type system.
//! - **Multiple Transport Layers**: Support for different transport mechanisms (IPC, TCP) with a
//!   unified interface.
//! - **Platform Flexibility**: Automatic selection of appropriate IPC mechanism based on platform
//!   (Unix sockets on Linux/macOS, Named Pipes on Windows).
//!
//! ## Basic Usage
//!
//! The primary types you'll interact with are the role-based endpoints:
//!
//! ```rust,ignore
//! use hana_network::{HanaEndpoint, VisualizationEndpoint, Instruction};
//!
//! // Connect a Hana controller to a visualization
//! async fn controller_example() -> error_stack::Result<(), hana_network::Error> {
//!     // Connect to a visualization endpoint
//!     let mut endpoint = HanaEndpoint::connect_to_visualization().await?;
//!
//!     // Send an instruction (only available because HanaRole can send Instructions)
//!     endpoint.send(&Instruction::Ping).await?;
//!
//!     Ok(())
//! }
//!
//! // Accept connections from a Hana controller
//! async fn visualization_example() -> error_stack::Result<(), hana_network::Error> {
//!     // Listen for a connection from a Hana controller
//!     let mut endpoint = VisualizationEndpoint::listen_for_hana().await?;
//!
//!     // Receive an instruction (only available because VisualizationRole can receive Instructions)
//!     if let Some(instruction) = endpoint.receive::<Instruction>().await? {
//!         match instruction {
//!             Instruction::Ping => println!("Received ping"),
//!             Instruction::Shutdown => println!("Shutting down"),
//!         }
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Role-Based Message System
//!
//! The library uses Rust's type system to enforce which roles can send or receive specific
//! messages:
//!
//! ```rust,ignore
//! use hana_network::message::{HanaMessage, Sender, Receiver};
//! use serde::{Serialize, Deserialize};
//!
//! // Define a new message type
//! #[derive(Serialize, Deserialize, Debug)]
//! pub enum VisualizationStatus {
//!     Ready,
//!     Processing,
//! }
//!
//! // Mark it as a valid message type
//! impl HanaMessage for VisualizationStatus {}
//!
//! // Define which roles can send or receive this message
//! impl Sender<VisualizationStatus> for crate::role::VisualizationRole {}
//! impl Receiver<VisualizationStatus> for crate::role::HanaRole {}
//! ```
//!
//! ## Transport Layers
//!
//! The library automatically selects the appropriate transport mechanism based on the platform:
//!
//! - On Unix systems (Linux, macOS): Unix domain sockets
//! - On Windows: Named pipes
//!
//! You can also explicitly use TCP transport when needed:
//!
//! ```rust,ignore
//! use hana_network::transport::{TcpProvider, TransportProvider};
//!
//! async fn using_tcp_transport() -> error_stack::Result<(), hana_network::Error> {
//!     let tcp_listener = TcpProvider::listener().await?;
//!     // Use TCP for transport instead of the default
//!
//!     Ok(())
//! }
//! ```
mod endpoint;
mod error;
pub mod message;
mod prelude;
mod role;
mod transport;

pub use crate::endpoint::{HanaEndpoint, VisualizationEndpoint};
pub use crate::message::Instruction;
pub use crate::prelude::*;
