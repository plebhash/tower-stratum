//! A [`Sv2SiblingServerServiceIo`] is used to send and receive requests to a sibling [`crate::client::service::Sv2ClientService`] that pairs with this server.    
//!
use crate::client::service::request::RequestToSv2Client;
use crate::server::service::request::RequestToSv2Server;

use async_channel::Receiver;
use async_channel::Sender;
use async_channel::TrySendError;

/// A [`Sv2SiblingServerServiceIo`] is used to send and receive requests to a sibling [`crate::client::service::Sv2ClientService`] that pairs with this server.    
#[derive(Debug, Clone)]
pub struct Sv2SiblingServerServiceIo {
    rx: Receiver<RequestToSv2Client<'static>>,
    tx: Sender<RequestToSv2Server<'static>>,
}

impl Sv2SiblingServerServiceIo {
    /// Create a new [`Sv2SiblingServerServiceIo`] from a prebuilt [`Sv2SiblingClientServiceIo`].
    pub fn set(
        rx: Receiver<RequestToSv2Client<'static>>,
        tx: Sender<RequestToSv2Server<'static>>,
    ) -> Self {
        Self { rx, tx }
    }

    /// Send a request to the sibling server service.
    pub fn send(
        &self,
        request: RequestToSv2Server<'static>,
    ) -> Result<(), TrySendError<RequestToSv2Server<'static>>> {
        self.tx.try_send(request)
    }

    /// Receive a request from the sibling server service.
    pub async fn recv(&self) -> Result<RequestToSv2Client<'static>, async_channel::RecvError> {
        self.rx.recv().await
    }
}
