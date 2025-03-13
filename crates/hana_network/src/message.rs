//! Defines message types. You can create a Sender or a Receiver and attach it to any Role
//! this way you can control whether (for example) a Visualization can receive instructions
//! or (in the future) a Visualization can send a thumbnail
//! this creates an extensible format
//!
//! all you need to do is create a class of messages that can be sent or received is to
//! ```rust
//! # use hana_network::message::{HanaMessage, Sender, Receiver};
//! # use bincode::{Encode, Decode};
//! # mod role {
//! #     pub struct HanaApp;
//! #     pub struct Visualization;
//! # }
//!
//! // create a message type
//! #[derive(Serialize, Deserialize)]
//! pub enum VisualizationStatus {
//!     Visualizing,
//!     DoingOtherStuff,
//! }
//!
//! // impl HanaMessage for it
//! impl HanaMessage for VisualizationStatus {}
//!
//! // and then impl Sender and/or Receiver for this message on which ever role is allowed
//! // to handle this message - in this example, a visualization status update that can
//! // only be sent by the Visualization role to the HanaApp role
//! impl Receiver<VisualizationStatus> for role::HanaApp {}
//! impl Sender<VisualizationStatus> for role::Visualization {}
//! ```
//! other messages will be added in the future
//! this may require expanding messages into a module for ease of use
//!
//! in the future when we add other crates for logically separated functionality
//! we may need to include messages in them and possibly the message functionality
//! might make sense to move to its own crate - HanaMessage's are a form of documentation
//! for hana behavior. If we split this out, we should probably move Role with it
//! ass they are tightly coupled
use bincode::{Decode, Encode};

/// Messages that can be sent over the Hana network
pub trait HanaMessage: Encode + Decode<()> {}

/// Define sender capability for a specific message type
pub trait Sender<M: HanaMessage> {}

/// Define receiver capability for a specific message type
pub trait Receiver<M: HanaMessage> {}

#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub enum Instruction {
    Ping,
    Shutdown,
}

impl HanaMessage for Instruction {}

/// Define who can send/receive Instructions
// this is extensible - whatever other types of messages you want
// to send and receive you can specify what type of message
// and what role can send and receive them
impl Sender<Instruction> for super::role::HanaRole {}
impl Receiver<Instruction> for super::role::VisualizationRole {}
