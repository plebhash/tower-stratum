use crate::server::service::config::Sv2ServerServiceConfig;
use crate::server::service::subprotocols::mining::handler::Sv2MiningServerHandler;
use crate::server::service::Sv2ServerService;
use tower::Layer;

/// A [`tower::Layer`] implementer that produces [`crate::server::service::Sv2ServerService`].
#[derive(Clone)]
pub struct Sv2ServerLayer<M>
where
    M: Sv2MiningServerHandler + Clone + Send + Sync + 'static,
{
    config: Sv2ServerServiceConfig,
    mining_handler: M,
}

impl<M> Sv2ServerLayer<M>
where
    M: Sv2MiningServerHandler + Clone + Send + Sync + 'static,
{
    /// Creates a new [`Sv2ServerLayer`]
    pub fn new(config: Sv2ServerServiceConfig, mining_handler: M) -> Self {
        Sv2ServerLayer {
            config,
            mining_handler,
        }
    }
}

impl<S, M> Layer<S> for Sv2ServerLayer<M>
where
    M: Sv2MiningServerHandler + Clone + Send + Sync + 'static,
{
    type Service = Sv2ServerService<M>;

    fn layer(&self, _inner: S) -> Self::Service {
        Sv2ServerService::new(self.config.clone(), self.mining_handler.clone())
            .expect("Failed to create Sv2ServerService")
    }
}
