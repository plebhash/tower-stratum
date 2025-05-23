use crate::Sv2MessageIo;
use codec_sv2::{HandshakeRole, Responder};
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use network_helpers_sv2::noise_connection::Connection;
use roles_logic_sv2::parsers::AnyMessage;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio::sync::mpsc;

/// A function that creates a TCP server that listens for clients under Sv2 noise encryption.
///
/// As soon as a client connects, a [`Sv2MessageIo`] is created and sent through a channel to the service layer.
pub async fn start_encrypted_tcp_server(
    listen_address: SocketAddr,
    pub_key: Secp256k1PublicKey,
    priv_key: Secp256k1SecretKey,
    cert_validity: u64,
    new_client_tx: mpsc::Sender<Sv2MessageIo>,
    shutdown_rx: broadcast::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(listen_address).await?;
    tracing::debug!("Server bound to {}", listen_address);

    let mut shutdown_rx = shutdown_rx.resubscribe();

    // spawn a task to accept incoming connections
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    tracing::debug!("Encrypted TCP server received shutdown signal");
                    break;
                }
                Ok((stream, addr)) = listener.accept() => {
                    tracing::debug!("Accepted connection from {}", addr);
                    let responder = Responder::from_authority_kp(
                        &pub_key.into_bytes(),
                        &priv_key.into_bytes(),
                        std::time::Duration::from_secs(cert_validity),
                    )
                    .expect("invalid key");

                    if let Ok((rx, tx)) =
                        Connection::new::<'static, AnyMessage<'static>>(
                            stream,
                            HandshakeRole::Responder(responder),
                        )
                        .await
                    {
                        let sv2_message_io = Sv2MessageIo {
                            rx,
                            tx,
                        };

                        // Send the new client's IO to the service layer
                        if new_client_tx.send(sv2_message_io).await.is_ok() {
                            tracing::debug!("Connected to: {}", addr);
                        } else {
                            tracing::error!("Failed to send new client to service layer for {}", addr);
                        }
                    } else {
                        tracing::warn!("Failed to perform handshake with client {}", addr);
                    }
                }
            }
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::client::tcp::Sv2TcpClient;
    use crate::Sv2MessageFrame;
    use const_sv2::{MESSAGE_TYPE_SETUP_CONNECTION, MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS};
    use framing_sv2::framing::Sv2Frame;
    use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
    use once_cell::sync::Lazy;
    use roles_logic_sv2::common_messages_sv2::{Protocol, SetupConnection, SetupConnectionSuccess};
    use roles_logic_sv2::parsers::AnyMessage;
    use std::{
        collections::HashSet,
        convert::TryInto,
        net::{SocketAddr, TcpListener},
        sync::Mutex,
    };
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
    async fn start_encrypted_tcp_server_works() {
        let server_port = get_available_port();
        let server_addr = SocketAddr::from(([127, 0, 0, 1], server_port));
        let pub_key = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72"
            .to_string()
            .parse::<Secp256k1PublicKey>()
            .unwrap();
        let priv_key = "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n"
            .to_string()
            .parse::<Secp256k1SecretKey>()
            .unwrap();

        // Create channel for new client connections
        let (new_client_tx, mut new_client_rx) = mpsc::channel(32);

        let (_shutdown_tx, shutdown_rx) = broadcast::channel(1);

        // Create server
        super::start_encrypted_tcp_server(
            server_addr,
            pub_key.clone(),
            priv_key,
            10000,
            new_client_tx,
            shutdown_rx,
        )
        .await
        .expect("Server should start successfully");

        // Connect client
        let sv2_encrypted_tcp_client = Sv2TcpClient::new(server_addr, true, Some(pub_key))
            .await
            .unwrap();

        // Wait for server to send the new client through the channel
        let server_client_io = new_client_rx
            .recv()
            .await
            .expect("should receive new client");

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

        // Send SetupConnection from client
        sv2_encrypted_tcp_client
            .io()
            .send_message(setup_connection.into(), MESSAGE_TYPE_SETUP_CONNECTION)
            .await
            .unwrap();

        // Receive frame on server side
        let received_frame = server_client_io.rx.recv().await.unwrap();

        // Assert received frame
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

        // Send SetupConnection.Success from server side
        server_client_io
            .tx
            .send(setup_connection_success_frame)
            .await
            .unwrap();

        // Receive frame on client side
        let received_frame = sv2_encrypted_tcp_client.io().rx.recv().await.unwrap();

        // Assert received frame
        if let Sv2MessageFrame::Sv2(frame) = received_frame {
            let received_message_header = frame.get_header().unwrap();
            let received_message_type = received_message_header.msg_type();
            assert_eq!(received_message_type, MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS);
        } else {
            panic!("received wrong frame");
        }
    }
}
