mod tcp;

use crate::prelude::*;
use crate::transport::provider::*;

use tcp::*;

pub struct TcpProvider;

impl TransportProvider for TcpProvider {
    type Transport = TcpTransport;
    type Connector = TcpConnector;
    type Listener = TcpListener;

    fn connector() -> Result<Self::Connector> {
        Ok(TcpConnector::default())
    }

    async fn listener() -> Result<Self::Listener> {
        TcpListener::bind_default().await
    }
}
