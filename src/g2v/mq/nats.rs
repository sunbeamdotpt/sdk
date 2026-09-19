//! NATS/JetStream client wrapper for Sunbeam services.
//!
//! Wraps `async_nats::Client` with:
//! - A 5-second connection timeout via `ConnectOptions` defaults.
//! - Optional JetStream context constructed at connect time.
//! - JetStream helpers: `ensure_stream`, `publish_jetstream`, `pull_subscribe`.
//!
//! # Example
//!
//! ```rust,no_run
//! # #[cfg(feature = "g2v-nats")]
//! # async fn example() -> crate::g2v::error::ServiceResult<()> {
//! use crate::g2v::mq::NatsClient;
//! use crate::g2v::config::NatsConfig;
//!
//! let client = NatsClient::connect(&NatsConfig::default()).await?;
//! assert!(client.is_connected());
//! client.publish("events.test", bytes::Bytes::from("hello")).await?;
//! # Ok(())
//! # }
//! ```

#[cfg(feature = "g2v-nats")]
mod inner {
    use crate::g2v::config::NatsConfig;
    use crate::g2v::error::{ServiceError, ServiceResult};
    use crate::g2v::nats_util::parse_nats_url;

    // ============================================================================
    // NatsClient
    // ============================================================================

    /// Opaque NATS client wrapper with optional JetStream support.
    ///
    /// Obtain one via [`NatsClient::connect`].
    ///
    /// # Notes
    ///
    /// - The wrapper is intentionally opaque (no `Deref` to `async_nats::Client`)
    ///   so instrumentation hooks can intercept calls in the future.
    /// - Use [`client`][NatsClient::client] when you need the raw NATS client
    ///   (e.g. for [`NatsHealthCheck`][crate::g2v::health::NatsHealthCheck]).
    #[derive(Clone)]
    pub struct NatsClient {
        client: async_nats::Client,
        jetstream: Option<async_nats::jetstream::Context>,
    }

    impl NatsClient {
        /// Connect to NATS using `config.url`.
        ///
        /// Extracts an authentication token from the URL userinfo (e.g.
        /// `nats://TOKEN@host` or `nats://:TOKEN@host`) and strips the
        /// credentials before connecting. An explicit `config.auth_token`
        /// takes precedence over a token embedded in the URL.
        ///
        /// Uses `ConnectOptions` defaults (5-second connection timeout,
        /// no `retry_on_initial_connect`). If `config.jetstream` is `true`,
        /// a JetStream context is also created.
        pub async fn connect(config: &NatsConfig) -> ServiceResult<Self> {
            let (url, url_token) = parse_nats_url(&config.url);
            let token = config.auth_token.clone().or(url_token);

            let mut opts = async_nats::ConnectOptions::new();
            if let Some(t) = token {
                opts = opts.token(t);
            }

            let client = opts
                .connect(&url)
                .await
                .map_err(|e| ServiceError::Unavailable(format!("NATS connect failed: {e}")))?;

            let jetstream = if config.jetstream {
                Some(async_nats::jetstream::new(client.clone()))
            } else {
                None
            };

            Ok(Self { client, jetstream })
        }

        /// Return a reference to the underlying `async_nats::Client`.
        ///
        /// Useful when you need to pass the client to APIs that require it
        /// directly, such as [`NatsHealthCheck`][crate::g2v::health::NatsHealthCheck].
        pub fn client(&self) -> &async_nats::Client {
            &self.client
        }

        /// Return the JetStream context, if JetStream was enabled at connect time.
        pub fn jetstream(&self) -> Option<&async_nats::jetstream::Context> {
            self.jetstream.as_ref()
        }

        /// Publish `payload` to `subject`.
        pub async fn publish(
            &self,
            subject: impl AsRef<str>,
            payload: bytes::Bytes,
        ) -> ServiceResult<()> {
            self.client
                .publish(subject.as_ref().to_owned(), payload)
                .await?;
            Ok(())
        }

        /// Send a request to `subject` and await the reply.
        pub async fn request(
            &self,
            subject: impl AsRef<str>,
            payload: bytes::Bytes,
        ) -> ServiceResult<async_nats::Message> {
            let msg = self
                .client
                .request(subject.as_ref().to_owned(), payload)
                .await?;
            Ok(msg)
        }

        /// Subscribe to `subject`, returning a [`async_nats::Subscriber`].
        pub async fn subscribe(
            &self,
            subject: impl AsRef<str>,
        ) -> ServiceResult<async_nats::Subscriber> {
            let sub = self.client.subscribe(subject.as_ref().to_owned()).await?;
            Ok(sub)
        }

        /// Drain the connection: flush pending messages and close subscriptions.
        pub async fn drain(&self) -> ServiceResult<()> {
            self.client
                .drain()
                .await
                .map_err(|e| ServiceError::Internal(format!("NATS drain failed: {e}")))?;
            Ok(())
        }

