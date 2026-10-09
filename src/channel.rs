// Copyright 2021-2026 ONDEWO GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Channel construction: plaintext, TLS and mutual TLS.
//!
//! This module is HAND-WRITTEN - it is the rust counterpart of `get_secure_channel` /
//! `_get_grpc_channel` of the python `ondewo-client-utils`, and follows the same contract in every
//! ONDEWO SDK:
//!
//! * [`ClientConfig::grpc_cert`] is the PEM **content** of the CA to trust; empty means the
//!   platform's trust store. [`ClientConfig::grpc_client_cert`] / [`ClientConfig::grpc_client_key`]
//!   are the PEM content of the client identity for mutual TLS - both or neither.
//! * Every misconfiguration is refused with a [`ChannelError`] BEFORE tonic sees it: half a client
//!   identity, a client identity on a plaintext channel, a certificate field without a PEM
//!   certificate in it (typically a file path).
//! * No error message and no [`fmt::Debug`] output ever renders a PEM or a key.
//!
//! ```text
//! let config = ClientConfig::new("nlu.example.com", 443)
//!     .with_grpc_cert(std::fs::read_to_string("certs/ca.pem")?)
//!     .with_client_identity(
//!         std::fs::read_to_string("certs/client.pem")?,
//!         std::fs::read_to_string("certs/client.key")?,
//!     );
//! let channel = config.connect().await?;
//! let mut client = ContextsClient::new(channel)
//!     .max_decoding_message_size(MAX_MESSAGE_LENGTH)
//!     .max_encoding_message_size(MAX_MESSAGE_LENGTH);
//! ```

use std::error::Error;
use std::fmt;
use std::net::Ipv6Addr;
use std::time::Duration;

use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity};

/// The largest message the ONDEWO servers accept (`2**31 - 1` bytes), as in the python SDKs.
///
/// tonic limits message sizes per generated client, not per channel (decoding defaults to
/// 4 MiB). Pass this to `max_decoding_message_size` / `max_encoding_message_size` of a client
/// that sends or receives large payloads (audio, model exports).
pub const MAX_MESSAGE_LENGTH: usize = i32::MAX as usize;

/// Idle time before the first TCP keepalive probe; see [`ClientConfig::endpoint`].
pub const TCP_KEEPALIVE: Duration = Duration::from_secs(30);

/// Interval between unanswered TCP keepalive probes.
pub const TCP_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);

/// Unanswered TCP keepalive probes after which the connection counts as dead.
pub const TCP_KEEPALIVE_RETRIES: u32 = 2;

/// What [`fmt::Debug`] prints in place of a non-empty secret.
pub const REDACTED: &str = "***REDACTED***";

/// The marker every PEM certificate block starts with.
const PEM_CERTIFICATE_MARKER: &str = "-----BEGIN CERTIFICATE-----";

/// Where and how to connect to an ONDEWO server.
///
/// [`fmt::Debug`] never renders PEM content: certificates appear as their length, and a non-empty
/// [`grpc_client_key`](Self::grpc_client_key) as `***REDACTED***`. The crate offers no
/// serialization of this type; if you persist one, the key is in it - treat that file as a secret.
#[derive(Clone, PartialEq, Eq)]
pub struct ClientConfig {
    /// Host name or IP address of the server. A bare IPv6 literal (`::1`) is bracketed for you.
    pub host: String,
    /// Port of the server.
    pub port: u16,
    /// PEM content (never a path) of the CA that signed the server certificate. Empty: trust the
    /// platform's certificate store.
    pub grpc_cert: String,
    /// PEM content of the client certificate (chain) for mutual TLS; set together with
    /// [`grpc_client_key`](Self::grpc_client_key), or leave both empty.
    pub grpc_client_cert: String,
    /// PEM content of the private key of [`grpc_client_cert`](Self::grpc_client_cert).
    pub grpc_client_key: String,
    /// `true` (the default) for TLS, `false` for a plaintext channel (not for production).
    pub use_secure_channel: bool,
    /// The name to verify the server certificate against, when it differs from
    /// [`host`](Self::host) - e.g. when connecting by IP to a certificate without an IP SAN. The
    /// counterpart of gRPC's `grpc.ssl_target_name_override`.
    pub tls_domain_name: Option<String>,
}

