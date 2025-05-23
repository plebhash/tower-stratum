use std::task::{Context, Poll};

use roles_logic_sv2::template_distribution_sv2::{
    CoinbaseOutputConstraints, RequestTransactionData, SubmitSolution,
};

use crate::server::service::{request::RequestToSv2ServerError, response::ResponseFromSv2Server};

/// Trait that must be implemented in case [`crate::server::service::Sv2ServerService`] supports the Template Distribution protocol
pub trait Sv2TemplateDistributionServerHandler {
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), RequestToSv2ServerError>>;

    fn handle_coinbase_out_data_size(
        &mut self,
        m: CoinbaseOutputConstraints,
    ) -> impl std::future::Future<
        Output = Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError>,
    > + Send;

    fn handle_request_tx_data(
        &mut self,
        m: RequestTransactionData,
    ) -> impl std::future::Future<
        Output = Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError>,
    > + Send;

    fn handle_request_submit_solution(
        &mut self,
        m: SubmitSolution<'static>,
    ) -> impl std::future::Future<
        Output = Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError>,
    > + Send;
}

// -------------------------------------------------------------------------------------------------
// NullSv2TemplateDistributionServerHandler
// -------------------------------------------------------------------------------------------------

/// A [`Sv2TemplateDistributionServerHandler`] implementation that does nothing.
///
/// It should be used when creating a [`crate::server::service::Sv2ServerService`] that
/// does not support the template distribution subprotocol.
pub struct NullSv2TemplateDistributionServerHandler;

impl Sv2TemplateDistributionServerHandler for NullSv2TemplateDistributionServerHandler {
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), RequestToSv2ServerError>> {
        unimplemented!("NullSv2TemplateDistributionServerHandler does not implement poll_ready");
    }

    async fn handle_coinbase_out_data_size(
        &mut self,
        _m: CoinbaseOutputConstraints,
    ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
        unimplemented!("NullSv2TemplateDistributionServerHandler does not implement handle_coinbase_out_data_size");
    }

    async fn handle_request_tx_data(
        &mut self,
        _m: RequestTransactionData,
    ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
        unimplemented!(
            "NullSv2TemplateDistributionServerHandler does not implement handle_request_tx_data"
        );
    }

    async fn handle_request_submit_solution(
        &mut self,
        _m: SubmitSolution<'static>,
    ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
        unimplemented!("NullSv2TemplateDistributionServerHandler does not implement handle_request_submit_solution");
    }
}