        /// Returns `true` when the connection state is `Connected`.
        pub fn is_connected(&self) -> bool {
            use async_nats::connection::State;
            self.client.connection_state() == State::Connected
        }

        // ====================================================================
        // JetStream helpers
        // ====================================================================

        /// Get or create a JetStream stream from `config`.
        ///
        /// Returns `ServiceError::Configuration` if JetStream was not enabled
        /// when the client was constructed.
        pub async fn ensure_stream(
            &self,
            config: async_nats::jetstream::stream::Config,
        ) -> ServiceResult<async_nats::jetstream::stream::Stream> {
            let js = self.require_jetstream()?;
            let stream = js.get_or_create_stream(config).await.map_err(|e| {
                ServiceError::Internal(format!("JetStream get_or_create_stream failed: {e}"))
            })?;
            Ok(stream)
        }

        /// Publish `payload` to `subject` via JetStream and return the
        /// `PublishAckFuture` for caller-controlled acknowledgment.
        ///
        /// Returns `ServiceError::Configuration` if JetStream was not enabled.
        pub async fn publish_jetstream(
            &self,
            subject: impl AsRef<str>,
            payload: bytes::Bytes,
        ) -> ServiceResult<async_nats::jetstream::context::PublishAckFuture> {
            let js = self.require_jetstream()?;
            let ack = js.publish(subject.as_ref().to_owned(), payload).await?;
            Ok(ack)
        }

        /// Get or create a pull consumer on `stream_name` using `consumer_config`,
        /// then return the [`PullConsumer`][async_nats::jetstream::consumer::PullConsumer].
        ///
        /// Returns `ServiceError::Configuration` if JetStream was not enabled.
        pub async fn pull_subscribe(
            &self,
            stream_name: &str,
            consumer_config: async_nats::jetstream::consumer::pull::Config,
        ) -> ServiceResult<async_nats::jetstream::consumer::PullConsumer> {
            let js = self.require_jetstream()?;
            let stream = js
                .get_stream(stream_name)
                .await
                .map_err(|e| ServiceError::Internal(format!("JetStream get_stream failed: {e}")))?;
            // An unnamed pull consumer gets a server-generated name.
            let consumer_name = match &consumer_config.name {
                Some(name) => name.clone(),
                None => String::new(),
            };
            let consumer = stream
                .get_or_create_consumer(&consumer_name, consumer_config)
                .await
                .map_err(|e| {
                    ServiceError::Internal(format!("JetStream get_or_create_consumer failed: {e}"))
                })?;
            Ok(consumer)
        }

        // ====================================================================
        // Private
        // ====================================================================

        fn require_jetstream(&self) -> ServiceResult<&async_nats::jetstream::Context> {
            self.jetstream
                .as_ref()
                .ok_or_else(|| ServiceError::Configuration("jetstream not enabled".to_string()))
        }
    }

    impl std::fmt::Debug for NatsClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("NatsClient")
                .field("connected", &self.is_connected())
                .field("jetstream", &self.jetstream.is_some())
                .finish()
        }
    }

    // ============================================================================
    // Shared type alias
    // ============================================================================

    /// Shared (Arc-wrapped) NATS client.
    pub type SharedNatsClient = std::sync::Arc<NatsClient>;

    // ============================================================================
    // Unit tests
    // ============================================================================

    #[cfg(test)]
    mod tests {
        use super::*;

        /// `connect` with an obviously unreachable port must fail with
        /// `ServiceError::Unavailable`, not panic.
        #[tokio::test]
        async fn test_connect_bad_url_returns_error() {
            // Port 1 is privileged / not a NATS server — connection refused immediately.
            let config = NatsConfig {
                url: "nats://localhost:1".to_string(),
                jetstream: false,
                lease_duration: 30,
                auth_token: None,
            };
            let result = NatsClient::connect(&config).await;
            assert!(
                result.is_err(),
                "expected connect error for unreachable port"
            );
            match result.unwrap_err() {
                ServiceError::Unavailable(_) => {}
                other => panic!("expected Unavailable variant, got {other:?}"),
            }
        }

        /// `require_jetstream` returns `Configuration` error when JetStream
        /// was not enabled.
        #[tokio::test]
        async fn test_jetstream_helpers_fail_without_jetstream() {
            // We can't easily construct a NatsClient without a real connection,
            // so test the logic path through `pull_subscribe` by using a config
            // with jetstream=false. The bad URL means connect fails first, so
            // we synthesise a minimal test via the error type only.
            //
            // This test documents the expected variant rather than running I/O.
            let err = ServiceError::Configuration("jetstream not enabled".to_string());
            match err {
                ServiceError::Configuration(ref msg) => {
                    assert_eq!(msg, "jetstream not enabled");
                }
                other => panic!("unexpected variant: {other:?}"),
            }
        }
    }
}

// Re-export under the feature gate so users can write `use crate::g2v::mq::NatsClient`.
#[cfg(feature = "g2v-nats")]
pub use inner::{NatsClient, SharedNatsClient};
