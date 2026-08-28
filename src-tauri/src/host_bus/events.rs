//! Host bus event surface: `event.subscribe` + `event.publish` + `event.poll`.
//!
//! Pull-based delivery: publish fans an event into every matching active
//! subscription's bounded queue; poll drains one subscription in sequence
//! order. The bus stays lock-free; the kernel's `RwLock<EventBus>` is held by
//! the caller.

use std::sync::RwLock;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::kernel::events::{DeliveryMode, EventBus};
use crate::kernel_commands::{inline_request, HostCallRequest};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventSubscribeRequest {
    pub topic: String,
    /// `"queued"` (default) or `"latest"`.
    #[serde(default = "default_delivery")]
    pub delivery: String,
    pub ttl_ms: u64,
}

fn default_delivery() -> String {
    "queued".to_string()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventPublishRequest {
    pub topic: String,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventPollRequest {
    pub subscription_id: u64,
    #[serde(default)]
    pub limit: usize,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// `event.subscribe` — register a pull subscription with a TTL.
pub fn dispatch_event_subscribe(
    request: &HostCallRequest,
    events: &RwLock<EventBus>,
) -> Result<Value, String> {
    let subscribe = inline_request::<EventSubscribeRequest>(request)
        .map_err(|error| format!("invalid event.subscribe request: {error}"))?;
    let mode = DeliveryMode::from_wire(&subscribe.delivery)
        .ok_or_else(|| "event.subscribe delivery must be 'queued' or 'latest'".to_string())?;
    let now = now_ms();
    let expires_at = now
        .checked_add(subscribe.ttl_ms)
        .ok_or_else(|| "event.subscribe ttl overflow".to_string())?;
    let mut bus = events
        .write()
        .map_err(|_| "event bus lock is poisoned".to_string())?;
    let subscription_id = bus
        .subscribe(subscribe.topic, mode, expires_at, now)
        .map_err(|error| format!("event.subscribe failed: {error}"))?;
    Ok(json!({ "subscriptionId": subscription_id, "expiresAtMs": expires_at }))
}

/// `event.publish` — fan one event out to matching active subscriptions.
pub fn dispatch_event_publish(
    request: &HostCallRequest,
    events: &RwLock<EventBus>,
) -> Result<Value, String> {
    let publish = inline_request::<EventPublishRequest>(request)
        .map_err(|error| format!("invalid event.publish request: {error}"))?;
    let mut bus = events
        .write()
        .map_err(|_| "event bus lock is poisoned".to_string())?;
    let delivered = bus
        .publish(&publish.topic, publish.payload, now_ms())
        .map_err(|error| format!("event.publish failed: {error}"))?;
    Ok(json!({ "topic": publish.topic, "delivered": delivered }))
}

/// `event.poll` — drain one subscription in sequence order.
pub fn dispatch_event_poll(
    request: &HostCallRequest,
    events: &RwLock<EventBus>,
) -> Result<Value, String> {
    let poll = inline_request::<EventPollRequest>(request)
        .map_err(|error| format!("invalid event.poll request: {error}"))?;
    let mut bus = events
        .write()
        .map_err(|_| "event bus lock is poisoned".to_string())?;
    let drained = bus
        .poll(poll.subscription_id, poll.limit, now_ms())
        .map_err(|error| format!("event.poll failed: {error}"))?;
    Ok(json!({ "events": drained }))
}
