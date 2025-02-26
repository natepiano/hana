use serde::{Deserialize, Serialize};

/// Messages that can be sent over the Hana network
pub trait HanaMessage: Serialize + for<'de> Deserialize<'de> {}

/// Define sender capability for a specific message type
pub trait Sender<M: HanaMessage> {}

/// Define receiver capability for a specific message type
pub trait Receiver<M: HanaMessage> {}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum Instruction {
    Ping,
    Shutdown,
}

impl HanaMessage for Instruction {}

/// Define who can send/receive Instructions
// this is extensible - whatever other types of messages you want
// to send and receive you can specify what type of message
// and what role can send and receive them
impl Sender<Instruction> for super::endpoint::HanaApp {}
impl Receiver<Instruction> for super::endpoint::Visualization {}
