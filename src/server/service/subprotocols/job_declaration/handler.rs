use std::task::{Context, Poll};

use roles_logic_sv2::job_declaration_sv2::{
    AllocateMiningJobTokenSuccess, DeclareMiningJobError, DeclareMiningJobSuccess,
    IdentifyTransactions, ProvideMissingTransactions,
};

use crate::server::service::{request::RequestToSv2ServerError, response::ResponseFromSv2Server};

/// Trait that must be implemented in case [`crate::server::service::Sv2ServerService`] supports the Job Declaration subprotocol.

pub trait Sv2JobDeclarationServerHandler {
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), RequestToSv2ServerError>>;

    fn handle_allocate_mining_job_token_success(
        &mut self,
        m: AllocateMiningJobTokenSuccess<'static>,
    ) -> impl std::future::Future<
        Output = Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError>,
    > + Send;

    fn handle_declare_mining_job_success(
        &mut self,
        m: DeclareMiningJobSuccess<'static>,
    ) -> impl std::future::Future<
        Output = Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError>,
    > + Send;

    fn handle_declare_mining_job_error(
        &mut self,
        m: DeclareMiningJobError<'static>,
    ) -> impl std::future::Future<
        Output = Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError>,
    > + Send;

    fn handle_identify_transactions(
        &mut self,
        m: IdentifyTransactions,
    ) -> impl std::future::Future<
        Output = Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError>,
    > + Send;

    fn handle_provide_missing_transactions(
        &mut self,
        m: ProvideMissingTransactions<'static>,
    ) -> impl std::future::Future<
        Output = Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError>,
    > + Send;
}

// -------------------------------------------------------------------------------------------------
// NullSv2JobDeclarationServerHandler
// -------------------------------------------------------------------------------------------------

/// A [`Sv2JobDeclarationServerHandler`] implementation that does nothing.
///
/// It should be used when creating a [`crate::server::service::Sv2ServerService`] that
/// does not support the job declaration subprotocol.

pub struct NullSv2JobDeclarationServerHandler;

impl Sv2JobDeclarationServerHandler for NullSv2JobDeclarationServerHandler {
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), RequestToSv2ServerError>> {
        unimplemented!("NullSv2JobDeclarationServerHandler does not implement poll_ready");
    }

    async fn handle_allocate_mining_job_token_success(
        &mut self,
        _m: AllocateMiningJobTokenSuccess<'static>,
    ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
        unimplemented!("NullSv2JobDeclarationServerHandler does not implement handle_allocate_mining_job_token_success")
    }

    async fn handle_declare_mining_job_success(
        &mut self,
        _m: DeclareMiningJobSuccess<'static>,
    ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
        unimplemented!("NullSv2JobDeclarationServerHandler does not implement handle_declare_mining_job_success")
    }

    async fn handle_declare_mining_job_error(
        &mut self,
        _m: DeclareMiningJobError<'static>,
    ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
        unimplemented!(
            "NullSv2JobDeclarationServerHandler does not implement handle_declare_mining_job_error"
        )
    }

    async fn handle_identify_transactions(
        &mut self,
        _m: IdentifyTransactions,
    ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
        unimplemented!(
            "NullSv2JobDeclarationServerHandler does not implement handle_identify_transactions"
        )
    }

    async fn handle_provide_missing_transactions(
        &mut self,
        _m: ProvideMissingTransactions<'static>,
    ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
        unimplemented!("NullSv2JobDeclarationServerHandler does not implement handle_provide_missing_transactions")
    }
}
