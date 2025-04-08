use roles_logic_sv2::common_messages_sv2::Protocol;
use roles_logic_sv2::parsers::AnyMessage;

use crate::client::service::request::RequestToSv2Client;
use crate::server::service::response::Sv2MessageToClient;
use crate::server::service::subprotocols::mining::request::RequestToSv2MiningServer;

/// The request type for the [`crate::server::service::Sv2ServerService`] service.
#[derive(Debug, Clone)]
pub enum RequestToSv2Server<'a> {
    /// Some Sv2 message addressed to the server.
    /// Could belong to any subprotocol.
    Message(Sv2MessageToServer<'a>),
    /// Some trigger for the mining subprotocol service
    MiningTrigger(RequestToSv2MiningServer<'a>),
    // todo:
    // JobDeclarationTrigger(RequestToSv2JobDeclarationServer<'a>),
    // TemplateDistributionTrigger(RequestToSv2TemplateDistributionServer<'a>),
    SendRequestToSiblingClientService(RequestToSv2Client<'a>),
}

/// A Sv2 message addressed to the server, to be used as the request type of [`crate::server::service::Sv2ServerService`].
///
/// The client_id is always Some(id), with the exception of the initial SetupConnection request,
/// where the client_id is allocated by the server and returned in the response.
#[derive(Debug, Clone)]
pub struct Sv2MessageToServer<'a> {
    pub client_id: Option<u32>,
    pub message: AnyMessage<'a>,
}

/// The error type for the [`crate::server::service::Sv2ServerService`] service.
#[derive(Debug, Clone)]
pub enum RequestToSv2ServerError {
    IdNotFound,
    IdMustBeSome,
    BadRouting,
    UnsupportedMessage,
    FailedToSendResponseToClient,
    UnsupportedProtocol { protocol: Protocol },
    FailedToSendRequestToSiblingClientService,
    NoSiblingClientService,
    Reply(Sv2MessageToClient<'static>),
}
