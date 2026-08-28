//! Pure in-memory service registry for the AnCordis extension host.
//!
//! This module is the Phase 6 slice of the migration roadmap: legacy plugin
//! types remain, and one service registers and executes through the Host Bus.
//! The registry is bounded (fixed descriptor sizes, service and method caps,
//! per-service TTL) and deliberately has no worker: execution is a
//! deterministic echo of the routed arguments, which proves the Host Bus
//! routed the call to the right service and method. Callers hold the kernel's
//! [`std::sync::RwLock`] around the registry; the registry itself is
//! lock-free.

use std::collections::BTreeMap;

use serde_json::{json, Value};

/// Default maximum number of concurrently registered services.
pub const DEFAULT_MAX_SERVICES: usize = 32;

/// Default time-to-live for a registered service, in milliseconds.
pub const DEFAULT_TTL_MS: u64 = 60_000;

/// Upper bound shared by every bounded text field on the wire.
const MAX_TEXT_CHARS: usize = 128;

/// A service exposes at most this many methods.
const MAX_METHODS_PER_SERVICE: usize = 32;

/// A service declares at most this many required capabilities.
const MAX_REQUIRED_CAPABILITIES: usize = 16;

/// One callable method exposed by a service.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ServiceMethodDescriptor {
    pub name: String,
    pub description: Option<String>,
}

impl ServiceMethodDescriptor {
    pub fn new(
        name: impl Into<String>,
        description: Option<String>,
    ) -> Result<Self, ServiceRegistryError> {
        let name = name.into();
        if !valid_method_name(&name) {
            return Err(ServiceRegistryError::Invalid(format!(
                "method name must be 1..={MAX_TEXT_CHARS} characters with no control or whitespace characters"
            )));
        }
        if let Some(description) = &description {
            if !valid_bounded_text(description, MAX_TEXT_CHARS) {
                return Err(ServiceRegistryError::Invalid(format!(
                    "method description must be 1..={MAX_TEXT_CHARS} characters with no control characters"
                )));
            }
        }
        Ok(Self { name, description })
    }
}

/// A bounded declaration of one in-memory service.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ServiceDescriptor {
    pub service_id: String,
    pub version: String,
    pub display_name: String,
    pub methods: Vec<ServiceMethodDescriptor>,
    pub required_capabilities: Vec<String>,
}

impl ServiceDescriptor {
    pub fn new(
        service_id: impl Into<String>,
        version: impl Into<String>,
        display_name: impl Into<String>,
        methods: Vec<ServiceMethodDescriptor>,
        required_capabilities: Vec<String>,
    ) -> Result<Self, ServiceRegistryError> {
        let descriptor = Self {
            service_id: service_id.into(),
            version: version.into(),
            display_name: display_name.into(),
            methods,
            required_capabilities,
        };
        descriptor.validate()?;
        Ok(descriptor)
    }

    /// Re-validate a descriptor regardless of how it was constructed. The
    /// fields are public, so `register` calls this again as defense in depth.
    fn validate(&self) -> Result<(), ServiceRegistryError> {
        if !valid_service_id(&self.service_id) {
            return Err(ServiceRegistryError::Invalid(format!(
                "service id must match ^[a-z][a-z0-9]*(?:[._/-][a-z0-9]+)*$ and be 1..={MAX_TEXT_CHARS} characters"
            )));
        }
        if !valid_bounded_text(&self.version, MAX_TEXT_CHARS) {
            return Err(ServiceRegistryError::Invalid(format!(
                "service version must be 1..={MAX_TEXT_CHARS} characters with no control characters"
            )));
        }
        if !valid_bounded_text(&self.display_name, MAX_TEXT_CHARS) {
            return Err(ServiceRegistryError::Invalid(format!(
                "service display name must be 1..={MAX_TEXT_CHARS} characters with no control characters"
            )));
        }
        if self.methods.len() > MAX_METHODS_PER_SERVICE {
            return Err(ServiceRegistryError::Invalid(format!(
                "a service exposes at most {MAX_METHODS_PER_SERVICE} methods"
            )));
        }
        let mut seen = BTreeMap::new();
        for method in &self.methods {
            if !valid_method_name(&method.name) {
                return Err(ServiceRegistryError::Invalid(format!(
                    "service method names must be 1..={MAX_TEXT_CHARS} characters with no control or whitespace characters"
                )));
            }
            if seen.insert(method.name.clone(), ()).is_some() {
                return Err(ServiceRegistryError::Invalid(format!(
                    "service declares duplicate method '{}'",
                    method.name
                )));
            }
        }
        if self.required_capabilities.len() > MAX_REQUIRED_CAPABILITIES {
            return Err(ServiceRegistryError::Invalid(format!(
                "a service declares at most {MAX_REQUIRED_CAPABILITIES} required capabilities"
            )));
        }
        for capability in &self.required_capabilities {
            if !valid_bounded_text(capability, MAX_TEXT_CHARS) {
                return Err(ServiceRegistryError::Invalid(format!(
                    "required capability names must be 1..={MAX_TEXT_CHARS} characters with no control characters"
                )));
            }
        }
        Ok(())
    }
}

