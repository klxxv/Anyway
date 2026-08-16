//! Kernel-owned capability policy.
//!
//! The policy is deliberately independent from Tauri, process management, and
//! transport code. It turns a requested operation into a required capability,
//! attenuates a kernel grant into a lease, and authorizes only active leases
//! or explicit native bootstrap grants.

use std::{collections::BTreeMap, fmt};

use super::identity::{Capability, CapabilityLease, CapabilityScope, PrincipalId};

pub const KERNEL_PRINCIPAL_NAME: &str = "kernel";
pub const NATIVE_UI_PRINCIPAL_NAME: &str = "native.ui";
pub const PLUGIN_CATALOG_READ_OPERATION: &str = "plugin.catalog.read";
pub const PLUGIN_LIST_OPERATION: &str = "plugin.list";

/// Stable policy failures returned to a gateway or a supervisor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolicyError {
    InvalidArgument(&'static str),
    UnknownOperation(String),
    RequestedCapabilityMismatch,
    RequestedScopeExpansion,
    RequestedExpiryExpansion,
    IssuerNotKernel,
    GrantPrincipalMismatch,
    GrantInactive,
    PluginLeaseMustExpire,
    LeaseIdZero,
    DuplicateLease(u64),
    UnknownLease(u64),
    LeasePrincipalMismatch,
    LeaseInactive,
    LeaseRequired,
    CapabilityDenied,
    LeaseExpiryExpansion,
}

impl fmt::Display for PolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidArgument(message) => formatter.write_str(message),
            Self::UnknownOperation(operation) => {
                write!(formatter, "unknown operation: {operation}")
            }
            Self::RequestedCapabilityMismatch => {
                formatter.write_str("requested capability does not match operation")
            }
            Self::RequestedScopeExpansion => {
                formatter.write_str("requested or granted scope expands the operation")
            }
            Self::RequestedExpiryExpansion => {
                formatter.write_str("granted expiry expands the requested expiry")
            }
            Self::IssuerNotKernel => {
                formatter.write_str("only the kernel principal may grant leases")
            }
            Self::GrantPrincipalMismatch => {
                formatter.write_str("grant principal does not match request")
            }
            Self::GrantInactive => formatter.write_str("capability grant is not active"),
            Self::PluginLeaseMustExpire => {
                formatter.write_str("plugin capability leases must expire")
            }
            Self::LeaseIdZero => formatter.write_str("lease id must be non-zero"),
            Self::DuplicateLease(lease_id) => write!(formatter, "duplicate lease: {lease_id}"),
            Self::UnknownLease(lease_id) => write!(formatter, "unknown lease: {lease_id}"),
            Self::LeasePrincipalMismatch => {
                formatter.write_str("lease principal does not match caller")
            }
            Self::LeaseInactive => formatter.write_str("capability lease is not active"),
            Self::LeaseRequired => {
                formatter.write_str("plugin authorization requires an explicit lease")
            }
            Self::CapabilityDenied => formatter.write_str("required capability is denied"),
            Self::LeaseExpiryExpansion => formatter.write_str("lease expiry expands its grant"),
        }
    }
}

impl std::error::Error for PolicyError {}

/// The capability and scope required by one kernel operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationRequirement {
    operation: &'static str,
    capability: Capability,
    scope: CapabilityScope,
}

impl OperationRequirement {
    pub fn operation(&self) -> &'static str {
        self.operation
    }

    pub fn capability(&self) -> &Capability {
        &self.capability
    }

    pub fn scope(&self) -> &CapabilityScope {
        &self.scope
    }
}

/// The caller's initial request. A request carries intent, not authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityRequest {
    operation: String,
    principal: PrincipalId,
    capability: Capability,
    scope: CapabilityScope,
    requested_expires_at: Option<u64>,
}

impl CapabilityRequest {
    pub fn operation(&self) -> &str {
        &self.operation
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

    pub fn requested_expires_at(&self) -> Option<u64> {
        self.requested_expires_at
    }
}

/// A kernel-approved, but not yet active, attenuated capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityGrant {
    operation: String,
    principal: PrincipalId,
    capability: Capability,
    scope: CapabilityScope,
    issued_at: u64,
    max_expires_at: Option<u64>,
    source: GrantSource,
}

