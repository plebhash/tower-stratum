#[allow(unused_imports)]
use encrypted::start_encrypted_tcp_server;
// #[allow(unused_imports)]
// use unencrypted::Sv2UnencryptedTcpServer;

/// Provides a TCP server that listens for clients **with** Sv2 noise encryption
///
/// The main function of this module is [`start_encrypted_tcp_server`]
pub mod encrypted;

/// Provides a TCP server that listens for clients **without** Sv2 noise encryption
///
/// The main function of this module is [`start_unencrypted_tcp_server`]
pub mod unencrypted;
