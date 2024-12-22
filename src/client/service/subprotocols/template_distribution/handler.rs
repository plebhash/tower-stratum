use crate::client::service::request::RequestToSv2ClientServiceError;
use crate::client::service::response::ResponseFromSv2ClientService;

use roles_logic_sv2::parsers::{AnyMessage, TemplateDistribution};
use roles_logic_sv2::template_distribution_sv2::{
    CoinbaseOutputConstraints, NewTemplate, RequestTransactionData, RequestTransactionDataError,
    RequestTransactionDataSuccess, SetNewPrevHash,
};

/// Trait that must be implemented in case [`crate::client::service::Sv2ClientService`] supports the Template Distribution protocol
pub trait Sv2TemplateDistributionClientHandler {
    fn handle_new_template(
        &self,
        template: NewTemplate,
    ) -> Result<ResponseFromSv2ClientService<'static>, RequestToSv2ClientServiceError>;

    fn handle_set_new_prev_hash(
        &self,
        prev_hash: SetNewPrevHash,
    ) -> Result<ResponseFromSv2ClientService<'static>, RequestToSv2ClientServiceError>;

    fn handle_request_transaction_data_success(
        &self,
        transaction_data: RequestTransactionDataSuccess,
    ) -> Result<ResponseFromSv2ClientService<'static>, RequestToSv2ClientServiceError>;

    fn handle_request_transaction_data_error(
        &self,
        error: RequestTransactionDataError,
    ) -> Result<ResponseFromSv2ClientService<'static>, RequestToSv2ClientServiceError>;

    fn transaction_data_needed(
        &self,
        template_id: u64,
    ) -> Result<ResponseFromSv2ClientService<'static>, RequestToSv2ClientServiceError> {
        let message = AnyMessage::TemplateDistribution(
            TemplateDistribution::RequestTransactionData(RequestTransactionData { template_id }),
        );

        Ok(ResponseFromSv2ClientService::SendToServer(message))
    }

    fn set_coinbase_output_constraints(
        &self,
        coinbase_output_max_additional_size: u32,
        coinbase_output_max_additional_sigops: u16,
    ) -> Result<ResponseFromSv2ClientService<'static>, RequestToSv2ClientServiceError> {
        let message = AnyMessage::TemplateDistribution(
            TemplateDistribution::CoinbaseOutputConstraints(CoinbaseOutputConstraints {
                coinbase_output_max_additional_size,
                coinbase_output_max_additional_sigops,
            }),
        );

        Ok(ResponseFromSv2ClientService::SendToServer(message))
    }
}

// -------------------------------------------------------------------------------------------------
// NullSv2TemplateDistributionClientHandler
// -------------------------------------------------------------------------------------------------

/// A [`Sv2TemplateDistributionClientHandler`] implementation that does nothing.
///
/// It should be used when creating a [`crate::client::service::Sv2ClientService`] that
/// does not support the Template Distribution protocol.
#[derive(Debug, Clone)]
pub struct NullSv2TemplateDistributionClientHandler;

impl Sv2TemplateDistributionClientHandler for NullSv2TemplateDistributionClientHandler {
    fn handle_new_template(
        &self,
        _template: NewTemplate,
    ) -> Result<ResponseFromSv2ClientService<'static>, RequestToSv2ClientServiceError> {
        unimplemented!(
            "NullSv2TemplateDistributionClientHandler does not implement handle_new_template"
        );
    }

    fn handle_set_new_prev_hash(
        &self,
        _prev_hash: SetNewPrevHash,
    ) -> Result<ResponseFromSv2ClientService<'static>, RequestToSv2ClientServiceError> {
        unimplemented!(
            "NullSv2TemplateDistributionClientHandler does not implement handle_set_new_prev_hash"
        );
    }

    fn handle_request_transaction_data_success(
        &self,
        _transaction_data: RequestTransactionDataSuccess,
    ) -> Result<ResponseFromSv2ClientService<'static>, RequestToSv2ClientServiceError> {
        unimplemented!("NullSv2TemplateDistributionClientHandler does not implement handle_request_transaction_data_success");
    }

    fn handle_request_transaction_data_error(
        &self,
        _error: RequestTransactionDataError,
    ) -> Result<ResponseFromSv2ClientService<'static>, RequestToSv2ClientServiceError> {
        unimplemented!("NullSv2TemplateDistributionClientHandler does not implement handle_request_transaction_data_error");
    }

    fn transaction_data_needed(
        &self,
        _template_id: u64,
    ) -> Result<ResponseFromSv2ClientService<'static>, RequestToSv2ClientServiceError> {
        unimplemented!(
            "NullSv2TemplateDistributionClientHandler does not implement request_transaction_data"
        );
    }

    fn set_coinbase_output_constraints(
        &self,
        _coinbase_output_max_additional_size: u32,
        _coinbase_output_max_additional_sigops: u16,
    ) -> Result<ResponseFromSv2ClientService<'static>, RequestToSv2ClientServiceError> {
        unimplemented!("NullSv2TemplateDistributionClientHandler does not implement send_coinbase_output_constraints");
    }
}