impl ClientConfig {
    /// A TLS config for `host:port` that trusts the platform's certificate store.
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            grpc_cert: String::new(),
            grpc_client_cert: String::new(),
            grpc_client_key: String::new(),
            use_secure_channel: true,
            tls_domain_name: None,
        }
    }

    /// Trust the CA whose PEM content is `grpc_cert` instead of the platform's store.
    pub fn with_grpc_cert(mut self, grpc_cert: impl Into<String>) -> Self {
        self.grpc_cert = grpc_cert.into();
        self
    }

    /// Present the client identity (PEM certificate chain and PEM private key) for mutual TLS.
    pub fn with_client_identity(
        mut self,
        grpc_client_cert: impl Into<String>,
        grpc_client_key: impl Into<String>,
    ) -> Self {
        self.grpc_client_cert = grpc_client_cert.into();
        self.grpc_client_key = grpc_client_key.into();
        self
    }

    /// `false` for a plaintext channel (not for production).
    pub fn with_use_secure_channel(mut self, use_secure_channel: bool) -> Self {
        self.use_secure_channel = use_secure_channel;
        self
    }

    /// Verify the server certificate against `tls_domain_name` instead of the host.
    pub fn with_tls_domain_name(mut self, tls_domain_name: impl Into<String>) -> Self {
        self.tls_domain_name = Some(tls_domain_name.into());
        self
    }

    /// `host:port`, with a bare IPv6 literal bracketed (`[::1]:50051`). A host that is already
    /// bracketed, or anything else that is not an IPv6 literal, is left as it is.
    pub fn host_and_port(&self) -> String {
        if self.host.parse::<Ipv6Addr>().is_ok() {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    /// Check the config without building anything; [`endpoint`](Self::endpoint) runs it first.
    ///
    /// # Errors
    ///
    /// * [`ChannelError::IncompleteClientIdentity`] - exactly one of `grpc_client_cert` and
    ///   `grpc_client_key` is set.
    /// * [`ChannelError::ClientIdentityWithoutTls`] - a client identity on a plaintext channel,
    ///   which would silently go nowhere.
    /// * [`ChannelError::NotAPemCertificate`] - `grpc_cert` or `grpc_client_cert` is set but holds
    ///   no PEM certificate (typically a file path instead of the file's content).
    pub fn validate(&self) -> Result<(), ChannelError> {
        let has_cert = !self.grpc_client_cert.is_empty();
        let has_key = !self.grpc_client_key.is_empty();
        if has_cert != has_key {
            return Err(ChannelError::IncompleteClientIdentity {
                host_and_port: self.host_and_port(),
                missing: if has_cert {
                    "grpc_client_key"
                } else {
                    "grpc_client_cert"
                },
            });
        }
        if !self.use_secure_channel {
            if has_cert {
                return Err(ChannelError::ClientIdentityWithoutTls {
                    host_and_port: self.host_and_port(),
                });
            }
            return Ok(());
        }
        for (field, pem) in [
            ("grpc_cert", &self.grpc_cert),
            ("grpc_client_cert", &self.grpc_client_cert),
        ] {
            if !pem.is_empty() && !pem.contains(PEM_CERTIFICATE_MARKER) {
                return Err(ChannelError::NotAPemCertificate {
                    host_and_port: self.host_and_port(),
                    field,
                });
            }
        }
        Ok(())
    }

    /// The tonic [`Endpoint`] for this config, not yet connected.
    ///
    /// `https://` with TLS configured for a secure config, `http://` otherwise, and TCP keepalive
    /// ([`TCP_KEEPALIVE`], [`TCP_KEEPALIVE_INTERVAL`], [`TCP_KEEPALIVE_RETRIES`]) so that a
    /// silently dropped connection is noticed. HTTP/2 keepalive pings are deliberately NOT enabled:
    /// hyper offers no cap on pings without data (gRPC's `http2.max_pings_without_data`), and a
    /// default grpc-core server answers a client that keeps pinging a silent stream with GOAWAY
    /// `too_many_pings`. Tune the returned endpoint further as needed.
    ///
    /// A plaintext endpoint logs a `tracing` warning naming `host:port`.
    ///
    /// # Errors
    ///
    /// Everything [`validate`](Self::validate) refuses, plus [`ChannelError::InvalidEndpoint`]
    /// when tonic rejects the target or the TLS material (e.g. a key that is not PEM).
    pub fn endpoint(&self) -> Result<Endpoint, ChannelError> {
        self.validate()?;
        let host_and_port = self.host_and_port();
        let invalid = |source| ChannelError::InvalidEndpoint {
            host_and_port: host_and_port.clone(),
            source,
        };
        let scheme = if self.use_secure_channel {
            "https"
        } else {
            "http"
        };
        let endpoint = Endpoint::from_shared(format!("{scheme}://{host_and_port}"))
            .map_err(invalid)?
            .tcp_keepalive(Some(TCP_KEEPALIVE))
            .tcp_keepalive_interval(Some(TCP_KEEPALIVE_INTERVAL))
            .tcp_keepalive_retries(Some(TCP_KEEPALIVE_RETRIES));
        if !self.use_secure_channel {
            tracing::warn!("Using an INSECURE (plaintext) gRPC channel to {host_and_port}.");
            return Ok(endpoint);
        }
        endpoint.tls_config(self.tls_config()).map_err(invalid)
    }

    /// Connect now; fails if the server cannot be reached or the TLS handshake fails.
    ///
    /// # Errors
    ///
    /// Everything [`endpoint`](Self::endpoint) refuses, plus [`ChannelError::Connect`].
    pub async fn connect(&self) -> Result<Channel, ChannelError> {
        let endpoint = self.endpoint()?;
        endpoint
            .connect()
            .await
            .map_err(|source| ChannelError::Connect {
                host_and_port: self.host_and_port(),
                source,
            })
    }

    /// A channel that connects on its first call. Must be called inside a tokio runtime.
    ///
    /// # Errors
    ///
    /// Everything [`endpoint`](Self::endpoint) refuses.
    pub fn connect_lazy(&self) -> Result<Channel, ChannelError> {
        Ok(self.endpoint()?.connect_lazy())
    }

    fn tls_config(&self) -> ClientTlsConfig {
        let mut tls = if self.grpc_cert.is_empty() {
            ClientTlsConfig::new().with_native_roots()
        } else {
            ClientTlsConfig::new().ca_certificate(Certificate::from_pem(&self.grpc_cert))
        };
        // Both-or-neither is checked in validate(); empty on both means plain TLS.
        if !self.grpc_client_cert.is_empty() {
            tls = tls.identity(Identity::from_pem(
                &self.grpc_client_cert,
                &self.grpc_client_key,
            ));
        }
        // rustls takes the server name without the brackets tonic would take from the URI.
        if let Some(domain_name) = &self.tls_domain_name {
            tls = tls.domain_name(domain_name.clone());
        } else if self.host.parse::<Ipv6Addr>().is_ok() {
            tls = tls.domain_name(self.host.clone());
        }
        tls
    }
}

/// Renders the length of a PEM, never its content.
struct PemLength<'a>(&'a str);

