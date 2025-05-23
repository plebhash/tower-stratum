mod encrypted;

mod unencrypted;

use std::net::SocketAddr;

use encrypted::Sv2EncryptedTcpClient;
use key_utils::Secp256k1PublicKey;
use unencrypted::Sv2UnencryptedTcpClient;

use crate::Sv2MessageIo;

#[derive(Debug, Clone)]
pub enum Sv2TcpClient {
    /// Provides a TCP client that connects to a server **without** Sv2 noise encryption
    ///
    /// The main object of this module is [`Sv2UnencryptedTcpClient`]
    Encrypted(Sv2EncryptedTcpClient),
    /// Provides a TCP client that connects to a server **with** Sv2 noise encryption
    ///
    /// The main object of this module is [`Sv2EncryptedTcpClient`]
    Unencrypted(Sv2UnencryptedTcpClient),
}

impl Sv2TcpClient {
    pub fn shutdown(&self) {
        match self {
            Sv2TcpClient::Encrypted(client) => client.shutdown(),
            Sv2TcpClient::Unencrypted(client) => client.shutdown(),
        }
    }

    pub async fn new(
        server_addr: SocketAddr,
        encrypted: bool,
        auth_pk: Option<Secp256k1PublicKey>,
    ) -> Option<Self> {
        if encrypted {
            Sv2EncryptedTcpClient::new(server_addr, auth_pk)
                .await
                .map(Sv2TcpClient::Encrypted)
        } else {
            Sv2UnencryptedTcpClient::new(server_addr)
                .await
                .map(Sv2TcpClient::Unencrypted)
        }
    }

    pub fn io(&self) -> &Sv2MessageIo {
        match self {
            Sv2TcpClient::Encrypted(client) => &client.io,
            Sv2TcpClient::Unencrypted(client) => &client.io,
        }
    }
}
