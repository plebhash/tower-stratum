#[allow(unused_imports)]
use encrypted::Sv2EncryptedTcpClient;
// #[allow(unused_imports)]
// use unencrypted::Sv2UnencryptedTcpClient;

/// Provides a TCP client that connects to a server **with** Sv2 noise encryption
///
/// The main object of this module is [`Sv2EncryptedTcpClient`]
pub mod encrypted;

/// Provides a TCP client that connects to a server **without** Sv2 noise encryption
///
/// The main object of this module is [`Sv2UnencryptedTcpClient`]
pub mod unencrypted;
