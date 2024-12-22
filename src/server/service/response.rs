use roles_logic_sv2::parsers::AnyMessage;

/// A reply containing a Sv2 message, to be delivered back to the client.
#[derive(Debug, Clone)]
pub struct Sv2MessageToClient<'a> {
    pub client_id: u32,
    pub message: AnyMessage<'a>,
    pub message_type: u8,
}

/// The Response type for the tower service [`crate::server::service::Sv2ServerService`].
#[derive(Debug, Clone)]
pub enum ResponseFromSv2Server<'a> {
    // triggers the service to send a reply to the client
    SendReplyToClient(Sv2MessageToClient<'a>),
    // indicates that the service has sent a reply to the client
    SentReplyToClient(Sv2MessageToClient<'a>),
    ToDo, // dummy placeholder for future response types (e.g.: Relay)
}
