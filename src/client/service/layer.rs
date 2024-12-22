use crate::client::service::config::Sv2ClientServiceConfig;
use crate::client::service::subprotocols::template_distribution::handler::Sv2TemplateDistributionClientHandler;
use crate::client::service::Sv2ClientService;
use tower::Layer;

/// A [`tower::Layer`] implementation that produces [`crate::client::service::Sv2ClientService`].
///
/// This layer wraps the configuration and handlers needed for a Stratum V2 client service.
pub struct Sv2ClientLayer<T> {
    /// Configuration for the Stratum V2 client service
    config: Sv2ClientServiceConfig,
    // todo: mining_handler
    // todo: job_declaration_handler
    /// Handler for Template Distribution protocol events
    template_distribution_handler: T,
}

impl<T> Sv2ClientLayer<T> {
    /// Creates a new Sv2ClientLayer
    pub fn new(config: Sv2ClientServiceConfig, template_distribution_handler: T) -> Self {
        Self {
            config,
            template_distribution_handler,
        }
    }
}

impl<S, T> Layer<S> for Sv2ClientLayer<T>
where
    T: Sv2TemplateDistributionClientHandler + Clone + Send + Sync + 'static,
{
    type Service = Sv2ClientService<T>;

    fn layer(&self, _inner: S) -> Self::Service {
        Sv2ClientService::new(
            self.config.clone(),
            self.template_distribution_handler.clone(),
        )
        .expect("Failed to create Sv2ClientService")
    }
}
