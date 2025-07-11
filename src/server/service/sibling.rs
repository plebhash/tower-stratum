//! A "sibling client service" is a [`crate::client::service::Sv2ClientService`] that is paired with this [`crate::server::service::Sv2ServerService`].
//!
use crate::client::service::request::RequestToSv2Client;
use crate::client::service::sibling::Sv2SiblingServerServiceIo;
use crate::server::service::request::RequestToSv2Server;

use async_channel::Receiver;
use async_channel::Sender;
use async_channel::TrySendError;

/// A [`Sv2SiblingClientServiceIo`] is used by a [`crate::server::service::Sv2ServerService`] to send and receive requests to a sibling [`crate::client::service::Sv2ClientService`] that pairs with this server.    
#[derive(Debug, Clone)]
pub struct Sv2SiblingClientServiceIo {
    rx: Receiver<Box<RequestToSv2Server<'static>>>,
    tx: Sender<Box<RequestToSv2Client<'static>>>,
}

impl Sv2SiblingClientServiceIo {
    /// Create a new [`Sv2SiblingClientServiceIo`] and a new [`Sv2SiblingServerServiceIo`].
    ///
    /// The [`Sv2SiblingClientServiceIo`] is used to send requests to the outside (e.g.: some [`crate::client::service::Sv2ClientService`] that pairs with this server).    
    pub fn new() -> (Self, Sv2SiblingServerServiceIo) {
        let (server_tx, server_rx) = async_channel::unbounded::<Box<RequestToSv2Server<'static>>>();
        let (client_tx, client_rx) = async_channel::unbounded::<Box<RequestToSv2Client<'static>>>();

        let sibling_server_service_io = Sv2SiblingServerServiceIo::set(client_rx, server_tx);
        let sibling_client_service_io = Self {
            rx: server_rx,
            tx: client_tx,
        };

        (sibling_client_service_io, sibling_server_service_io)
    }

    /// Send a request to the sibling client service.
    pub fn send(
        &self,
        request: RequestToSv2Client<'static>,
    ) -> Result<(), TrySendError<Box<RequestToSv2Client<'static>>>> {
        self.tx.try_send(Box::new(request))
    }

    /// Receive a request from the sibling client service.
    pub async fn recv(&self) -> Result<Box<RequestToSv2Server<'static>>, async_channel::RecvError> {
        self.rx.recv().await
    }

    /// Shutdown the sibling client service io.
    pub fn shutdown(&self) {
        self.tx.close();
        self.rx.close();
    }
}
