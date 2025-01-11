use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::TcpStream;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] bincode::Error),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Command {
    Count(u32),
    Ping,
    Stop,
}

pub type Result<T> = std::result::Result<T, NetworkError>;

pub fn write_command(stream: &mut TcpStream, command: &Command) -> Result<()> {
    let bytes = bincode::serialize(command)?;
    let len = bytes.len() as u32;
    stream.write_all(&len.to_le_bytes())?; // Write length prefix
    stream.write_all(&bytes)?; // Write command data
    Ok(())
}

pub fn read_command(stream: &mut TcpStream) -> Result<Option<Command>> {
    let mut len_bytes = [0u8; 4];
    match stream.read_exact(&mut len_bytes) {
        Ok(_) => {
            let len = u32::from_le_bytes(len_bytes) as usize;
            let mut buffer = vec![0u8; len];
            stream.read_exact(&mut buffer)?;
            Ok(Some(bincode::deserialize(&buffer)?))
        }
        Err(ref e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
        Err(e) => Err(e.into()),
    }
}