impl CapabilityGrant {
    pub fn operation(&self) -> &str {
        &self.operation
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

    pub fn issued_at(&self) -> u64 {
        self.issued_at
    }

    pub fn max_expires_at(&self) -> Option<u64> {
        self.max_expires_at
    }

    pub fn source(&self) -> GrantSource {
        self.source
    }

    pub fn is_active_at(&self, epoch: u64) -> bool {
        epoch >= self.issued_at
            && self
                .max_expires_at
                .is_none_or(|expires_at| epoch < expires_at)
    }
}

/// Why the kernel accepted a capability grant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrantSource {
    NativeBootstrap,
    KernelExplicit,
}

/// The proof selected by the final authorization decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorizationSource {
    NativeBootstrap,
    Lease(u64),
}

/// A successful active authorization returned to the Host Bus.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Authorization {
    operation: String,
    principal: PrincipalId,
    capability: Capability,
    scope: CapabilityScope,
    epoch: u64,
    source: AuthorizationSource,
}

impl Authorization {
    pub fn operation(&self) -> &str {
        &self.operation
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

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn source(&self) -> AuthorizationSource {
        self.source
    }
}

/// In-memory policy state owned by the kernel.
pub struct CapabilityPolicy {
    kernel_principal: PrincipalId,
    native_ui_principal: PrincipalId,
    bootstrap_grants: Vec<CapabilityGrant>,
    leases: BTreeMap<u64, CapabilityLease>,
}

impl CapabilityPolicy {
    /// Build the default policy with the canonical kernel and native UI actors.
    pub fn new() -> Self {
        Self::with_principals(
            PrincipalId::new(KERNEL_PRINCIPAL_NAME).expect("constant kernel principal is valid"),
            PrincipalId::new(NATIVE_UI_PRINCIPAL_NAME)
                .expect("constant native UI principal is valid"),
        )
    }

    /// Build a policy for a transport-bound native UI principal.
    pub fn with_principals(
        kernel_principal: PrincipalId,
        native_ui_principal: PrincipalId,
    ) -> Self {
        let bootstrap_operations = [
            (
                PLUGIN_CATALOG_READ_OPERATION,
                custom_capability("plugin.catalog.read"),
            ),
            ("blob.read", Capability::BlobRead),
            ("graph.read", Capability::GraphRead),
            ("ui.register", Capability::UiRegister),
            ("rpc.invoke", Capability::RpcInvoke),
            ("service.register", custom_capability("service.register")),
            ("service.call", custom_capability("service.call")),
            (
                "plugin.settings.read",
                custom_capability("plugin.settings.read"),
            ),
            (
                "plugin.settings.write",
                custom_capability("plugin.settings.write"),
            ),
            (
                "plugin.settings.reset",
                custom_capability("plugin.settings.reset"),
            ),
            ("project.save", custom_capability("project.save")),
            ("project.import", custom_capability("project.import")),
            (
                "workspace.folder.list",
                custom_capability("workspace.folder.list"),
            ),
            (
                "workspace.git.read",
                custom_capability("workspace.git.read"),
            ),
            (
                "workspace.github.read",
                custom_capability("workspace.github.read"),
            ),
            (
                "plugin.icon-theme.read",
                custom_capability("plugin.icon-theme.read"),
            ),
            ("agent.job.status", custom_capability("agent.job.status")),
            ("agent.job.list", custom_capability("agent.job.list")),
            (
                "agent.batch.status",
                custom_capability("agent.batch.status"),
            ),
        ];
        let bootstrap_grants = bootstrap_operations
            .into_iter()
            .map(|(operation, capability)| CapabilityGrant {
                operation: operation.to_string(),
                principal: native_ui_principal.clone(),
                capability,
                scope: CapabilityScope::Global,
                issued_at: 0,
                max_expires_at: None,
                source: GrantSource::NativeBootstrap,
            })
            .collect();

        Self {
            kernel_principal,
            native_ui_principal,
            bootstrap_grants,
            leases: BTreeMap::new(),
        }
    }

    pub fn kernel_principal(&self) -> &PrincipalId {
        &self.kernel_principal
    }

    pub fn native_ui_principal(&self) -> &PrincipalId {
        &self.native_ui_principal
    }

    pub fn bootstrap_grants(&self) -> &[CapabilityGrant] {
        &self.bootstrap_grants
    }

    pub fn lease(&self, lease_id: u64) -> Option<&CapabilityLease> {
        self.leases.get(&lease_id)
    }

