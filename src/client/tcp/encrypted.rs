use codec_sv2::{HandshakeRole, Initiator};
use key_utils::Secp256k1PublicKey;
use network_helpers_sv2::noise_connection::Connection;
use roles_logic_sv2::parsers::AnyMessage;
use std::net::SocketAddr;
use tokio::net::TcpStream;

use crate::Sv2MessageIo;

/// a TCP client that connects to a server **with** Sv2 noise encryption
///
/// Note: [`Sv2EncryptedTcpClient`] is NOT a [`tower::Service`],
/// but rather a helper primitive to facilitate establishing encrypted TCP connections
#[derive(Debug, Clone)]
pub struct Sv2EncryptedTcpClient {
    /// IO of Sv2 Message Frames
    pub io: Sv2MessageIo,
}

impl Sv2EncryptedTcpClient {
    /// [`Sv2EncryptedTcpClient`] constructor
    ///
    /// once constructed, `rx` and `tx` are used for IO
    ///
    /// returns `None` if the connection is not successfully established
    pub async fn new(server_addr: SocketAddr, auth_pk: Option<Secp256k1PublicKey>) -> Option<Self> {
        let tcp_stream = TcpStream::connect(server_addr).await.ok()?;

        // initiate handshake with network_helpers_sv2
        let initiator = match auth_pk {
            Some(auth_pk) => Initiator::from_raw_k(auth_pk.into_bytes()),
            None => Initiator::without_pk(),
        }
        .ok()?;

        if let Ok((rx, tx)) = Connection::new::<'static, AnyMessage<'static>>(
            tcp_stream,
            HandshakeRole::Initiator(initiator),
        )
        .await
        {
            let sv2_message_io = Sv2MessageIo { rx, tx };
            tracing::info!("connected to: {}", server_addr);
            Some(Self { io: sv2_message_io })
        } else {
            None
        }
    }

    pub fn shutdown(&self) {
        self.io.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Sv2MessageFrame;
    use roles_logic_sv2::common_messages_sv2::{Protocol, SetupConnection};
    use roles_logic_sv2::common_messages_sv2::{
        MESSAGE_TYPE_SETUP_CONNECTION, MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
    };

    #[tokio::test]
    async fn new_sv2_encrypted_tcp_client_works() {
        // start a TemplateProvider
        let (_tp, tp_addr) = integration_tests_sv2::start_template_provider(None);

        // connect client
        let sv2_encrypted_tcp_client = Sv2EncryptedTcpClient::new(tp_addr, None).await.unwrap();
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

        // send SetupConnection from client
        sv2_encrypted_tcp_client
            .io
            .send_message(setup_connection.into(), MESSAGE_TYPE_SETUP_CONNECTION)
            .await
            .unwrap();

        // receive response frame on client side
        let received_frame = sv2_encrypted_tcp_client.io.rx.recv().await.unwrap();

        // assert received frame is SetupConnection.Success
        if let Sv2MessageFrame::Sv2(frame) = received_frame {
            let received_message_header = frame.get_header().unwrap();
            let received_message_type = received_message_header.msg_type();
            assert_eq!(received_message_type, MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS);
        } else {
            panic!("received wrong frame");
        }
    }
}
