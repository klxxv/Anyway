//! Bounded in-memory event bus for the host-bus `event.*` surface.
//!
//! Pull-based delivery: `publish` fans an event out to every matching active
//! subscription's bounded queue; `poll` drains one subscription in sequence
//! order. Queues drop the oldest event once full. The bus is lock-free; the
//! kernel's `RwLock<EventBus>` is held by the caller.

use std::collections::{BTreeMap, VecDeque};

use serde_json::Value;

pub const DEFAULT_MAX_SUBSCRIPTIONS: usize = 64;
pub const DEFAULT_MAX_QUEUE_EVENTS: usize = 64;
pub const DEFAULT_SUBSCRIPTION_TTL_MS: u64 = 60_000;

const MAX_TOPIC_CHARS: usize = 128;

/// Delivery policy declared at subscription time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryMode {
    /// Events accumulate in the subscription queue until polled.
    Queued,
    /// Only the latest event per sequence window is retained.
    Latest,
}

impl DeliveryMode {
    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "latest" => Some(Self::Latest),
            _ => None,
        }
    }
}

/// One event retained in a subscription queue.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventRecord {
    pub sequence: u64,
    pub topic: String,
    pub payload: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventBusConfig {
    pub max_subscriptions: usize,
    pub max_queue_events: usize,
    pub ttl_ms: u64,
}

impl Default for EventBusConfig {
    fn default() -> Self {
        Self {
            max_subscriptions: DEFAULT_MAX_SUBSCRIPTIONS,
            max_queue_events: DEFAULT_MAX_QUEUE_EVENTS,
            ttl_ms: DEFAULT_SUBSCRIPTION_TTL_MS,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventBusError {
    Invalid(String),
    TooManySubscriptions,
    UnknownSubscription(u64),
    ExpiredSubscription(u64),
}

impl std::fmt::Display for EventBusError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(reason) => write!(formatter, "invalid event bus input: {reason}"),
            Self::TooManySubscriptions => write!(formatter, "event bus subscription cap reached"),
            Self::UnknownSubscription(id) => write!(formatter, "unknown subscription {id}"),
            Self::ExpiredSubscription(id) => write!(formatter, "subscription {id} expired"),
        }
    }
}

impl std::error::Error for EventBusError {}

struct Subscription {
    topic: String,
    mode: DeliveryMode,
    expires_at_ms: u64,
    queue: VecDeque<EventRecord>,
    next_sequence: u64,
}

#[derive(Default)]
pub struct EventBus {
    config: EventBusConfig,
    next_subscription_id: u64,
    subscriptions: BTreeMap<u64, Subscription>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            config: EventBusConfig::default(),
            next_subscription_id: 0,
            subscriptions: BTreeMap::new(),
        }
    }

    pub fn subscription_count(&self) -> usize {
        self.subscriptions.len()
    }

    fn valid_topic(topic: &str) -> bool {
        !topic.is_empty()
            && topic.chars().count() <= MAX_TOPIC_CHARS
            && !topic.chars().any(char::is_control)
    }

    /// `event.subscribe` — register a pull subscription with a TTL.
    pub fn subscribe(
        &mut self,
        topic: impl Into<String>,
        mode: DeliveryMode,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<u64, EventBusError> {
        let topic = topic.into();
        if !Self::valid_topic(&topic) {
            return Err(EventBusError::Invalid(format!(
                "topic must be 1..={MAX_TOPIC_CHARS} characters with no control characters"
            )));
        }
        if expires_at_ms <= now_ms {
            return Err(EventBusError::Invalid(
                "subscription expiry must be in the future".to_string(),
            ));
        }
        if self.subscriptions.len() >= self.config.max_subscriptions {
            return Err(EventBusError::TooManySubscriptions);
        }
        self.next_subscription_id = self.next_subscription_id.saturating_add(1);
        let id = self.next_subscription_id;
        self.subscriptions.insert(
            id,
            Subscription {
                topic,
                mode,
                expires_at_ms,
                queue: VecDeque::new(),
                next_sequence: 0,
            },
        );
        Ok(id)
    }

    /// `event.publish` — fan one event out to every matching active
    /// subscription. Returns the number of subscriptions the event reached.
    pub fn publish(
        &mut self,
        topic: &str,
        payload: Value,
        now_ms: u64,
    ) -> Result<usize, EventBusError> {
        if !Self::valid_topic(topic) {
            return Err(EventBusError::Invalid(format!(
                "topic must be 1..={MAX_TOPIC_CHARS} characters with no control characters"
            )));
        }
        let mut delivered = 0usize;
        for subscription in self.subscriptions.values_mut() {
            if subscription.topic != topic || now_ms >= subscription.expires_at_ms {
                continue;
            }
            subscription.next_sequence = subscription.next_sequence.saturating_add(1);
            if subscription.mode == DeliveryMode::Latest {
                subscription.queue.clear();
            }
            subscription.queue.push_back(EventRecord {
                sequence: subscription.next_sequence,
                topic: topic.to_string(),
                payload: payload.clone(),
            });
            while subscription.queue.len() > self.config.max_queue_events {
                subscription.queue.pop_front();
            }
            delivered += 1;
        }
        Ok(delivered)
    }

    /// `event.poll` — drain one subscription in sequence order.
    pub fn poll(
        &mut self,
        subscription_id: u64,
        limit: usize,
        now_ms: u64,
    ) -> Result<Vec<EventRecord>, EventBusError> {
        let subscription = self
            .subscriptions
            .get_mut(&subscription_id)
            .ok_or(EventBusError::UnknownSubscription(subscription_id))?;
        if now_ms >= subscription.expires_at_ms {
            return Err(EventBusError::ExpiredSubscription(subscription_id));
        }
        let limit = limit.min(self.config.max_queue_events);
        let drained: Vec<EventRecord> = subscription
            .queue
            .drain(..limit.min(subscription.queue.len()))
            .collect();
        Ok(drained)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn publish_fans_out_to_matching_subscriptions() {
        let mut bus = EventBus::new();
        let alpha = bus
            .subscribe("project.saved", DeliveryMode::Queued, 10_000, 0)
            .unwrap();
        let beta = bus
            .subscribe("project.saved", DeliveryMode::Queued, 10_000, 0)
            .unwrap();
        let other = bus
            .subscribe("project.deleted", DeliveryMode::Queued, 10_000, 0)
            .unwrap();

        let delivered = bus
            .publish("project.saved", json!({"id": "p1"}), 100)
            .unwrap();
        assert_eq!(delivered, 2);

        assert_eq!(bus.poll(alpha, 8, 200).unwrap().len(), 1);
        assert_eq!(bus.poll(beta, 8, 200).unwrap().len(), 1);
        assert!(bus.poll(other, 8, 200).unwrap().is_empty());
    }

    #[test]
    fn latest_mode_keeps_only_the_newest_event() {
        let mut bus = EventBus::new();
        let id = bus
            .subscribe("metric", DeliveryMode::Latest, 10_000, 0)
            .unwrap();
        bus.publish("metric", json!({"v": 1}), 100).unwrap();
        bus.publish("metric", json!({"v": 2}), 200).unwrap();
        let events = bus.poll(id, 8, 300).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload, json!({"v": 2}));
    }

    #[test]
    fn expired_subscription_rejects_poll() {
        let mut bus = EventBus::new();
        let id = bus
            .subscribe("topic", DeliveryMode::Queued, 1_000, 0)
            .unwrap();
        assert!(matches!(
            bus.poll(id, 8, 2_000),
            Err(EventBusError::ExpiredSubscription(_))
        ));
    }
}