    /// Resolve an operation into the capability and scope checked by the kernel.
    pub fn operation_requirement(operation: &str) -> Result<OperationRequirement, PolicyError> {
        let (canonical_operation, capability) = match operation {
            PLUGIN_CATALOG_READ_OPERATION | PLUGIN_LIST_OPERATION => (
                PLUGIN_CATALOG_READ_OPERATION,
                custom_capability("plugin.catalog.read"),
            ),
            "blob.read" => ("blob.read", Capability::BlobRead),
            "blob.write" => ("blob.write", Capability::BlobWrite),
            "rpc.invoke" => ("rpc.invoke", Capability::RpcInvoke),
            "graph.read" => ("graph.read", Capability::GraphRead),
            "graph.patch.propose" => ("graph.patch.propose", Capability::GraphPatchPropose),
            "ui.register" => ("ui.register", Capability::UiRegister),
            "worker.spawn" => ("worker.spawn", Capability::WorkerSpawn),
            "network.request" => ("network.request", Capability::NetworkClient),
            "filesystem.read" => ("filesystem.read", Capability::FilesystemRead),
            "filesystem.write" => ("filesystem.write", Capability::FilesystemWrite),
            "process.spawn" => ("process.spawn", Capability::ProcessSpawn),
            "service.register" => ("service.register", custom_capability("service.register")),
            "service.call" => ("service.call", custom_capability("service.call")),
            "plugin.settings.read" => (
                "plugin.settings.read",
                custom_capability("plugin.settings.read"),
            ),
            "plugin.settings.write" => (
                "plugin.settings.write",
                custom_capability("plugin.settings.write"),
            ),
            "plugin.settings.reset" => (
                "plugin.settings.reset",
                custom_capability("plugin.settings.reset"),
            ),
            "project.save" => ("project.save", custom_capability("project.save")),
            "project.import" => ("project.import", custom_capability("project.import")),
            "workspace.folder.list" => (
                "workspace.folder.list",
                custom_capability("workspace.folder.list"),
            ),
            "workspace.git.read" => (
                "workspace.git.read",
                custom_capability("workspace.git.read"),
            ),
            "workspace.github.read" => (
                "workspace.github.read",
                custom_capability("workspace.github.read"),
            ),
            "plugin.icon-theme.read" => (
                "plugin.icon-theme.read",
                custom_capability("plugin.icon-theme.read"),
            ),
            "agent.job.status" => (
                "agent.job.status",
                custom_capability("agent.job.status"),
            ),
            "agent.job.list" => ("agent.job.list", custom_capability("agent.job.list")),
            "agent.batch.status" => (
                "agent.batch.status",
                custom_capability("agent.batch.status"),
            ),
            other => return Err(PolicyError::UnknownOperation(other.to_string())),
        };

        Ok(OperationRequirement {
            operation: canonical_operation,
            capability,
            scope: CapabilityScope::Global,
        })
    }

    /// Create the requested stage after validating the operation contract.
    pub fn request(
        &self,
        operation: impl Into<String>,
        principal: PrincipalId,
        capability: Capability,
        scope: CapabilityScope,
        requested_expires_at: Option<u64>,
    ) -> Result<CapabilityRequest, PolicyError> {
        let operation = operation.into();
        let requirement = Self::operation_requirement(&operation)?;
        if capability != requirement.capability {
            return Err(PolicyError::RequestedCapabilityMismatch);
        }
        if !scope_covers(&requirement.scope, &scope) {
            return Err(PolicyError::RequestedScopeExpansion);
        }

        Ok(CapabilityRequest {
            operation,
            principal,
            capability,
            scope,
            requested_expires_at,
        })
    }

    /// Turn a request into an attenuated kernel grant.
    pub fn grant(
        &self,
        issuer: &PrincipalId,
        request: &CapabilityRequest,
        granted_scope: CapabilityScope,
        max_expires_at: Option<u64>,
        issued_at: u64,
    ) -> Result<CapabilityGrant, PolicyError> {
        if issuer != &self.kernel_principal {
            return Err(PolicyError::IssuerNotKernel);
        }
        if !scope_covers(&request.scope, &granted_scope) {
            return Err(PolicyError::RequestedScopeExpansion);
        }
        if request
            .requested_expires_at
            .is_some_and(|requested| match max_expires_at {
                Some(granted) => granted > requested,
                None => true,
            })
        {
            return Err(PolicyError::RequestedExpiryExpansion);
        }
        if max_expires_at.is_some_and(|expires_at| expires_at <= issued_at) {
            return Err(PolicyError::InvalidArgument(
                "grant expiry must be after grant issuance",
            ));
        }
        if request
            .requested_expires_at
            .is_some_and(|requested| requested <= issued_at)
        {
            return Err(PolicyError::InvalidArgument(
                "requested expiry must be after grant issuance",
            ));
        }
        if request.principal != self.native_ui_principal && max_expires_at.is_none() {
            return Err(PolicyError::PluginLeaseMustExpire);
        }

        Ok(CapabilityGrant {
            operation: request.operation.clone(),
            principal: request.principal.clone(),
            capability: request.capability.clone(),
            scope: granted_scope,
            issued_at,
            max_expires_at,
            source: GrantSource::KernelExplicit,
        })
    }