/// Failure domain for the in-memory service registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServiceRegistryError {
    DuplicateService(String),
    UnknownService(String),
    UnknownMethod {
        service_id: String,
        method: String,
    },
    Expired {
        service_id: String,
        expired_at_ms: u64,
        now_ms: u64,
    },
    TooManyServices {
        max_services: usize,
    },
    Invalid(String),
}

impl std::fmt::Display for ServiceRegistryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateService(service_id) => {
                write!(formatter, "service {service_id} is already registered")
            }
            Self::UnknownService(service_id) => {
                write!(formatter, "unknown service {service_id}")
            }
            Self::UnknownMethod { service_id, method } => {
                write!(formatter, "unknown method {method} on service {service_id}")
            }
            Self::Expired {
                service_id,
                expired_at_ms,
                now_ms,
            } => write!(
                formatter,
                "service {service_id} expired at {expired_at_ms} ms (now {now_ms} ms)"
            ),
            Self::TooManyServices { max_services } => {
                write!(
                    formatter,
                    "service registry is full (max {max_services} services)"
                )
            }
            Self::Invalid(reason) => {
                write!(formatter, "invalid service descriptor: {reason}")
            }
        }
    }
}

impl std::error::Error for ServiceRegistryError {}

/// Registry bounds validated at construction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceRegistryConfig {
    pub max_services: usize,
    pub ttl_ms: u64,
}

impl ServiceRegistryConfig {
    pub fn new(max_services: usize, ttl_ms: u64) -> Result<Self, ServiceRegistryError> {
        if max_services == 0 {
            return Err(ServiceRegistryError::Invalid(
                "service registry max_services must be non-zero".to_string(),
            ));
        }
        if ttl_ms == 0 {
            return Err(ServiceRegistryError::Invalid(
                "service registry ttl_ms must be non-zero".to_string(),
            ));
        }
        Ok(Self {
            max_services,
            ttl_ms,
        })
    }
}

impl Default for ServiceRegistryConfig {
    fn default() -> Self {
        Self {
            max_services: DEFAULT_MAX_SERVICES,
            ttl_ms: DEFAULT_TTL_MS,
        }
    }
}

struct ServiceRecord {
    descriptor: ServiceDescriptor,
    /// Recorded as part of the registration contract for future audit and
    /// snapshot surfaces; expiry (not age) governs calls in this slice.
    #[allow(dead_code)]
    registered_at_ms: u64,
    expires_at_ms: u64,
}

/// A bounded, in-memory, lock-free registry of services.
///
/// The registry never locks or spawns; the kernel's [`RwLock`] is held by the
/// caller. `register` inserts an entry with a TTL from the config, and `call`
/// routes a method invocation to a deterministic echo until the entry expires.
#[derive(Default)]
pub struct ServiceRegistry {
    services: BTreeMap<String, ServiceRecord>,
    config: ServiceRegistryConfig,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config(config: ServiceRegistryConfig) -> Self {
        Self {
            services: BTreeMap::new(),
            config,
        }
    }

    pub fn config(&self) -> &ServiceRegistryConfig {
        &self.config
    }

    /// Number of registered services, for ledger and test snapshots.
    pub fn service_count(&self) -> usize {
        self.services.len()
    }

