//! Implement standard AsyncRead and AsyncWrite traits for the appropriate type
//! So we don't have to write this twice for both tcp and unix sockets
#[macro_export]
macro_rules! impl_async_io {
    ($type:ty, $field:ident) => {
        impl tokio::io::AsyncRead for $type {
            fn poll_read(
                mut self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
                buf: &mut tokio::io::ReadBuf<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                std::pin::Pin::new(&mut self.$field).poll_read(cx, buf)
            }
        }

        impl tokio::io::AsyncWrite for $type {
            fn poll_write(
                mut self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
                buf: &[u8],
            ) -> std::task::Poll<std::io::Result<usize>> {
                std::pin::Pin::new(&mut self.$field).poll_write(cx, buf)
            }

            fn poll_flush(
                mut self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                std::pin::Pin::new(&mut self.$field).poll_flush(cx)
            }

            fn poll_shutdown(
                mut self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                std::pin::Pin::new(&mut self.$field).poll_shutdown(cx)
            }
        }
    };
}
