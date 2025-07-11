use std::net::SocketAddr;
use stratum_common::network_helpers_sv2::plain_connection::PlainConnection;
use stratum_common::roles_logic_sv2::parsers::AnyMessage;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio::sync::mpsc;

use crate::Sv2MessageIo;

/// A function that creates a TCP server that listens for clients without Sv2 noise encryption.
///
/// As soon as a client connects, a [`Sv2MessageIo`] is created and sent through a channel to the service layer.
pub async fn start_unencrypted_tcp_server(
    listen_address: SocketAddr,
    new_client_tx: mpsc::Sender<Sv2MessageIo>,
    shutdown_rx: broadcast::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(listen_address).await?;

    let mut shutdown_rx = shutdown_rx.resubscribe();

    // spawn a task to accept incoming connections
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    tracing::debug!("Unencrypted TCP server received shutdown signal");
                    break;
                }
                Ok((stream, addr)) = listener.accept() => {
                    tracing::debug!("Unencrypted TCP server accepted new connection");

                    let (rx, tx) =
                    PlainConnection::new::<'static, AnyMessage<'static>>(stream).await;

                    let sv2_message_io = Sv2MessageIo { rx, tx };

                    if new_client_tx.send(sv2_message_io).await.is_ok() {
                        tracing::debug!("Connected to: {}", addr);
                    } else {
                        tracing::error!("Failed to send new client to service layer for {}", addr);
                    }
                }
            }
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::client::tcp::unencrypted::Sv2UnencryptedTcpClient;
    use crate::Sv2MessageFrame;
    use once_cell::sync::Lazy;
    use std::collections::HashSet;
    use std::net::{SocketAddr, TcpListener};
    use std::sync::Mutex;
    use stratum_common::roles_logic_sv2::codec_sv2::framing_sv2::framing::Sv2Frame;
    use stratum_common::roles_logic_sv2::common_messages_sv2::{
        Protocol, SetupConnection, SetupConnectionSuccess,
    };
    use stratum_common::roles_logic_sv2::common_messages_sv2::{
        MESSAGE_TYPE_SETUP_CONNECTION, MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
    };
    use stratum_common::roles_logic_sv2::parsers::AnyMessage;
    use tokio::sync::broadcast;
    use tokio::sync::mpsc;

    // prevents get_available_port from ever returning the same port twice
    static UNIQUE_PORTS: Lazy<Mutex<HashSet<u16>>> = Lazy::new(|| Mutex::new(HashSet::new()));

    fn get_available_port() -> u16 {
        let mut unique_ports = UNIQUE_PORTS.lock().unwrap();

        loop {
            let port = TcpListener::bind("127.0.0.1:0")
                .unwrap()
                .local_addr()
                .unwrap()
                .port();
            if !unique_ports.contains(&port) {
                unique_ports.insert(port);
                return port;
            }
        }
    }

    #[tokio::test]
    async fn new_sv2_unencrypted_tcp_server_works() {
        let server_port = get_available_port();
        let server_addr = SocketAddr::from(([127, 0, 0, 1], server_port));

        // Create channel for new client connections
        let (new_client_tx, mut new_client_rx) = mpsc::channel(32);

        let (_shutdown_tx, shutdown_rx) = broadcast::channel(1);

        super::start_unencrypted_tcp_server(server_addr, new_client_tx, shutdown_rx)
            .await
            .expect("Server should start successfully");

        // connect client
        let sv2_unencrypted_tcp_client = Sv2UnencryptedTcpClient::new(server_addr).await.unwrap();

        // SetupConnection message
        let setup_connection = SetupConnection {
            protocol: Protocol::TemplateDistributionProtocol,
            min_version: 2,
            max_version: 2,
            flags: 0b0000_0000_0000_0000_0000_0000_0000_0000,
            endpoint_host: "".to_string().try_into().unwrap(),
            endpoint_port: 0,
            vendor: "".to_string().try_into().unwrap(),
            hardware_version: "".to_string().try_into().unwrap(),
            firmware: "".to_string().try_into().unwrap(),
            device_id: "".to_string().try_into().unwrap(),
        };

        // SetupConnection message frame
        let setup_connection_frame = Sv2MessageFrame::Sv2(
            Sv2Frame::from_message(
                AnyMessage::Common(setup_connection.into()),
                MESSAGE_TYPE_SETUP_CONNECTION,
                0,
                false,
            )
            .unwrap(),
        );

        // send SetupConnection from client
        sv2_unencrypted_tcp_client
            .io
            .tx
            .send(setup_connection_frame)
            .await
            .unwrap();

        // Wait for server to send the new client through the channel
        let server_client_io = new_client_rx
            .recv()
            .await
            .expect("should receive new client");

        // receive frame on server side
        let received_frame = server_client_io.rx.recv().await.unwrap();

        // assert received frame
        if let Sv2MessageFrame::Sv2(frame) = received_frame {
            let received_message_header = frame.get_header().unwrap();
            let received_message_type = received_message_header.msg_type();
            assert_eq!(received_message_type, MESSAGE_TYPE_SETUP_CONNECTION);
        } else {
            panic!("received wrong frame");
        }

        // SetupConnection.Success message
        let setup_connection_success = SetupConnectionSuccess {
            used_version: 0,
            flags: 0,
        };

        // SetupConnection.Success message frame
        let setup_connection_success_frame = Sv2MessageFrame::Sv2(
            Sv2Frame::from_message(
                AnyMessage::Common(setup_connection_success.into()),
                MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
                0,
                false,
            )
            .unwrap(),
        );

        // send SetupConnection.Success from server side
        server_client_io
            .tx
            .send(setup_connection_success_frame)
            .await
            .unwrap();

        // receive frame on client side
        let received_frame = sv2_unencrypted_tcp_client.io.rx.recv().await.unwrap();

        // assert received frame
        if let Sv2MessageFrame::Sv2(frame) = received_frame {
            let received_message_header = frame.get_header().unwrap();
            let received_message_type = received_message_header.msg_type();
            assert_eq!(received_message_type, MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS);
        } else {
            panic!("received wrong frame");
        }
    }
}