    /// Register a validated descriptor, rejecting duplicates and the cap.
    pub fn register(
        &mut self,
        descriptor: ServiceDescriptor,
        now_ms: u64,
    ) -> Result<(), ServiceRegistryError> {
        descriptor.validate()?;
        if self.services.contains_key(&descriptor.service_id) {
            return Err(ServiceRegistryError::DuplicateService(
                descriptor.service_id,
            ));
        }
        if self.services.len() >= self.config.max_services {
            return Err(ServiceRegistryError::TooManyServices {
                max_services: self.config.max_services,
            });
        }
        let expires_at_ms = now_ms.checked_add(self.config.ttl_ms).ok_or_else(|| {
            ServiceRegistryError::Invalid("service registration timestamp overflow".to_string())
        })?;
        self.services.insert(
            descriptor.service_id.clone(),
            ServiceRecord {
                descriptor,
                registered_at_ms: now_ms,
                expires_at_ms,
            },
        );
        Ok(())
    }

    /// Route a method call. This slice has no worker, so execution is a
    /// deterministic echo of the arguments that proves the routing.
    pub fn call(
        &self,
        service_id: &str,
        method: &str,
        args: Value,
        now_ms: u64,
    ) -> Result<Value, ServiceRegistryError> {
        let record = self
            .services
            .get(service_id)
            .ok_or_else(|| ServiceRegistryError::UnknownService(service_id.to_string()))?;
        if now_ms >= record.expires_at_ms {
            return Err(ServiceRegistryError::Expired {
                service_id: service_id.to_string(),
                expired_at_ms: record.expires_at_ms,
                now_ms,
            });
        }
        if !record
            .descriptor
            .methods
            .iter()
            .any(|method_descriptor| method_descriptor.name == method)
        {
            return Err(ServiceRegistryError::UnknownMethod {
                service_id: service_id.to_string(),
                method: method.to_string(),
            });
        }
        Ok(json!({
            "serviceId": service_id,
            "method": method,
            "args": args,
        }))
    }

    /// Snapshot every non-expired registration for `service.list`.
    pub fn list(&self, now_ms: u64) -> Vec<ServiceDescriptor> {
        self.services
            .values()
            .filter(|record| now_ms < record.expires_at_ms)
            .map(|record| record.descriptor.clone())
            .collect()
    }

    /// Remove one registration for `service.unregister`; missing entries are
    /// an error so callers can tell "already gone" from "removed now".
    pub fn unregister(&mut self, service_id: &str) -> Result<(), ServiceRegistryError> {
        self.services
            .remove(service_id)
            .map(|_| ())
            .ok_or_else(|| ServiceRegistryError::UnknownService(service_id.to_string()))
    }
}

fn valid_bounded_text(value: &str, max_chars: usize) -> bool {
    !value.is_empty() && value.chars().count() <= max_chars && !value.chars().any(char::is_control)
}

fn valid_method_name(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= MAX_TEXT_CHARS
        && !value.chars().any(char::is_control)
        && !value.chars().any(char::is_whitespace)
}

