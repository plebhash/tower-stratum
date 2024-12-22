use crate::server::service::request::RequestToSv2ServerError;
use crate::server::service::response::ResponseFromSv2Server;

use roles_logic_sv2::mining_sv2::{
    CloseChannel, OpenExtendedMiningChannel, OpenStandardMiningChannel, SetCustomMiningJob,
    SubmitSharesExtended, SubmitSharesStandard, UpdateChannel,
};
use roles_logic_sv2::template_distribution_sv2::{NewTemplate, SetNewPrevHash};

/// Trait that must be implemented in case [`crate::server::service::Sv2ServerService`] supports the Mining subprotocol.
///
/// We assume that it will keep a state for every client, where each client_id is in sync with the client_id of the
/// [`crate::server::service::Sv2ServerServiceClient`] in the [`crate::server::service::Sv2ServerService`].
///
/// Removing a client on [`crate::server::service::Sv2ServerService`] also triggers removing the client on this handler.
pub trait Sv2MiningServerHandler {
    fn setup_connection_success_flags(&self) -> u32;

    fn add_client(
        &mut self,
        client_id: u32,
        flags: u32,
    ) -> impl std::future::Future<Output = ()> + Send;

    fn remove_client(&mut self, client_id: u32) -> impl std::future::Future<Output = ()> + Send;

    fn remove_all_clients(&mut self) -> impl std::future::Future<Output = ()> + Send;

    fn handle_open_standard_mining_channel(
        &self,
        client_id: u32,
        m: OpenStandardMiningChannel<'static>,
    ) -> impl std::future::Future<
        Output = Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError>,
    > + Send;

    fn handle_open_extended_mining_channel(
        &self,
        client_id: u32,
        m: OpenExtendedMiningChannel<'static>,
    ) -> impl std::future::Future<
        Output = Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError>,
    > + Send;

    fn handle_update_channel(
        &self,
        client_id: u32,
        m: UpdateChannel<'static>,
    ) -> impl std::future::Future<
        Output = Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError>,
    > + Send;

    fn handle_close_channel(
        &self,
        client_id: u32,
        m: CloseChannel<'static>,
    ) -> impl std::future::Future<
        Output = Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError>,
    > + Send;

    fn handle_submit_shares_standard(
        &self,
        client_id: u32,
        m: SubmitSharesStandard,
    ) -> impl std::future::Future<
        Output = Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError>,
    > + Send;

    fn handle_submit_shares_extended(
        &self,
        client_id: u32,
        m: SubmitSharesExtended<'static>,
    ) -> impl std::future::Future<
        Output = Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError>,
    > + Send;

    fn handle_set_custom_mining_job(
        &self,
        client_id: u32,
        m: SetCustomMiningJob<'static>,
    ) -> impl std::future::Future<
        Output = Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError>,
    > + Send;

    fn on_new_template(
        &self,
        m: NewTemplate<'static>,
    ) -> impl std::future::Future<
        Output = Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError>,
    > + Send;

    fn on_set_new_prev_hash(
        &self,
        m: SetNewPrevHash<'static>,
    ) -> impl std::future::Future<
        Output = Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError>,
    > + Send;
}

// -------------------------------------------------------------------------------------------------
// NullSv2MiningServerHandler
// -------------------------------------------------------------------------------------------------

/// A [`Sv2MiningServerHandler`] implementation that does nothing.
///
/// It should be used when creating a [`crate::server::service::Sv2ServerService`] that
/// does not support the mining subprotocol.
#[derive(Debug, Clone)]
pub struct NullSv2MiningServerHandler;

impl Sv2MiningServerHandler for NullSv2MiningServerHandler {
    /// The subprotocol flags to be used on SetupConnectionSuccess
    fn setup_connection_success_flags(&self) -> u32 {
        unimplemented!("NullSv2MiningServerHandler does not implement return_flags")
    }

    /// Add a client to the subprotocol handler
    async fn add_client(&mut self, _client_id: u32, _flags: u32) {
        unimplemented!("NullSv2MiningServerHandler does not implement add_client")
    }

    /// Remove a client from the subprotocol handler
    async fn remove_client(&mut self, _client_id: u32) {
        unimplemented!("NullSv2MiningServerHandler does not implement remove_client")
    }

    /// Remove all clients from the subprotocol handler
    async fn remove_all_clients(&mut self) {
        unimplemented!("NullSv2MiningServerHandler does not implement remove_all_clients")
    }

    /// Handle an OpenStandardMiningChannel message
    async fn handle_open_standard_mining_channel(
        &self,
        _client_id: u32,
        _m: OpenStandardMiningChannel<'static>,
    ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
        unimplemented!(
            "NullSv2MiningServerHandler does not implement handle_open_standard_mining_channel"
        )
    }

    /// Handle an OpenExtendedMiningChannel message
    async fn handle_open_extended_mining_channel(
        &self,
        _client_id: u32,
        _m: OpenExtendedMiningChannel<'static>,
    ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
        unimplemented!(
            "NullSv2MiningServerHandler does not implement handle_open_extended_mining_channel"
        )
    }

    /// Handle an UpdateChannel message
    async fn handle_update_channel(
        &self,
        _client_id: u32,
        _m: UpdateChannel<'static>,
    ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
        unimplemented!("NullSv2MiningServerHandler does not implement handle_update_channel")
    }

    /// Handle a CloseChannel message
    async fn handle_close_channel(
        &self,
        _client_id: u32,
        _m: CloseChannel<'static>,
    ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
        unimplemented!("NullSv2MiningServerHandler does not implement handle_close_channel")
    }

    /// Handle a SubmitSharesStandard message
    async fn handle_submit_shares_standard(
        &self,
        _client_id: u32,
        _m: SubmitSharesStandard,
    ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
        unimplemented!(
            "NullSv2MiningServerHandler does not implement handle_submit_shares_standard"
        )
    }

    /// Handle a SubmitSharesExtended message
    async fn handle_submit_shares_extended(
        &self,
        _client_id: u32,
        _m: SubmitSharesExtended<'static>,
    ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
        unimplemented!(
            "NullSv2MiningServerHandler does not implement handle_submit_shares_extended"
        )
    }

    /// Handle a SetCustomMiningJob message
    async fn handle_set_custom_mining_job(
        &self,
        _client_id: u32,
        _m: SetCustomMiningJob<'static>,
    ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
        unimplemented!("NullSv2MiningServerHandler does not implement handle_set_custom_mining_job")
    }

    /// Handle a NewTemplate message
    async fn on_new_template(
        &self,
        _m: NewTemplate<'static>,
    ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
        unimplemented!("NullSv2MiningServerHandler does not implement on_new_template")
    }

    /// Handle a SetNewPrevHash message
    async fn on_set_new_prev_hash(
        &self,
        _m: SetNewPrevHash<'static>,
    ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
        unimplemented!("NullSv2MiningServerHandler does not implement on_set_new_prev_hash")
    }
}