    /// Materialize a grant as a revocable lease held by the policy ledger.
    pub fn issue_lease(
        &mut self,
        issuer: &PrincipalId,
        grant: &CapabilityGrant,
        lease_id: u64,
        issued_at: u64,
        expires_at: Option<u64>,
    ) -> Result<CapabilityLease, PolicyError> {
        if issuer != &self.kernel_principal {
            return Err(PolicyError::IssuerNotKernel);
        }
        if grant.principal == self.kernel_principal {
            return Err(PolicyError::GrantPrincipalMismatch);
        }
        if lease_id == 0 {
            return Err(PolicyError::LeaseIdZero);
        }
        if self.leases.contains_key(&lease_id) {
            return Err(PolicyError::DuplicateLease(lease_id));
        }
        if issued_at < grant.issued_at || !grant.is_active_at(issued_at) {
            return Err(PolicyError::GrantInactive);
        }
        if grant
            .max_expires_at
            .is_some_and(|maximum| match expires_at {
                Some(expires_at) => expires_at > maximum,
                None => true,
            })
        {
            return Err(PolicyError::LeaseExpiryExpansion);
        }
        if grant.principal != self.native_ui_principal && expires_at.is_none() {
            return Err(PolicyError::PluginLeaseMustExpire);
        }

        let lease = CapabilityLease::issue(
            lease_id,
            grant.principal.clone(),
            grant.capability.clone(),
            grant.scope.clone(),
            issued_at,
            expires_at,
        )
        .map_err(|_| PolicyError::InvalidArgument("invalid lease range"))?;
        self.leases.insert(lease_id, lease.clone());
        Ok(lease)
    }

    /// Revoke a lease. Revocation is kernel-only and monotonic.
    pub fn revoke_lease(
        &mut self,
        issuer: &PrincipalId,
        lease_id: u64,
        epoch: u64,
    ) -> Result<(), PolicyError> {
        if issuer != &self.kernel_principal {
            return Err(PolicyError::IssuerNotKernel);
        }
        let lease = self
            .leases
            .get_mut(&lease_id)
            .ok_or(PolicyError::UnknownLease(lease_id))?;
        if !lease.revoke(epoch) {
            return Err(PolicyError::LeaseInactive);
        }
        Ok(())
    }

