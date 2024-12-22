//! # Introduction
//!
//! This crate aims to provide a robust middleware API for building Bitcoin mining apps based on:
//!- [Tower](https://docs.rs/tower/latest/tower/): an asynchronous middleware framework for Rust
//!- [Tokio](https://tokio.rs/): an asynchronous runtime for Rust
//!- [Stratum V2 Reference Implementation](https://github.com/stratum-mining/stratum): the reference implementation of the Stratum V2 protocol
//!
//! The goal is to provide a "batteries-included" approach to implement stateful Sv2 applications.

use async_channel::{Receiver, Sender};
use codec_sv2::StandardEitherFrame;
use framing_sv2::framing::{Frame, Sv2Frame};
use roles_logic_sv2::parsers::{
    AnyMessage, CommonMessages,
    JobDeclaration::{
        AllocateMiningJobToken, AllocateMiningJobTokenSuccess, DeclareMiningJob,
        DeclareMiningJobError, DeclareMiningJobSuccess, IdentifyTransactions,
        IdentifyTransactionsSuccess, ProvideMissingTransactions, ProvideMissingTransactionsSuccess,
        SubmitSolution,
    },
    TemplateDistribution::{self, CoinbaseOutputConstraints},
};

/// Client-side modules for establishing and managing Stratum V2 connections.
///
/// This module provides the client implementation of the Stratum V2 protocol, including:
/// - TCP connection handlers (both encrypted and unencrypted)
/// - Service implementations for different Stratum V2 subprotocols
/// - Configuration options for client behavior
pub mod client;

/// Server-side modules for accepting and handling Stratum V2 client connections.
///
/// This module provides the server implementation of the Stratum V2 protocol, including:
/// - TCP connection handlers (both encrypted and unencrypted)
/// - Service implementations for different Stratum V2 subprotocols
/// - Client session management
/// - Configuration options for server behavior
pub mod server;

/// alias to abstract away [`codec_sv2::StandardEitherFrame`] and [`roles_logic_sv2::parsers::AnyMessage`]
pub type Sv2MessageFrame = StandardEitherFrame<AnyMessage<'static>>;

/// Sv2 Message IO as [`async_channel`] of [`Sv2MessageFrame`]
///
/// Note: [`Sv2MessageIo`] is NOT a [`tower::Service`],
/// but rather a helper primitive to abstract IO from:
/// - [`crate::client::tcp::encrypted::Sv2EncryptedTcpClient`]
/// - [`crate::client::tcp::unencrypted::Sv2UnencryptedTcpClient`]
/// - [`crate::server::tcp::encrypted::start_encrypted_tcp_server`]
/// - [`crate::server::tcp::unencrypted::Sv2UnencryptedTcpServer`]
#[derive(Debug, Clone)]
pub struct Sv2MessageIo {
    // receiver channel of Sv2 Message Frames
    rx: Receiver<Sv2MessageFrame>,
    // sender channel of Sv2 Message Frames
    tx: Sender<Sv2MessageFrame>,
}

impl Sv2MessageIo {
    pub async fn send_message(
        &self,
        message: AnyMessage<'static>,
        msg_type: u8,
    ) -> Result<(), Sv2MessageIoError> {
        let frame = Sv2MessageFrame::Sv2(
            Sv2Frame::from_message(message, msg_type, 0, false)
                .ok_or(Sv2MessageIoError::FrameError)?,
        );
        self.tx
            .send(frame)
            .await
            .map_err(|_| Sv2MessageIoError::SendError)?;
        Ok(())
    }

    pub async fn recv_message(&self) -> Result<(AnyMessage<'static>, u8), Sv2MessageIoError> {
        let frame = self
            .rx
            .recv()
            .await
            .map_err(|_| Sv2MessageIoError::RecvError)?;

        match frame {
            Frame::Sv2(mut frame) => {
                if let Some(header) = frame.get_header() {
                    let message_type = header.msg_type();
                    let mut payload = frame.payload().to_vec();
                    let message: Result<AnyMessage<'_>, _> =
                        (message_type, payload.as_mut_slice()).try_into();
                    match message {
                        Ok(message) => {
                            let message = Self::into_static(message);
                            Ok((message, message_type))
                        }
                        _ => {
                            return Err(Sv2MessageIoError::FrameError);
                        }
                    }
                } else {
                    return Err(Sv2MessageIoError::FrameError);
                }
            }
            Frame::HandShake(_) => {
                return Err(Sv2MessageIoError::FrameError);
            }
        }
    }

