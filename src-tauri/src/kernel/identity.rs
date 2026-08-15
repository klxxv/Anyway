//! Kernel identity and capability primitives.
//!
//! This module deliberately contains no process, thread, filesystem, or RPC
//! implementation.  It is the vocabulary that a later kernel bus can use to
//! attribute every request and lease to a stable principal.

use std::{fmt, str::FromStr};

const MAX_IDENTIFIER_LENGTH: usize = 128;

/// Errors returned when a kernel identity or scope is malformed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdentityError {
    Empty(&'static str),
    TooLong(&'static str),
    ControlCharacter(&'static str),
    Whitespace(&'static str),
    InvalidRange(&'static str),
}

impl fmt::Display for IdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty(kind) => write!(formatter, "{kind} must not be empty"),
            Self::TooLong(kind) => write!(
                formatter,
                "{kind} exceeds {MAX_IDENTIFIER_LENGTH} characters"
            ),
            Self::ControlCharacter(kind) => {
                write!(formatter, "{kind} contains a control character")
            }
            Self::Whitespace(kind) => write!(formatter, "{kind} contains whitespace"),
            Self::InvalidRange(kind) => write!(formatter, "{kind} has an invalid range"),
        }
    }
}

impl std::error::Error for IdentityError {}

fn validate_identifier(kind: &'static str, value: &str) -> Result<String, IdentityError> {
    if value.is_empty() {
        return Err(IdentityError::Empty(kind));
    }
    if value.chars().count() > MAX_IDENTIFIER_LENGTH {
        return Err(IdentityError::TooLong(kind));
    }
    if value.chars().any(char::is_control) {
        return Err(IdentityError::ControlCharacter(kind));
    }
    if value.chars().any(char::is_whitespace) {
        return Err(IdentityError::Whitespace(kind));
    }
    Ok(value.to_string())
}

macro_rules! identifier_type {
    ($name:ident, $kind:literal) => {
        #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, IdentityError> {
                Ok(Self(validate_identifier($kind, &value.into())?))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn into_string(self) -> String {
                self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = IdentityError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }
    };
}

/// The security principal attributed to a host, plugin, or kernel-owned actor.
identifier_type!(PrincipalId, "principal id");

/// The supervisor identity of one scheduled worker.
identifier_type!(WorkerId, "worker id");

/// A plugin identity paired with an incarnation nonce.
///
/// The plugin identity is intentionally separate from the instance nonce:
/// restarting a plugin creates a new instance without changing the publisher
/// or plugin-level policy identity.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PluginInstanceId {
    plugin_id: PrincipalId,
    instance: String,
}

impl PluginInstanceId {
    pub fn new(plugin_id: PrincipalId, instance: impl Into<String>) -> Result<Self, IdentityError> {
        Ok(Self {
            plugin_id,
            instance: validate_identifier("plugin instance id", &instance.into())?,
        })
    }

    pub fn plugin_id(&self) -> &PrincipalId {
        &self.plugin_id
    }

    pub fn instance(&self) -> &str {
        &self.instance
    }

    /// A stable, human-readable key for logs and request attribution.
    pub fn stable_key(&self) -> String {
        format!("{}#{}", self.plugin_id, self.instance)
    }
}

impl fmt::Display for PluginInstanceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}#{}", self.plugin_id, self.instance)
    }
}

/// Kernel-recognized capability names.
///
/// `Custom` keeps the kernel protocol extensible, while the policy layer can
/// still reject names it does not understand.  A capability is not a grant;
/// a [`CapabilityLease`] is the time- and principal-bound grant.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Capability {
    BlobRead,
    BlobWrite,
    RpcInvoke,
    GraphRead,
    GraphPatchPropose,
    UiRegister,
    WorkerSpawn,
    NetworkClient,
    FilesystemRead,
    FilesystemWrite,
    ProcessSpawn,
    Custom(String),
}

impl Capability {
    pub fn custom(name: impl Into<String>) -> Result<Self, IdentityError> {
        Ok(Self::Custom(validate_identifier(
            "capability name",
            &name.into(),
        )?))
    }

    pub fn name(&self) -> &str {
        match self {
            Self::BlobRead => "blob.read",
            Self::BlobWrite => "blob.write",
            Self::RpcInvoke => "rpc.invoke",
            Self::GraphRead => "graph.read",
            Self::GraphPatchPropose => "graph.patch.propose",
            Self::UiRegister => "ui.register",
            Self::WorkerSpawn => "worker.spawn",
            Self::NetworkClient => "network.client",
            Self::FilesystemRead => "filesystem.read",
            Self::FilesystemWrite => "filesystem.write",
            Self::ProcessSpawn => "process.spawn",
            Self::Custom(name) => name,
        }
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

impl FromStr for Capability {
    type Err = IdentityError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Ok(match name {
            "blob.read" => Self::BlobRead,
            "blob.write" => Self::BlobWrite,
            "rpc.invoke" => Self::RpcInvoke,
            "graph.read" => Self::GraphRead,
            "graph.patch.propose" => Self::GraphPatchPropose,
            "ui.register" => Self::UiRegister,
            "worker.spawn" => Self::WorkerSpawn,
            "network.client" => Self::NetworkClient,
            "filesystem.read" => Self::FilesystemRead,
            "filesystem.write" => Self::FilesystemWrite,
            "process.spawn" => Self::ProcessSpawn,
            other => Self::custom(other)?,
        })
    }
}

/// Scope attached to a capability lease.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CapabilityScope {
    Global,
    Resource(String),
}