impl fmt::Debug for PemLength<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            formatter.write_str("\"\"")
        } else {
            write!(formatter, "<PEM, {} bytes>", self.0.len())
        }
    }
}

/// Renders a non-empty secret as [`REDACTED`] and an empty one as `""`.
struct Redacted<'a>(&'a str);

impl fmt::Debug for Redacted<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            formatter.write_str("\"\"")
        } else {
            formatter.write_str(REDACTED)
        }
    }
}

impl fmt::Debug for ClientConfig {
    /// Never renders a PEM or the key: a key that reaches a log is a leaked key.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClientConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("grpc_cert", &PemLength(&self.grpc_cert))
            .field("grpc_client_cert", &PemLength(&self.grpc_client_cert))
            .field("grpc_client_key", &Redacted(&self.grpc_client_key))
            .field("use_secure_channel", &self.use_secure_channel)
            .field("tls_domain_name", &self.tls_domain_name)
            .finish()
    }
}

/// Why a channel could not be built. Messages name the field and `host:port`, never a PEM or key.
#[derive(Debug)]
#[non_exhaustive]
pub enum ChannelError {
    /// Exactly one of `grpc_client_cert` and `grpc_client_key` is set.
    IncompleteClientIdentity {
        /// The target of the refused channel.
        host_and_port: String,
        /// The field that is missing.
        missing: &'static str,
    },
    /// A client identity on a plaintext channel.
    ClientIdentityWithoutTls {
        /// The target of the refused channel.
        host_and_port: String,
    },
    /// A certificate field that holds no PEM certificate.
    NotAPemCertificate {
        /// The target of the refused channel.
        host_and_port: String,
        /// The offending field.
        field: &'static str,
    },
    /// tonic rejected the target or the TLS material.
    InvalidEndpoint {
        /// The target of the refused channel.
        host_and_port: String,
        /// tonic's error.
        source: tonic::transport::Error,
    },
    /// Connecting failed: unreachable server, or a failed TLS handshake.
    Connect {
        /// The target of the failed connection.
        host_and_port: String,
        /// tonic's error.
        source: tonic::transport::Error,
    },
}

impl fmt::Display for ChannelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IncompleteClientIdentity {
                host_and_port,
                missing,
            } => write!(
                formatter,
                "the client identity for {host_and_port} is incomplete: {missing} is not set; set \
                 both grpc_client_cert and grpc_client_key for mutual TLS, or neither"
            ),
            Self::ClientIdentityWithoutTls { host_and_port } => write!(
                formatter,
                "the config for {host_and_port} carries a client certificate for mutual TLS, but \
                 use_secure_channel is false and would send it nowhere; use a secure channel"
            ),
            Self::NotAPemCertificate {
                host_and_port,
                field,
            } => write!(
                formatter,
                "{field} for {host_and_port} holds no PEM certificate; pass the PEM content, not \
                 a file path"
            ),
            Self::InvalidEndpoint {
                host_and_port,
                source,
            } => write!(
                formatter,
                "cannot build a channel to {host_and_port}: {source}"
            ),
            Self::Connect {
                host_and_port,
                source,
            } => write!(formatter, "cannot connect to {host_and_port}: {source}"),
        }
    }
}

impl Error for ChannelError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidEndpoint { source, .. } | Self::Connect { source, .. } => Some(source),
            _ => None,
        }
    }
}
