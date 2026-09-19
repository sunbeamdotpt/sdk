//! Message queue utilities for Sunbeam services.

pub mod nats;

use crate::g2v::error::ServiceResult;
use std::sync::Arc;

/// Re-export NATS types.
#[cfg(feature = "g2v-nats")]
pub use nats::{NatsClient, SharedNatsClient};

/// Message queue trait.
#[async_trait::async_trait]
pub trait MessageQueue: Send + Sync + 'static {
    /// Publish a message to a topic.
    async fn publish(&self, topic: &str, message: Vec<u8>) -> ServiceResult<()>;

    /// Subscribe to a topic.
    async fn subscribe(&self, topic: &str) -> ServiceResult<Box<dyn MessageStream>>;

    /// Check if the queue is healthy.
    async fn health_check(&self) -> ServiceResult<()>;
}

/// Message stream trait.
#[async_trait::async_trait]
pub trait MessageStream: Send + Sync + 'static {
    /// Receive the next message.
    async fn next(&mut self) -> ServiceResult<Option<Message>>;
}

/// A message from the queue.
#[derive(Debug, Clone)]
pub struct Message {
    /// The message payload.
    pub payload: Vec<u8>,
    /// The message subject/topic.
    pub subject: String,
    /// Message headers.
    pub headers: std::collections::HashMap<String, String>,
}

impl Message {
    /// Create a new message.
    pub fn new(subject: impl Into<String>, payload: Vec<u8>) -> Self {
        Self {
            payload,
            subject: subject.into(),
            headers: std::collections::HashMap::new(),
        }
    }

    /// Add a header.
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }
}

/// Boxed message queue.
pub type BoxedMessageQueue = Box<dyn MessageQueue>;

/// Shared message queue.
pub type SharedMessageQueue = Arc<dyn MessageQueue>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = Message::new("test.subject", b"hello world".to_vec());
        assert_eq!(msg.subject, "test.subject");
        assert_eq!(msg.payload, b"hello world");
    }

    #[test]
    fn test_message_with_header() {
        let msg = Message::new("test.subject", b"hello".to_vec())
            .with_header("content-type", "text/plain");
        assert_eq!(
            msg.headers.get("content-type"),
            Some(&"text/plain".to_string())
        );
    }
}
