# Template Distribution Client Example

This example demonstrates some basic patterns for using `tower-stratum` to build a Sv2 Client Service, with focus on the Template Distribution Protocol.

It connects to a Sv2 Template Provider Service and simply logs into the terminal the `NewTemplate` and `SetNewPrevHash` messages that it receives.

## `config` module

The `config` module is responsible for parsing a `.toml` file, which should contain the basic information needed for the Client Service:
- `server_addr`: IP and port of the Sv2 Template Provider
- `auth_pk`: authority Public Key of the Sv2 Template Provider (optional)
- `coinbase_output_max_additional_size`: value used to emulate the maximum additional serialized bytes that would be added in coinbase transaction outputs
- `coinbase_output_max_additional_sigops`: value used to emulate the maximum additional sigops that would be added in coinbase transaction outputs

## `client` module

The `client` module illustrates how to utilize the `Sv2ClientService` while building some application. Three methods are implemented:
- `new`: loads a `Sv2ClientServiceConfig`, where the most relevant info are:
  - `supported_protocols`: a reference to the Template Distribution Protocol
  - `template_distribution_config`: a reference to a `Sv2ClientServiceTemplateDistributionConfig` with the necessary data for the `CoinbaseOutputConstraints` message
- `start`: calls `Sv2ClientService`'s `start` method
- `shutdown`: calls` Sv2ClientService`'s `shutdown` method

## `handler` module

The `handler` module illustrates how to implement the `Sv2TemplateDistributionClientHandler` trait, with functions that are responsible for handling the different messages that a Client could potentially receive under the Sv2 Template Distribution Protocol.