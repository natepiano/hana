use crate::transport::{TransportConnector, TransportListener};
use std::error::Error as StdError;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Test utility for IPC transport implementations.
pub async fn test_ipc_transport<L, C, T>(
    listener: L,
    connector: C,
) -> Result<(), Box<dyn StdError + Send + Sync>>
where
    L: TransportListener<Transport = T>,
    C: TransportConnector<Transport = T>,
    T: AsyncReadExt + AsyncWriteExt + Unpin,
{
    // Start accepting a connection, but don't await it yet
    let accept_fut = listener.accept();

    // Give a brief moment to ensure accept is started
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Create client connection
    let mut client_transport = connector
        .connect()
        .await
        .map_err(|e| format!("Connect failed: {e}"))?;

    // Now await the accepted connection
    let mut server_transport = accept_fut
        .await
        .map_err(|e| format!("Accept failed: {e}"))?;

    // Send a message from client to server
    client_transport.write_all(b"hello").await?;

    // Server receives the message
    let mut buf = [0u8; 5];
    server_transport.read_exact(&mut buf).await?;

    // Verify the message
    assert_eq!(&buf, b"hello");

    // Server replies
    server_transport.write_all(b"world").await?;

    // Client reads the response
    let mut response = [0u8; 5];
    client_transport.read_exact(&mut response).await?;

    // Verify the response
    assert_eq!(&response, b"world");

    Ok(())
}
