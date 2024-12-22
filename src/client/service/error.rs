use roles_logic_sv2::common_messages_sv2::Protocol;

#[derive(Debug)]
pub enum Sv2ClientServiceError {
    BadConfig,
    NullHandlerForSupportedProtocol { protocol: Protocol },
    NonNullHandlerForUnsupportedProtocol { protocol: Protocol },
}
