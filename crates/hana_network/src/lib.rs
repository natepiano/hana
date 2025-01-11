pub use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

mod error;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum Command {
    Count(u32),
    Ping,
    Stop,
}

pub fn write_command(stream: &mut impl Write, command: &Command) -> Result<()> {
    let command_bytes = bincode::serialize(command)?;
    let len_prefix = command_bytes.len() as u32;
    stream.write_all(&len_prefix.to_le_bytes())?;
    stream.write_all(&command_bytes)?;
    Ok(())
}

pub fn read_command(stream: &mut impl Read) -> Result<Option<Command>> {
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

#[cfg(test)]
mod write_tests {
    use super::*;
    use std::io::Write;
    use std::io::{Error as IoError, ErrorKind};

    struct FailingStream {
        error_kind: ErrorKind,
    }

    // mock stream that always fails on write
    impl Write for FailingStream {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(IoError::new(self.error_kind, "Mock IO error"))
        }

        // not relevant to the test
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_write_command_success() {
        let mut buffer = Vec::new();
        let command = Command::Ping;

        assert!(write_command(&mut buffer, &command).is_ok());
        assert!(!buffer.is_empty());
    }

    #[test]
    fn test_write_command_io_error() {
        let mut mock_stream = FailingStream {
            error_kind: ErrorKind::BrokenPipe,
        };
        let command = Command::Ping;

        // this has to fail so let's validate that it does
        match write_command(&mut mock_stream, &command) {
            Err(Error::Io(e)) => assert_eq!(e.kind(), ErrorKind::BrokenPipe),
            _ => panic!("Expected IO error"),
        }
    }

    #[test]
    fn test_write_command_length_prefix_error() {
        struct FailAfterNBytes {
            fail_after: usize,
            bytes_written: usize,
            write_calls: Vec<usize>, // Track size of each write
        }

        impl Write for FailAfterNBytes {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.write_calls.push(buf.len()); // Record the size of this write

                // we're just using BrokenPipe as an example
                if self.bytes_written >= self.fail_after {
                    Err(IoError::new(ErrorKind::BrokenPipe, "Mock IO error"))
                } else {
                    self.bytes_written += buf.len();
                    Ok(buf.len())
                }
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let mut mock_stream = FailAfterNBytes {
            fail_after: 4,
            bytes_written: 0,
            write_calls: Vec::new(),
        };
        let command = Command::Ping;

        let result = write_command(&mut mock_stream, &command);

        // Print the sizes of all write calls
        println!("Write calls: {:?}", mock_stream.write_calls);

        match result {
            Err(Error::Io(e)) => assert_eq!(e.kind(), ErrorKind::BrokenPipe),
            _ => panic!("Expected IO error"),
        }
    }

    #[test]
    fn test_write_command_correct_format() {
        let mut buffer = Vec::new();
        let command = Command::Count(42);

        write_command(&mut buffer, &command).unwrap();

        // First 4 bytes should be length prefix
        let len_bytes = &buffer[0..4];
        let len = u32::from_le_bytes(len_bytes.try_into().unwrap());

        // Remaining bytes should be serialized command
        let command_bytes = &buffer[4..];
        assert_eq!(command_bytes.len(), len as usize);

        // Should deserialize back to original command
        let deserialized: Command = bincode::deserialize(command_bytes).unwrap();
        assert!(matches!(deserialized, Command::Count(42)));
    }
}

#[cfg(test)]
mod read_tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_read_command_success() {
        // Create a valid serialized command
        let command = Command::Ping;
        let mut buffer = Vec::new();
        write_command(&mut buffer, &command).unwrap();

        let mut cursor = Cursor::new(buffer);
        let result = read_command(&mut cursor).unwrap();
        assert_eq!(result, Some(Command::Ping));
    }

    #[test]
    fn test_read_command_unexpected_eof() {
        // Create a cursor with incomplete data (length prefix only)
        let data = vec![4, 0, 0, 0]; // Length prefix (4 bytes)
        let mut cursor = Cursor::new(data);

        match read_command(&mut cursor) {
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => (), // Expected error
            other => panic!("Expected UnexpectedEof error, got {:?}", other),
        }
    }

    #[test]
    fn test_read_command_io_error() {
        struct FailingStream {
            error_kind: std::io::ErrorKind,
        }

        impl Read for FailingStream {
            fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::new(self.error_kind, "Mock IO error"))
            }
        }

        let mut mock_stream = FailingStream {
            error_kind: std::io::ErrorKind::Other,
        };

        match read_command(&mut mock_stream) {
            Err(Error::Io(e)) => assert_eq!(e.kind(), std::io::ErrorKind::Other),
            _ => panic!("Expected IO error"),
        }
    }

    #[test]
    fn test_read_command_deserialization_error() {
        // Create a cursor with invalid data
        let data = vec![
            4, 0, 0, 0, // Length prefix (4 bytes)
            0, 1, 2, 3, // Invalid command data
        ];
        let mut cursor = std::io::Cursor::new(data);

        match read_command(&mut cursor) {
            Err(Error::Serialization(_)) => (), // Success
            other => panic!("Expected serialization error, got {:?}", other),
        }
    }
}