    /// Authorize an operation using native bootstrap or selected active leases.
    pub fn authorize(
        &self,
        operation: &str,
        principal: &PrincipalId,
        selected_lease_ids: &[u64],
        epoch: u64,
    ) -> Result<Authorization, PolicyError> {
        let requirement = Self::operation_requirement(operation)?;

        if selected_lease_ids.is_empty() {
            if principal == &self.native_ui_principal
                && self.bootstrap_grants.iter().any(|grant| {
                    grant.principal == *principal
                        && grant.capability == requirement.capability
                        && scope_covers(&grant.scope, &requirement.scope)
                        && grant.is_active_at(epoch)
                })
            {
                return Ok(Authorization {
                    operation: operation.to_string(),
                    principal: principal.clone(),
                    capability: requirement.capability,
                    scope: requirement.scope,
                    epoch,
                    source: AuthorizationSource::NativeBootstrap,
                });
            }
            if principal != &self.native_ui_principal {
                return Err(PolicyError::LeaseRequired);
            }
            return Err(PolicyError::CapabilityDenied);
        }

        for lease_id in selected_lease_ids {
            let lease = self
                .leases
                .get(lease_id)
                .ok_or(PolicyError::UnknownLease(*lease_id))?;
            if lease.principal() != principal {
                return Err(PolicyError::LeasePrincipalMismatch);
            }
            if !lease.is_active_at(epoch) {
                return Err(PolicyError::LeaseInactive);
            }
            if lease.covers(
                principal,
                &requirement.capability,
                &requirement.scope,
                epoch,
            ) {
                return Ok(Authorization {
                    operation: operation.to_string(),
                    principal: principal.clone(),
                    capability: requirement.capability,
                    scope: requirement.scope,
                    epoch,
                    source: AuthorizationSource::Lease(*lease_id),
                });
            }
        }

        Err(PolicyError::CapabilityDenied)
    }
}

impl Default for CapabilityPolicy {
    fn default() -> Self {
        Self::new()
    }
}

fn custom_capability(name: &str) -> Capability {
    Capability::custom(name).expect("policy capability name is valid")
}

fn scope_covers(granted: &CapabilityScope, requested: &CapabilityScope) -> bool {
    match (granted, requested) {
        (CapabilityScope::Global, _) => true,
        (CapabilityScope::Resource(granted), CapabilityScope::Resource(requested)) => {
            granted == requested
        }
        (CapabilityScope::Resource(_), CapabilityScope::Global) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin(policy: &mut CapabilityPolicy, lease_id: u64, expires_at: u64) -> PrincipalId {
        let principal = PrincipalId::new("plugin.example").expect("plugin principal");
        let request = policy
            .request(
                PLUGIN_CATALOG_READ_OPERATION,
                principal.clone(),
                custom_capability("plugin.catalog.read"),
                CapabilityScope::Global,
                Some(expires_at),
            )
            .expect("request");
        let kernel = policy.kernel_principal().clone();
        let grant = policy
            .grant(
                &kernel,
                &request,
                CapabilityScope::Global,
                Some(expires_at),
                0,
            )
            .expect("grant");
        policy
            .issue_lease(&kernel, &grant, lease_id, 0, Some(expires_at))
            .expect("lease");
        principal
    }

    #[test]
    fn native_bootstrap_authorizes_catalog_read_without_lease() {
        let policy = CapabilityPolicy::new();
        let principal = policy.native_ui_principal().clone();
        let authorization = policy
            .authorize(PLUGIN_CATALOG_READ_OPERATION, &principal, &[], 10)
            .expect("native bootstrap");

        assert_eq!(authorization.source(), AuthorizationSource::NativeBootstrap);
        assert_eq!(authorization.capability().name(), "plugin.catalog.read");
        assert!(policy.bootstrap_grants().iter().any(|grant| {
            grant.operation() == PLUGIN_CATALOG_READ_OPERATION
                && grant.capability().name() == "plugin.catalog.read"
                && grant.source() == GrantSource::NativeBootstrap
        }));
    }

    #[test]
    fn native_bootstrap_authorizes_plugin_settings_read_without_lease() {
        let policy = CapabilityPolicy::new();
        let principal = policy.native_ui_principal().clone();
        let authorization = policy
            .authorize("plugin.settings.read", &principal, &[], 10)
            .expect("native bootstrap");

        assert_eq!(authorization.source(), AuthorizationSource::NativeBootstrap);
        assert_eq!(
            authorization.capability().name(),
            "plugin.settings.read"
        );
        assert!(policy.bootstrap_grants().iter().any(|grant| {
            grant.operation() == "plugin.settings.read"
                && grant.capability().name() == "plugin.settings.read"
                && grant.source() == GrantSource::NativeBootstrap
        }));
    }

    #[test]
    fn native_bootstrap_authorizes_plugin_settings_write_and_reset_without_lease() {
        let policy = CapabilityPolicy::new();
        let principal = policy.native_ui_principal().clone();

        for operation in ["plugin.settings.write", "plugin.settings.reset"] {
            let authorization = policy
                .authorize(operation, &principal, &[], 10)
                .expect("native bootstrap");

            assert_eq!(authorization.source(), AuthorizationSource::NativeBootstrap);
            assert_eq!(authorization.capability().name(), operation);
            assert!(policy.bootstrap_grants().iter().any(|grant| {
                grant.operation() == operation
                    && grant.capability().name() == operation
                    && grant.source() == GrantSource::NativeBootstrap
            }));
        }
    }

    #[test]
    fn native_bootstrap_authorizes_project_save_and_import_without_lease() {
        let policy = CapabilityPolicy::new();
        let principal = policy.native_ui_principal().clone();

        for operation in ["project.save", "project.import"] {
            let authorization = policy
                .authorize(operation, &principal, &[], 10)
                .expect("native bootstrap");

            assert_eq!(authorization.source(), AuthorizationSource::NativeBootstrap);
            assert_eq!(authorization.capability().name(), operation);
            assert!(policy.bootstrap_grants().iter().any(|grant| {
                grant.operation() == operation
                    && grant.capability().name() == operation
                    && grant.source() == GrantSource::NativeBootstrap
            }));
        }
    }

    #[test]
    fn plugin_without_lease_is_rejected() {
        let policy = CapabilityPolicy::new();
        let plugin = PrincipalId::new("plugin.example").expect("plugin principal");

        assert_eq!(
            policy.authorize(PLUGIN_CATALOG_READ_OPERATION, &plugin, &[], 1),
            Err(PolicyError::LeaseRequired)
        );
    }

    #[test]
    fn lease_principal_must_match_caller() {
        let mut policy = CapabilityPolicy::new();
        let _owner = plugin(&mut policy, 7, 100);
        let other = PrincipalId::new("plugin.other").expect("other principal");

        assert_eq!(
            policy.authorize(PLUGIN_CATALOG_READ_OPERATION, &other, &[7], 10),
            Err(PolicyError::LeasePrincipalMismatch)
        );
    }

    #[test]
    fn expired_and_revoked_leases_are_inactive() {
        let mut policy = CapabilityPolicy::new();
        let owner = plugin(&mut policy, 7, 10);

        assert_eq!(
            policy.authorize(PLUGIN_CATALOG_READ_OPERATION, &owner, &[7], 10),
            Err(PolicyError::LeaseInactive)
        );

        let owner = plugin(&mut policy, 8, 100);
        let kernel = policy.kernel_principal().clone();
        policy.revoke_lease(&kernel, 8, 20).expect("revoke");
        assert_eq!(
            policy.authorize(PLUGIN_CATALOG_READ_OPERATION, &owner, &[8], 20),
            Err(PolicyError::LeaseInactive)
        );
    }

    #[test]
    fn unknown_operation_is_rejected() {
        let policy = CapabilityPolicy::new();
        let native = policy.native_ui_principal().clone();

        assert_eq!(
            policy.authorize("unknown.operation", &native, &[], 1),
            Err(PolicyError::UnknownOperation(
                "unknown.operation".to_string()
            ))
        );
    }

    #[test]
    fn grants_and_leases_cannot_expand_capability_scope_or_expiry() {
        let policy = CapabilityPolicy::new();
        let plugin = PrincipalId::new("plugin.example").expect("plugin principal");
        let kernel = policy.kernel_principal().clone();

        assert_eq!(
            policy.request(
                PLUGIN_CATALOG_READ_OPERATION,
                plugin.clone(),
                Capability::BlobRead,
                CapabilityScope::Global,
                Some(10),
            ),
            Err(PolicyError::RequestedCapabilityMismatch)
        );

        let resource_scope = CapabilityScope::resource("catalog:item-1").expect("scope");
        let request = policy
            .request(
                PLUGIN_CATALOG_READ_OPERATION,
                plugin,
                custom_capability("plugin.catalog.read"),
                resource_scope.clone(),
                Some(10),
            )
            .expect("request");
        assert_eq!(
            policy.grant(&kernel, &request, CapabilityScope::Global, Some(10), 0),
            Err(PolicyError::RequestedScopeExpansion)
        );

        let request = policy
            .request(
                PLUGIN_CATALOG_READ_OPERATION,
                PrincipalId::new("plugin.example").expect("plugin principal"),
                custom_capability("plugin.catalog.read"),
                resource_scope,
                Some(10),
            )
            .expect("request");
        assert_eq!(
            policy.grant(
                &PrincipalId::new("plugin.example").expect("plugin principal"),
                &request,
                request.scope().clone(),
                Some(10),
                0,
            ),
            Err(PolicyError::IssuerNotKernel)
        );
        assert_eq!(
            policy.grant(&kernel, &request, request.scope().clone(), Some(11), 0,),
            Err(PolicyError::RequestedExpiryExpansion)
        );
    }

    #[test]
    fn plugin_lease_must_be_explicit_and_bounded() {
        let policy = CapabilityPolicy::new();
        let plugin = PrincipalId::new("plugin.example").expect("plugin principal");
        let request = policy
            .request(
                PLUGIN_CATALOG_READ_OPERATION,
                plugin,
                custom_capability("plugin.catalog.read"),
                CapabilityScope::Global,
                None,
            )
            .expect("request");
        let kernel = policy.kernel_principal().clone();

        assert_eq!(
            policy.grant(&kernel, &request, CapabilityScope::Global, None, 0),
            Err(PolicyError::PluginLeaseMustExpire)
        );
    }
}
