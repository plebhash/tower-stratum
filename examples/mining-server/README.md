# Mining Server Example

This example demonstrates some basic patterns for using `tower-stratum` to build a Sv2 Server Service, with focus on the Mining Protocol.

It listens for clients trying to open Standard Channels (via `OpenStandardMiningChannel`), and simply responds with a `OpenStandardMiningChannel.Success`.

## `config` module

The `config` module is responsible for parsing a `.toml` file, which should contain the basic information needed for the Server Service:

- `listening_port`: the port the server should listen on
- `pub_key`: the public key for the connection encryption
- `priv_key`: the private key for the connection encryption
- `cert_validity`: how many seconds the server certificate should be valid for
- `inactivity_limit`: how many seconds some inactive client is allowed to go for without having its connection closed

## `server` module

The `server` module illustrates how to utilize the `Sv2ServerService` while building some application. Three methods are implemented:
- `new`: loads a `Sv2ServerServiceConfig`
- `start`: calls `Sv2ServerService`'s `start` method
- `shutdown`: calls` Sv2ServerService`'s `shutdown` method

# `handler` module

The `handler` module illustrates how to implement the `Sv2MiningServerHandler` trait, with a function that is responsible for handling the `OpenStandardMiningChannel` message that a Server could potentially receive under the Sv2 Mining Protocol.