/// Requests to the Client Service that are specific to the Template Distribution protocol
#[derive(Debug, Clone)]
pub enum RequestToSv2TemplateDistributionClientService {
    SetCoinbaseOutputConstraints(u32, u16),
    TransactionDataNeeded(u64),
}
