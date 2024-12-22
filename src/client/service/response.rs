use crate::client::service::request::RequestToSv2ClientService;
use crate::client::service::subprotocols::template_distribution::response::ResponseToTemplateDistributionTrigger;
use roles_logic_sv2::parsers::AnyMessage;

/// The response type for the tower service [`crate::client::service::Sv2ClientService`].
#[derive(Debug)]
pub enum ResponseFromSv2ClientService<'a> {
    ConnectionEstablished,
    SendToServer(AnyMessage<'a>),
    ResponseToTemplateDistributionTrigger(ResponseToTemplateDistributionTrigger),
    TriggerNewRequest(RequestToSv2ClientService<'a>),
    Ok,
    ToDo, // dummy placeholder for future response types (e.g.: Relay)
}