/// Matches `/^[a-z][a-z0-9]*(?:[._/-][a-z0-9]+)*$/` with a 1..=128 bound.
fn valid_service_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > MAX_TEXT_CHARS {
        return false;
    }
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    let mut index = 1;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_lowercase() || byte.is_ascii_digit() {
            index += 1;
            continue;
        }
        if !matches!(byte, b'.' | b'_' | b'/' | b'-') {
            return false;
        }
        // A separator must be followed by at least one `[a-z0-9]`.
        let Some(next) = bytes.get(index + 1) else {
            return false;
        };
        if !(next.is_ascii_lowercase() || next.is_ascii_digit()) {
            return false;
        }
        index += 2;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ping() -> ServiceDescriptor {
        ServiceDescriptor::new(
            "anyway.system.ping",
            "1.0.0",
            "Ping",
            vec![ServiceMethodDescriptor::new("ping", None).expect("method")],
            Vec::new(),
        )
        .expect("descriptor")
    }

    #[test]
    fn register_and_call_echoes_args_with_the_routed_identity() {
        let mut registry = ServiceRegistry::new();
        registry.register(ping(), 1_000).expect("registers");
        let result = registry
            .call(
                "anyway.system.ping",
                "ping",
                json!({ "hello": "world" }),
                1_500,
            )
            .expect("call");
        assert_eq!(
            result,
            json!({
                "serviceId": "anyway.system.ping",
                "method": "ping",
                "args": { "hello": "world" },
            })
        );
    }

    #[test]
    fn duplicate_registration_is_rejected() {
        let mut registry = ServiceRegistry::new();
        registry
            .register(ping(), 1_000)
            .expect("first registration");
        assert_eq!(
            registry.register(ping(), 2_000),
            Err(ServiceRegistryError::DuplicateService(
                "anyway.system.ping".to_string()
            ))
        );
    }

    #[test]
    fn unknown_service_and_method_are_rejected() {
        let mut registry = ServiceRegistry::new();
        registry.register(ping(), 1_000).expect("registers");
        assert_eq!(
            registry.call("anyway.system.missing", "ping", Value::Null, 1_500),
            Err(ServiceRegistryError::UnknownService(
                "anyway.system.missing".to_string()
            ))
        );
        assert_eq!(
            registry.call("anyway.system.ping", "missing", Value::Null, 1_500),
            Err(ServiceRegistryError::UnknownMethod {
                service_id: "anyway.system.ping".to_string(),
                method: "missing".to_string(),
            })
        );
    }

    #[test]
    fn expired_services_reject_calls() {
        let mut registry = ServiceRegistry::new();
        registry.register(ping(), 1_000).expect("registers");
        // The default TTL is 60_000 ms: still live just before, expired at.
        registry
            .call(
                "anyway.system.ping",
                "ping",
                Value::Null,
                1_000 + DEFAULT_TTL_MS - 1,
            )
            .expect("still live before expiry");
        assert!(matches!(
            registry.call(
                "anyway.system.ping",
                "ping",
                Value::Null,
                1_000 + DEFAULT_TTL_MS
            ),
            Err(ServiceRegistryError::Expired { .. })
        ));
    }

    #[test]
    fn the_service_cap_is_enforced() {
        let config = ServiceRegistryConfig::new(2, 10_000).expect("config");
        let mut registry = ServiceRegistry::with_config(config);
        registry.register(ping(), 1_000).expect("first service");
        registry
            .register(
                ServiceDescriptor::new(
                    "anyway.system.echo",
                    "1.0.0",
                    "Echo",
                    Vec::new(),
                    Vec::new(),
                )
                .expect("second service"),
                1_000,
            )
            .expect("second service");
        let third = ServiceDescriptor::new(
            "anyway.system.third",
            "1.0.0",
            "Third",
            Vec::new(),
            Vec::new(),
        )
        .expect("third service");
        assert_eq!(
            registry.register(third, 1_000),
            Err(ServiceRegistryError::TooManyServices { max_services: 2 })
        );
    }

    #[test]
    fn invalid_service_ids_are_rejected() {
        for invalid in [
            "Ping",
            "1ping",
            "-ping",
            "ping..pong",
            "ping-",
            "ping/",
            "ping pong",
            "piñg",
            "",
        ] {
            let result = ServiceDescriptor::new(
                invalid,
                "1.0.0",
                "Ping",
                vec![ServiceMethodDescriptor::new("ping", None).expect("method")],
                Vec::new(),
            );
            assert!(
                matches!(result, Err(ServiceRegistryError::Invalid(_))),
                "accepted invalid service id {invalid:?}"
            );
        }
    }

    #[test]
    fn valid_service_ids_with_separators_are_accepted() {
        for valid in ["anyway", "anyway.system.ping", "a.b-c_d/e9", "a1"] {
            ServiceDescriptor::new(valid, "1.0.0", "Ping", Vec::new(), Vec::new())
                .unwrap_or_else(|error| panic!("rejected valid service id {valid:?}: {error}"));
        }
    }

    #[test]
    fn invalid_method_names_are_rejected() {
        let too_long = "p".repeat(MAX_TEXT_CHARS + 1);
        for invalid in [
            String::new(),
            "ping pong".to_string(),
            "ping\t".to_string(),
            too_long,
        ] {
            assert!(
                matches!(
                    ServiceMethodDescriptor::new(invalid.clone(), None),
                    Err(ServiceRegistryError::Invalid(_))
                ),
                "accepted invalid method name {invalid:?}"
            );
        }
        assert!(
            ServiceMethodDescriptor::new("ping", Some("answers on demand".to_string())).is_ok()
        );
    }

    #[test]
    fn registry_errors_stringify_for_transport_boundaries() {
        let message =
            ServiceRegistryError::DuplicateService("anyway.system.ping".to_string()).to_string();
        assert!(message.contains("already registered"), "message: {message}");
        let message = ServiceRegistryError::UnknownMethod {
            service_id: "anyway.system.ping".to_string(),
            method: "missing".to_string(),
        }
        .to_string();
        assert!(message.contains("unknown method"), "message: {message}");
        assert!(message.contains("missing"), "message: {message}");
    }
}
