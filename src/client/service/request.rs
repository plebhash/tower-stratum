use crate::client::service::subprotocols::mining::trigger::MiningClientTrigger;
use crate::client::service::subprotocols::template_distribution::trigger::TemplateDistributionClientTrigger;
use crate::server::service::request::RequestToSv2Server;
use crate::Sv2MessageIoError;
use stratum_common::roles_logic_sv2::common_messages_sv2::Protocol;
use stratum_common::roles_logic_sv2::parsers::{AnyMessage, Mining, TemplateDistribution};

/// The request type for the [`crate::client::service::Sv2ClientService`] service.
#[derive(Debug, Clone)]
pub enum RequestToSv2Client<'a> {
    /// Trigger for the client to initiate a connection to the server under some subprotocol.
    SetupConnectionTrigger(Protocol, u32), // protocol, flags
    /// Some Sv2 message addressed to the client.
    /// Could belong to any subprotocol.
    IncomingMessage(AnyMessage<'a>),
    MiningTrigger(MiningClientTrigger),
    TemplateDistributionTrigger(TemplateDistributionClientTrigger<'a>),
    SendRequestToSiblingServerService(Box<RequestToSv2Server<'a>>),
    SendMessageToMiningServer(Box<Mining<'a>>),
    SendMessageToTemplateDistributionServer(Box<TemplateDistribution<'a>>),
    // SendMessageToJobDeclarationServer(Box<(JobDeclaration<'a>, u8)>),
    /// Execute an ordered sequence of requests.
    MultipleRequests(Box<Vec<RequestToSv2Client<'a>>>),
}

/// The error type for the [`crate::client::service::Sv2ClientService`] service.
#[derive(Debug, Clone)]
pub enum RequestToSv2ClientError {
    BadRouting,
    UnsupportedMessage,
    UnsupportedProtocol { protocol: Protocol },
    IsNotConnected,
    SetupConnectionError(String),
    ConnectionError(String),
    StringConversionError(String),
    NoSiblingServerServiceIo,
    FailedToSendRequestToSiblingServerService,
    U256ConversionError(String),
    MiningHandlerError(String),
    TemplateDistributionHandlerError(String),
    JobDeclarationHandlerError(String),
}

impl From<Sv2MessageIoError> for RequestToSv2ClientError {
    fn from(error: Sv2MessageIoError) -> Self {
        match error {
            Sv2MessageIoError::SendError => {
                RequestToSv2ClientError::ConnectionError("Failed to send message".to_string())
            }
            Sv2MessageIoError::FrameError => {
                RequestToSv2ClientError::ConnectionError("Failed to create frame".to_string())
            }
            Sv2MessageIoError::RecvError => {
                RequestToSv2ClientError::ConnectionError("Failed to receive message".to_string())
            }
        }
    }
}