    pub fn shutdown(&self) {
        self.tx.close();
        self.rx.close();
    }

    fn into_static(m: AnyMessage<'_>) -> AnyMessage<'static> {
        match m {
            AnyMessage::Mining(m) => AnyMessage::Mining(m.into_static()),
            AnyMessage::Common(m) => match m {
                CommonMessages::ChannelEndpointChanged(m) => {
                    AnyMessage::Common(CommonMessages::ChannelEndpointChanged(m.into_static()))
                }
                CommonMessages::SetupConnection(m) => {
                    AnyMessage::Common(CommonMessages::SetupConnection(m.into_static()))
                }
                CommonMessages::SetupConnectionError(m) => {
                    AnyMessage::Common(CommonMessages::SetupConnectionError(m.into_static()))
                }
                CommonMessages::SetupConnectionSuccess(m) => {
                    AnyMessage::Common(CommonMessages::SetupConnectionSuccess(m.into_static()))
                }
                CommonMessages::Reconnect(m) => {
                    AnyMessage::Common(CommonMessages::Reconnect(m.into_static()))
                }
            },
            AnyMessage::JobDeclaration(m) => match m {
                AllocateMiningJobToken(m) => {
                    AnyMessage::JobDeclaration(AllocateMiningJobToken(m.into_static()))
                }
                AllocateMiningJobTokenSuccess(m) => {
                    AnyMessage::JobDeclaration(AllocateMiningJobTokenSuccess(m.into_static()))
                }
                DeclareMiningJob(m) => {
                    AnyMessage::JobDeclaration(DeclareMiningJob(m.into_static()))
                }
                DeclareMiningJobError(m) => {
                    AnyMessage::JobDeclaration(DeclareMiningJobError(m.into_static()))
                }
                DeclareMiningJobSuccess(m) => {
                    AnyMessage::JobDeclaration(DeclareMiningJobSuccess(m.into_static()))
                }
                IdentifyTransactions(m) => {
                    AnyMessage::JobDeclaration(IdentifyTransactions(m.into_static()))
                }
                IdentifyTransactionsSuccess(m) => {
                    AnyMessage::JobDeclaration(IdentifyTransactionsSuccess(m.into_static()))
                }
                ProvideMissingTransactions(m) => {
                    AnyMessage::JobDeclaration(ProvideMissingTransactions(m.into_static()))
                }
                ProvideMissingTransactionsSuccess(m) => {
                    AnyMessage::JobDeclaration(ProvideMissingTransactionsSuccess(m.into_static()))
                }
                SubmitSolution(m) => AnyMessage::JobDeclaration(SubmitSolution(m.into_static())),
            },
            AnyMessage::TemplateDistribution(m) => match m {
                CoinbaseOutputConstraints(m) => {
                    AnyMessage::TemplateDistribution(CoinbaseOutputConstraints(m.into_static()))
                }
                TemplateDistribution::NewTemplate(m) => AnyMessage::TemplateDistribution(
                    TemplateDistribution::NewTemplate(m.into_static()),
                ),
                TemplateDistribution::RequestTransactionData(m) => {
                    AnyMessage::TemplateDistribution(TemplateDistribution::RequestTransactionData(
                        m.into_static(),
                    ))
                }
                TemplateDistribution::RequestTransactionDataError(m) => {
                    AnyMessage::TemplateDistribution(
                        TemplateDistribution::RequestTransactionDataError(m.into_static()),
                    )
                }
                TemplateDistribution::RequestTransactionDataSuccess(m) => {
                    AnyMessage::TemplateDistribution(
                        TemplateDistribution::RequestTransactionDataSuccess(m.into_static()),
                    )
                }
                TemplateDistribution::SetNewPrevHash(m) => AnyMessage::TemplateDistribution(
                    TemplateDistribution::SetNewPrevHash(m.into_static()),
                ),
                TemplateDistribution::SubmitSolution(m) => AnyMessage::TemplateDistribution(
                    TemplateDistribution::SubmitSolution(m.into_static()),
                ),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum Sv2MessageIoError {
    FrameError,
    SendError,
    RecvError,
}