impl CapabilityScope {
    pub fn resource(value: impl Into<String>) -> Result<Self, IdentityError> {
        Ok(Self::Resource(validate_identifier(
            "capability resource",
            &value.into(),
        )?))
    }

    fn covers(&self, requested: &Self) -> bool {
        match (self, requested) {
            (Self::Global, _) => true,
            (Self::Resource(granted), Self::Resource(requested)) => granted == requested,
            (Self::Resource(_), Self::Global) => false,
        }
    }
}

/// A revocable, epoch-based capability grant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityLease {
    lease_id: u64,
    principal: PrincipalId,
    capability: Capability,
    scope: CapabilityScope,
    issued_at: u64,
    expires_at: Option<u64>,
    revoked_at: Option<u64>,
}

impl CapabilityLease {
    pub fn issue(
        lease_id: u64,
        principal: PrincipalId,
        capability: Capability,
        scope: CapabilityScope,
        issued_at: u64,
        expires_at: Option<u64>,
    ) -> Result<Self, IdentityError> {
        if expires_at.is_some_and(|expiry| expiry <= issued_at) {
            return Err(IdentityError::InvalidRange("lease expiry after issuance"));
        }
        Ok(Self {
            lease_id,
            principal,
            capability,
            scope,
            issued_at,
            expires_at,
            revoked_at: None,
        })
    }

    pub fn lease_id(&self) -> u64 {
        self.lease_id
    }

    pub fn principal(&self) -> &PrincipalId {
        &self.principal
    }

    pub fn capability(&self) -> &Capability {
        &self.capability
    }

    pub fn scope(&self) -> &CapabilityScope {
        &self.scope
    }

    pub fn is_active_at(&self, epoch: u64) -> bool {
        self.revoked_at.is_none()
            && epoch >= self.issued_at
            && self.expires_at.is_none_or(|expiry| epoch < expiry)
    }

    pub fn covers(
        &self,
        principal: &PrincipalId,
        capability: &Capability,
        scope: &CapabilityScope,
        epoch: u64,
    ) -> bool {
        self.is_active_at(epoch)
            && &self.principal == principal
            && &self.capability == capability
            && self.scope.covers(scope)
    }

    pub fn revoke(&mut self, epoch: u64) -> bool {
        if self.revoked_at.is_some() {
            return false;
        }
        self.revoked_at = Some(epoch);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_typed_and_displayable() {
        let principal = PrincipalId::new("plugin.anmarket").expect("valid principal");
        let worker = WorkerId::new("worker-1").expect("valid worker");
        let instance =
            PluginInstanceId::new(principal.clone(), "generation-7").expect("valid instance");

        assert_eq!(principal.to_string(), "plugin.anmarket");
        assert_eq!(worker.to_string(), "worker-1");
        assert_eq!(instance.stable_key(), "plugin.anmarket#generation-7");
        assert_eq!(instance.plugin_id(), &principal);
    }

    #[test]
    fn invalid_identifiers_are_rejected() {
        assert_eq!(
            PrincipalId::new(""),
            Err(IdentityError::Empty("principal id"))
        );
        assert!(matches!(
            WorkerId::new("worker id"),
            Err(IdentityError::Whitespace("worker id"))
        ));
        assert!(matches!(
            WorkerId::new("worker\n1"),
            Err(IdentityError::ControlCharacter("worker id"))
        ));
    }

    #[test]
    fn capabilities_round_trip_by_wire_name() {
        let known: Capability = "graph.patch.propose".parse().expect("known capability");
        let custom: Capability = "vendor.audit.read".parse().expect("custom capability");

        assert_eq!(known, Capability::GraphPatchPropose);
        assert_eq!(known.to_string(), "graph.patch.propose");
        assert_eq!(custom.to_string(), "vendor.audit.read");
    }

    #[test]
    fn leases_are_principal_and_scope_bound() {
        let owner = PrincipalId::new("plugin.anmarket").expect("valid owner");
        let other = PrincipalId::new("plugin.other").expect("valid principal");
        let blob = Capability::BlobRead;
        let document = CapabilityScope::resource("blob:document-1").expect("valid scope");
        let mut lease = CapabilityLease::issue(
            7,
            owner.clone(),
            blob.clone(),
            document.clone(),
            10,
            Some(20),
        )
        .expect("valid lease");

        assert!(!lease.covers(&owner, &blob, &document, 9));
        assert!(lease.covers(&owner, &blob, &document, 10));
        assert!(!lease.covers(&other, &blob, &document, 10));
        assert!(!lease.covers(
            &owner,
            &blob,
            &CapabilityScope::resource("blob:document-2").expect("valid scope"),
            10
        ));
        assert!(!lease.covers(&owner, &blob, &document, 20));
        assert!(lease.revoke(12));
        assert!(!lease.covers(&owner, &blob, &document, 12));
        assert!(!lease.revoke(13));
    }

    #[test]
    fn global_lease_covers_resource_requests() {
        let owner = PrincipalId::new("host").expect("valid owner");
        let lease = CapabilityLease::issue(
            1,
            owner.clone(),
            Capability::RpcInvoke,
            CapabilityScope::Global,
            0,
            None,
        )
        .expect("valid lease");
        let resource = CapabilityScope::resource("service.anmarket").expect("valid scope");

        assert!(lease.covers(&owner, &Capability::RpcInvoke, &resource, 1));
    }
}
