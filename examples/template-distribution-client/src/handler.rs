use anyhow::Result;
use roles_logic_sv2::template_distribution_sv2::{
    NewTemplate, RequestTransactionDataError, RequestTransactionDataSuccess, SetNewPrevHash,
};
use tower_stratum::client::service::request::RequestToSv2ClientServiceError;
use tower_stratum::client::service::response::ResponseFromSv2ClientService;
use tower_stratum::client::service::subprotocols::template_distribution::handler::Sv2TemplateDistributionClientHandler;
use tracing::info;

#[derive(Debug, Clone, Default)]
pub struct MyTemplateDistributionHandler {
    // You could add fields here to store state or callbacks
}

/// Implement the Sv2TemplateDistributionClientHandler trait for MyTemplateDistributionClient
impl Sv2TemplateDistributionClientHandler for MyTemplateDistributionHandler {
    fn handle_new_template(
        &self,
        template: NewTemplate,
    ) -> Result<ResponseFromSv2ClientService<'static>, RequestToSv2ClientServiceError> {
        info!("received new template: {:?}", template);
        Ok(ResponseFromSv2ClientService::Ok)
    }

    fn handle_set_new_prev_hash(
        &self,
        prev_hash: SetNewPrevHash,
    ) -> Result<ResponseFromSv2ClientService<'static>, RequestToSv2ClientServiceError> {
        info!("received new prev_hash: {:?}", prev_hash);
        Ok(ResponseFromSv2ClientService::Ok)
    }

    fn handle_request_transaction_data_success(
        &self,
        transaction_data: RequestTransactionDataSuccess,
    ) -> Result<ResponseFromSv2ClientService<'static>, RequestToSv2ClientServiceError> {
        info!(
            "received request transaction data success: {:?}",
            transaction_data
        );
        Ok(ResponseFromSv2ClientService::Ok)
    }

    fn handle_request_transaction_data_error(
        &self,
        error: RequestTransactionDataError,
    ) -> Result<ResponseFromSv2ClientService<'static>, RequestToSv2ClientServiceError> {
        info!("received request transaction data error: {:?}", error);
        Ok(ResponseFromSv2ClientService::Ok)
    }
}
