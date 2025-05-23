use std::task::{Context, Poll};

use roles_logic_sv2::job_declaration_sv2::{
    AllocateMiningJobToken, DeclareMiningJob, ProvideMissingTransactionsSuccess, SubmitSolutionJd,
};

use crate::client::service::{request::RequestToSv2ClientError, response::ResponseFromSv2Client};

/// Trait that must be implemented in case [`crate::client::service::Sv2ClientService`] supports the Job Declaration protocol
pub trait Sv2JobDeclarationClientHandler {
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), RequestToSv2ClientError>>;

    fn handle_allocate_mining_job_token(
        &mut self,
        message: AllocateMiningJobToken<'static>,
    ) -> impl std::future::Future<
        Output = Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError>,
    > + Send;

    fn handle_declare_mining_job(
        &mut self,
        message: DeclareMiningJob<'static>,
    ) -> impl std::future::Future<
        Output = Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError>,
    > + Send;

    fn handle_provide_missing_transactions_success(
        &mut self,
        message: ProvideMissingTransactionsSuccess<'static>,
    ) -> impl std::future::Future<
        Output = Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError>,
    > + Send;

    fn handle_submit_solution(
        &mut self,
        message: SubmitSolutionJd<'static>,
    ) -> impl std::future::Future<
        Output = Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError>,
    > + Send;
}

// -------------------------------------------------------------------------------------------------
// NullSv2JobDeclarationClientHandler
// -------------------------------------------------------------------------------------------------

/// A [`Sv2JobDeclarationClientHandler`] implementation that does nothing.
///
/// It should be used when creating a [`crate::client::service::Sv2ClientService`] that
/// does not support the Job Declaration protocol.
#[derive(Debug, Clone)]
pub struct NullSv2JobDeclarationClientHandler;

impl Sv2JobDeclarationClientHandler for NullSv2JobDeclarationClientHandler {
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), RequestToSv2ClientError>> {
        unimplemented!("NullSv2JobDeclarationClientHandler does not implement poll_ready");
    }

    async fn handle_allocate_mining_job_token(
        &mut self,
        _message: AllocateMiningJobToken<'static>,
    ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
        unimplemented!("NullSv2JobDeclarationClientHandler does not implement handle_allocate_mining_job_token");
    }

    async fn handle_declare_mining_job(
        &mut self,
        _message: DeclareMiningJob<'static>,
    ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
        unimplemented!(
            "NullSv2JobDeclarationClientHandler does not implement handle_declare_mining_job"
        );
    }

    async fn handle_provide_missing_transactions_success(
        &mut self,
        _message: ProvideMissingTransactionsSuccess<'static>,
    ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
        unimplemented!("NullSv2JobDeclarationClientHandler does not implement handle_provide_missing_transactions_success");
    }
    async fn handle_submit_solution(
        &mut self,
        _message: SubmitSolutionJd<'static>,
    ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
        unimplemented!(
            "NullSv2JobDeclarationClientHandler does not implement handle_submit_solution"
        );
    }
}
