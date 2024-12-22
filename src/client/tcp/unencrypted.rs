use network_helpers_sv2::plain_connection::PlainConnection;
use roles_logic_sv2::parsers::AnyMessage;
use std::net::SocketAddr;
use tokio::net::TcpStream;

use crate::Sv2MessageIo;

/// a TCP client that connects to a server **without** Sv2 noise encryption
///
/// Note: [`Sv2UnencryptedTcpClient`] is NOT a [`tower::Service`],
/// but rather a helper primitive to facilitate establishing unencrypted TCP connections
#[derive(Debug)]
pub struct Sv2UnencryptedTcpClient {
    /// IO of Sv2 Message Frames
    pub io: Sv2MessageIo,
}

impl Sv2UnencryptedTcpClient {
    /// [`Sv2UnencryptedTcpClient`] constructor
    ///
    /// once constructed, `rx` and `tx` are used for IO
    ///
    /// returns `None` if the connection is not successfully established
    pub async fn new(server_addr: SocketAddr) -> Option<Self> {
        let tcp_stream = TcpStream::connect(server_addr).await.ok()?;

        let (rx, tx) = PlainConnection::new::<'static, AnyMessage<'static>>(tcp_stream).await;
        let sv2_message_io = Sv2MessageIo { rx, tx };
        tracing::info!("connected to: {}", server_addr);
        Some(Self { io: sv2_message_io })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn new_sv2_unencrypted_tcp_client_works() {
        // // tokio-console subscribe
        // console_subscriber::init();

        // // tracing subscribe
        // tracing_subscriber::fmt()
        //     .with_env_filter("debug") // Adjust log level as needed
        //     .init();

        let server_addr = SocketAddr::from(([127, 0, 0, 1], 1337));

        let _server = TcpListener::bind(server_addr)
            .await
            .expect("failed to bind to socket");

        // create new Sv2UnencryptedConnectionClient (connecting to server)
        let sv2_unencrypted_tcp_client = Sv2UnencryptedTcpClient::new(server_addr).await;

        // assert Sv2UnencryptedConnectionClient was constructed
        assert!(sv2_unencrypted_tcp_client.is_some());
    }
}
