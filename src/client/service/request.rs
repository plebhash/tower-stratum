use crate::client::service::subprotocols::template_distribution::request::RequestToSv2TemplateDistributionClientService;
use crate::Sv2MessageIoError;
use roles_logic_sv2::common_messages_sv2::Protocol;
use roles_logic_sv2::parsers::AnyMessage;

/// The request type for the [`crate::client::service::Sv2ClientService`] service.
#[derive(Debug, Clone)]
pub enum RequestToSv2ClientService<'a> {
    /// Trigger for the client to initiate a connection to the server under some subprotocol.
    SetupConnectionTrigger(Protocol),
    /// Some Sv2 message addressed to the client.
    /// Could belong to any subprotocol.
    Message(AnyMessage<'a>),
    TemplateDistributionTrigger(RequestToSv2TemplateDistributionClientService),
}

/// The error type for the [`crate::client::service::Sv2ClientService`] service.
#[derive(Debug, Clone)]
pub enum RequestToSv2ClientServiceError {
    BadRouting,
    UnsupportedMessage,
    UnsupportedProtocol { protocol: Protocol },
    IsNotConnected,
    SetupConnectionError(String),
    ConnectionError(String),
    StringConversionError(String),
}

impl From<Sv2MessageIoError> for RequestToSv2ClientServiceError {
    fn from(error: Sv2MessageIoError) -> Self {
        match error {
            Sv2MessageIoError::SendError => RequestToSv2ClientServiceError::ConnectionError(
                "Failed to send message".to_string(),
            ),
            Sv2MessageIoError::FrameError => RequestToSv2ClientServiceError::ConnectionError(
                "Failed to create frame".to_string(),
            ),
            Sv2MessageIoError::RecvError => RequestToSv2ClientServiceError::ConnectionError(
                "Failed to receive message".to_string(),
            ),
        }
    }
}
