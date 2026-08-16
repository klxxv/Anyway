//! Phase-one, in-memory Blob Store model for the Anyway kernel.
//!
//! The store deliberately exposes leases and immutable references instead of
//! filesystem paths. A future persistent backend can implement the same state
//! transitions without changing the kernel-facing contract.

use std::collections::{BTreeMap, BTreeSet};
use std::convert::TryFrom;
use std::fmt;

const MAX_MEDIA_TYPE_LEN: usize = 127;
const MAX_SCOPE_SUBJECT_LEN: usize = 256;
const MAX_OWNER_LEN: usize = 256;

/// Errors returned by the phase-one blob state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlobError {
    InvalidArgument(&'static str),
    LeaseNotFound,
    LeaseExpired,
    BlobNotFound,
    SizeMismatch { expected: u64, actual: u64 },
    UploadIncomplete { expected: u64, actual: u64 },
    QuotaExceeded(&'static str),
    ScopeDenied,
    AddressConflict,
    ReadOutOfRange,
}

impl fmt::Display for BlobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidArgument(message) => write!(formatter, "invalid argument: {message}"),
            Self::LeaseNotFound => formatter.write_str("lease not found"),
            Self::LeaseExpired => formatter.write_str("lease expired"),
            Self::BlobNotFound => formatter.write_str("blob not found"),
            Self::SizeMismatch { expected, actual } => {
                write!(
                    formatter,
                    "size mismatch: expected {expected}, got {actual}"
                )
            }
            Self::UploadIncomplete { expected, actual } => {
                write!(
                    formatter,
                    "upload incomplete: expected {expected}, got {actual}"
                )
            }
            Self::QuotaExceeded(message) => write!(formatter, "quota exceeded: {message}"),
            Self::ScopeDenied => formatter.write_str("blob scope denied"),
            Self::AddressConflict => formatter.write_str("content address conflict"),
            Self::ReadOutOfRange => formatter.write_str("read range out of bounds"),
        }
    }
}

impl std::error::Error for BlobError {}

/// A SHA-256 content address. The digest is the only storage identity.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlobDigest([u8; 32]);

impl BlobDigest {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn sha256(content: &[u8]) -> Self {
        Self(sha256_bytes(content))
    }

    pub fn to_hex(self) -> String {
        let mut output = String::with_capacity(64);
        for byte in self.0 {
            output.push(hex_digit(byte >> 4));
            output.push(hex_digit(byte & 0x0f));
        }
        output
    }
}

impl fmt::Debug for BlobDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("BlobDigest")
            .field(&self.to_hex())
            .finish()
    }
}

impl fmt::Display for BlobDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

/// Scope is part of the reference and is checked again when a read lease opens.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BlobScope {
    Shared,
    Workspace(String),
    Private(String),
}

impl BlobScope {
    pub fn workspace(identifier: impl Into<String>) -> Result<Self, BlobError> {
        let identifier = identifier.into();
        validate_token(&identifier, MAX_SCOPE_SUBJECT_LEN, "workspace scope")?;
        Ok(Self::Workspace(identifier))
    }

    pub fn private(principal: impl Into<String>) -> Result<Self, BlobError> {
        let principal = principal.into();
        validate_token(&principal, MAX_SCOPE_SUBJECT_LEN, "private scope")?;
        Ok(Self::Private(principal))
    }

    fn validate(&self) -> Result<(), BlobError> {
        match self {
            Self::Shared => Ok(()),
            Self::Workspace(identifier) => {
                validate_token(identifier, MAX_SCOPE_SUBJECT_LEN, "workspace scope")
            }
            Self::Private(principal) => {
                validate_token(principal, MAX_SCOPE_SUBJECT_LEN, "private scope")
            }
        }
    }

    fn allows(&self, caller: &str, workspace: Option<&str>) -> bool {
        match self {
            Self::Shared => true,
            Self::Private(owner) => owner == caller,
            Self::Workspace(expected) => workspace == Some(expected.as_str()),
        }
    }

    /// Parse a wire-format scope produced by [`Self::to_wire`]. This is a
    /// metadata helper, not an authorization decision; the read lease opens
    /// only after `open_read` re-checks scope against the bound caller.
    pub fn from_wire(value: &str) -> Result<Self, BlobError> {
        if value == "shared" {
            return Ok(Self::Shared);
        }
        let (kind, subject) = value
            .split_once(':')
            .ok_or(BlobError::InvalidArgument("scope must be shared|workspace:id|private:id"))?;
        match kind {
            "workspace" => Self::workspace(subject),
            "private" => Self::private(subject),
            _ => Err(BlobError::InvalidArgument("unknown scope kind")),
        }
    }

    /// Deterministic wire representation of a scope for one BlobRef round trip.
    pub fn to_wire(&self) -> String {
        match self {
            Self::Shared => "shared".to_string(),
            Self::Workspace(identifier) => format!("workspace:{identifier}"),
            Self::Private(principal) => format!("private:{principal}"),
        }
    }
}

/// Immutable metadata plus the content address. There are no mutating methods.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobRef {
    digest: BlobDigest,
    size: u64,
    media_type: String,
    scope: BlobScope,
}

impl BlobRef {
    pub fn new(
        digest: BlobDigest,
        size: u64,
        media_type: impl Into<String>,
        scope: BlobScope,
    ) -> Result<Self, BlobError> {
        let media_type = media_type.into();
        validate_token(&media_type, MAX_MEDIA_TYPE_LEN, "media type")?;
        scope.validate()?;
        Ok(Self {
            digest,
            size,
            media_type,
            scope,
        })
    }

    pub fn from_content(
        content: &[u8],
        media_type: impl Into<String>,
        scope: BlobScope,
    ) -> Result<Self, BlobError> {
        Self::new(
            BlobDigest::sha256(content),
            content.len() as u64,
            media_type,
            scope,
        )
    }

    pub fn digest(&self) -> BlobDigest {
        self.digest
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    pub fn scope(&self) -> &BlobScope {
        &self.scope
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlobQuota {
    pub max_stored_bytes: u64,
    pub max_blob_bytes: u64,
    pub max_inflight_bytes: u64,
    pub max_active_uploads: usize,
    pub max_read_leases: usize,
    pub max_read_lease_ttl_ms: u64,
}

impl BlobQuota {
    pub const fn new(
        max_stored_bytes: u64,
        max_blob_bytes: u64,
        max_inflight_bytes: u64,
        max_active_uploads: usize,
        max_read_leases: usize,
        max_read_lease_ttl_ms: u64,
    ) -> Self {
        Self {
            max_stored_bytes,
            max_blob_bytes,
            max_inflight_bytes,
            max_active_uploads,
            max_read_leases,
            max_read_lease_ttl_ms,
        }
    }

    fn validate(self) -> Result<Self, BlobError> {
        if self.max_stored_bytes == 0
            || self.max_blob_bytes == 0
            || self.max_inflight_bytes == 0
            || self.max_active_uploads == 0
            || self.max_read_leases == 0
            || self.max_read_lease_ttl_ms == 0
        {
            return Err(BlobError::InvalidArgument("quota values must be non-zero"));
        }
        if self.max_blob_bytes > self.max_inflight_bytes {
            return Err(BlobError::InvalidArgument(
                "max_blob_bytes cannot exceed max_inflight_bytes",
            ));
        }
        Ok(self)
    }
}

impl Default for BlobQuota {
    fn default() -> Self {
        Self::new(
            256 * 1024 * 1024,
            64 * 1024 * 1024,
            128 * 1024 * 1024,
            128,
            256,
            30_000,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UploadLeaseId(u128);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReadLeaseId(u128);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CleanupReport {
    pub expired_uploads: usize,
    pub expired_read_leases: usize,
    pub deleted_blobs: usize,
    pub freed_bytes: u64,
}

struct UploadState {
    owner: String,
    scope: BlobScope,
    media_type: String,
    expected_size: u64,
    expires_at_ms: u64,
    bytes: Vec<u8>,
}

struct StoredBlob {
    media_type: String,
    bytes: Vec<u8>,
    last_access_ms: u64,
}

struct ReadLease {
    digest: BlobDigest,
    owner: String,
    expires_at_ms: u64,
}

/// Deterministic state machine for upload, commit, read and cleanup leases.
pub struct BlobStore {
    quota: BlobQuota,
    next_lease_id: u128,
    inflight_bytes: u64,
    blobs: BTreeMap<BlobDigest, StoredBlob>,
    uploads: BTreeMap<UploadLeaseId, UploadState>,
    reads: BTreeMap<ReadLeaseId, ReadLease>,
}

impl BlobStore {
    pub fn new(quota: BlobQuota) -> Result<Self, BlobError> {
        Ok(Self {
            quota: quota.validate()?,
            next_lease_id: 1,
            inflight_bytes: 0,
            blobs: BTreeMap::new(),
            uploads: BTreeMap::new(),
            reads: BTreeMap::new(),
        })
    }

    pub fn quota(&self) -> BlobQuota {
        self.quota
    }

    pub fn begin_upload(
        &mut self,
        owner: impl Into<String>,
        scope: BlobScope,
        media_type: impl Into<String>,
        expected_size: u64,
        now_ms: u64,
        ttl_ms: u64,
    ) -> Result<UploadLeaseId, BlobError> {
        let owner = owner.into();
        validate_token(&owner, MAX_OWNER_LEN, "upload owner")?;
        scope.validate()?;
        let media_type = media_type.into();
        validate_token(&media_type, MAX_MEDIA_TYPE_LEN, "media type")?;

        if expected_size > self.quota.max_blob_bytes {
            return Err(BlobError::QuotaExceeded("single blob limit"));
        }
        if expected_size > usize::MAX as u64 {
            return Err(BlobError::QuotaExceeded("blob cannot fit in memory"));
        }
        if ttl_ms == 0 {
            return Err(BlobError::InvalidArgument("upload ttl must be non-zero"));
        }
        let expires_at_ms = now_ms
            .checked_add(ttl_ms)
            .ok_or(BlobError::InvalidArgument("upload ttl overflow"))?;
        if self.uploads.len() >= self.quota.max_active_uploads {
            return Err(BlobError::QuotaExceeded("active upload leases"));
        }
        let new_inflight = self
            .inflight_bytes
            .checked_add(expected_size)
            .ok_or(BlobError::QuotaExceeded("inflight bytes overflow"))?;
        if new_inflight > self.quota.max_inflight_bytes {
            return Err(BlobError::QuotaExceeded("inflight bytes"));
        }

        let lease_id = UploadLeaseId(self.allocate_lease_id());
        self.uploads.insert(
            lease_id,
            UploadState {
                owner,
                scope,
                media_type,
                expected_size,
                expires_at_ms,
                bytes: Vec::with_capacity(expected_size as usize),
            },
        );
        self.inflight_bytes = new_inflight;
        Ok(lease_id)
    }

    pub fn upload_chunk(
        &mut self,
        lease_id: UploadLeaseId,
        owner: impl Into<String>,
        chunk: &[u8],
        now_ms: u64,
    ) -> Result<usize, BlobError> {
        let owner = owner.into();
        validate_token(&owner, MAX_OWNER_LEN, "upload owner")?;
        self.expire_upload_if_needed(lease_id, now_ms)?;
        self.ensure_upload_owner(lease_id, &owner)?;
        let state = self
            .uploads
            .get_mut(&lease_id)
            .ok_or(BlobError::LeaseNotFound)?;
        let next_size = state
            .bytes
            .len()
            .checked_add(chunk.len())
            .ok_or(BlobError::QuotaExceeded("upload size overflow"))?;
        if next_size as u64 > state.expected_size {
            return Err(BlobError::SizeMismatch {
                expected: state.expected_size,
                actual: next_size as u64,
            });
        }
        state.bytes.extend_from_slice(chunk);
        Ok(chunk.len())
    }

    pub fn commit_upload(
        &mut self,
        lease_id: UploadLeaseId,
        owner: impl Into<String>,
        now_ms: u64,
    ) -> Result<BlobRef, BlobError> {
        let owner = owner.into();
        validate_token(&owner, MAX_OWNER_LEN, "upload owner")?;
        self.expire_upload_if_needed(lease_id, now_ms)?;
        self.ensure_upload_owner(lease_id, &owner)?;
        let state = self
            .uploads
            .get(&lease_id)
            .ok_or(BlobError::LeaseNotFound)?;
        let actual_size = state.bytes.len() as u64;
        if actual_size != state.expected_size {
            return Err(BlobError::UploadIncomplete {
                expected: state.expected_size,
                actual: actual_size,
            });
        }

        let digest = BlobDigest::sha256(&state.bytes);
        if let Some(existing) = self.blobs.get(&digest) {
            if existing.bytes.len() as u64 != state.expected_size
                || existing.media_type != state.media_type
            {
                return Err(BlobError::AddressConflict);
            }
        } else {
            let total = self
                .stored_bytes()
                .checked_add(state.expected_size)
                .ok_or(BlobError::QuotaExceeded("stored bytes overflow"))?;
            if total > self.quota.max_stored_bytes {
                return Err(BlobError::QuotaExceeded("stored bytes"));
            }
        }

        let upload = self
            .uploads
            .remove(&lease_id)
            .ok_or(BlobError::LeaseNotFound)?;
        self.inflight_bytes = self
            .inflight_bytes
            .checked_sub(upload.expected_size)
            .ok_or(BlobError::InvalidArgument("inflight accounting underflow"))?;
        if !self.blobs.contains_key(&digest) {
            self.blobs.insert(
                digest,
                StoredBlob {
                    media_type: upload.media_type.clone(),
                    bytes: upload.bytes,
                    last_access_ms: now_ms,
                },
            );
        } else if let Some(existing) = self.blobs.get_mut(&digest) {
            existing.last_access_ms = now_ms;
        }

        BlobRef::new(
            digest,
            upload.expected_size,
            upload.media_type,
            upload.scope,
        )
    }

    pub fn abort_upload(
        &mut self,
        lease_id: UploadLeaseId,
        owner: impl Into<String>,
        now_ms: u64,
    ) -> Result<(), BlobError> {
        let owner = owner.into();
        validate_token(&owner, MAX_OWNER_LEN, "upload owner")?;
        self.expire_upload_if_needed(lease_id, now_ms)?;
        self.ensure_upload_owner(lease_id, &owner)?;
        let upload = self
            .uploads
            .remove(&lease_id)
            .ok_or(BlobError::LeaseNotFound)?;
        self.inflight_bytes = self
            .inflight_bytes
            .checked_sub(upload.expected_size)
            .ok_or(BlobError::InvalidArgument("inflight accounting underflow"))?;
        Ok(())
    }

    pub fn open_read(
        &mut self,
        blob: &BlobRef,
        caller: impl Into<String>,
        workspace: Option<&str>,
        now_ms: u64,
        ttl_ms: u64,
    ) -> Result<ReadLeaseId, BlobError> {
        let caller = caller.into();
        validate_token(&caller, MAX_OWNER_LEN, "read caller")?;
        if !blob.scope.allows(&caller, workspace) {
            return Err(BlobError::ScopeDenied);
        }
        if ttl_ms == 0 || ttl_ms > self.quota.max_read_lease_ttl_ms {
            return Err(BlobError::InvalidArgument("read lease ttl outside quota"));
        }
        if self.reads.len() >= self.quota.max_read_leases {
            return Err(BlobError::QuotaExceeded("read leases"));
        }
        let stored = self
            .blobs
            .get_mut(&blob.digest)
            .ok_or(BlobError::BlobNotFound)?;
        if stored.bytes.len() as u64 != blob.size || stored.media_type != blob.media_type {
            return Err(BlobError::AddressConflict);
        }
        stored.last_access_ms = now_ms;
        let expires_at_ms = now_ms
            .checked_add(ttl_ms)
            .ok_or(BlobError::InvalidArgument("read ttl overflow"))?;
        let lease_id = ReadLeaseId(self.allocate_lease_id());
        self.reads.insert(
            lease_id,
            ReadLease {
                digest: blob.digest,
                owner: caller,
                expires_at_ms,
            },
        );
        Ok(lease_id)
    }

    pub fn read_chunk(
        &mut self,
        lease_id: ReadLeaseId,
        caller: impl Into<String>,
        offset: u64,
        max_len: usize,
        now_ms: u64,
    ) -> Result<Vec<u8>, BlobError> {
        let caller = caller.into();
        validate_token(&caller, MAX_OWNER_LEN, "read caller")?;
        self.expire_read_if_needed(lease_id, now_ms)?;
        self.ensure_read_owner(lease_id, &caller)?;
        let digest = self
            .reads
            .get(&lease_id)
            .ok_or(BlobError::LeaseNotFound)?
            .digest;
        let stored = self.blobs.get_mut(&digest).ok_or(BlobError::BlobNotFound)?;
        let offset = usize::try_from(offset).map_err(|_| BlobError::ReadOutOfRange)?;
        if offset > stored.bytes.len() {
            return Err(BlobError::ReadOutOfRange);
        }
        let end = offset.saturating_add(max_len).min(stored.bytes.len());
        stored.last_access_ms = now_ms;
        Ok(stored.bytes[offset..end].to_vec())
    }

    pub fn close_read(
        &mut self,
        lease_id: ReadLeaseId,
        caller: impl Into<String>,
    ) -> Result<(), BlobError> {
        let caller = caller.into();
        validate_token(&caller, MAX_OWNER_LEN, "read caller")?;
        self.ensure_read_owner(lease_id, &caller)?;
        self.reads
            .remove(&lease_id)
            .map(|_| ())
            .ok_or(BlobError::LeaseNotFound)
    }

    /// Remove expired leases and unpinned blobs that have been idle long enough.
    pub fn sweep(&mut self, now_ms: u64, idle_after_ms: u64) -> CleanupReport {
        let mut report = CleanupReport::default();

        let expired_uploads: Vec<_> = self
            .uploads
            .iter()
            .filter_map(|(id, state)| (now_ms >= state.expires_at_ms).then_some(*id))
            .collect();
        for id in expired_uploads {
            if let Some(upload) = self.uploads.remove(&id) {
                self.inflight_bytes = self.inflight_bytes.saturating_sub(upload.expected_size);
                report.expired_uploads += 1;
            }
        }

        let expired_reads: Vec<_> = self
            .reads
            .iter()
            .filter_map(|(id, lease)| (now_ms >= lease.expires_at_ms).then_some(*id))
            .collect();
        for id in expired_reads {
            if self.reads.remove(&id).is_some() {
                report.expired_read_leases += 1;
            }
        }

        let pinned: BTreeSet<_> = self.reads.values().map(|lease| lease.digest).collect();
        let stale: Vec<_> = self
            .blobs
            .iter()
            .filter_map(|(digest, blob)| {
                let idle =
                    now_ms >= blob.last_access_ms && now_ms - blob.last_access_ms >= idle_after_ms;
                (idle && !pinned.contains(digest)).then_some(*digest)
            })
            .collect();
        for digest in stale {
            if let Some(blob) = self.blobs.remove(&digest) {
                report.deleted_blobs += 1;
                report.freed_bytes += blob.bytes.len() as u64;
            }
        }
        report
    }

    pub fn stored_bytes(&self) -> u64 {
        self.blobs
            .values()
            .map(|blob| blob.bytes.len() as u64)
            .sum()
    }

    pub fn inflight_bytes(&self) -> u64 {
        self.inflight_bytes
    }

    pub fn stored_blob_count(&self) -> usize {
        self.blobs.len()
    }

    pub fn active_upload_count(&self) -> usize {
        self.uploads.len()
    }

    pub fn active_read_count(&self) -> usize {
        self.reads.len()
    }

    fn allocate_lease_id(&mut self) -> u128 {
        let id = self.next_lease_id;
        self.next_lease_id = self.next_lease_id.saturating_add(1);
        id
    }

    fn expire_upload_if_needed(
        &mut self,
        lease_id: UploadLeaseId,
        now_ms: u64,
    ) -> Result<(), BlobError> {
        let expired = self
            .uploads
            .get(&lease_id)
            .ok_or(BlobError::LeaseNotFound)?
            .expires_at_ms
            <= now_ms;
        if expired {
            if let Some(upload) = self.uploads.remove(&lease_id) {
                self.inflight_bytes = self.inflight_bytes.saturating_sub(upload.expected_size);
            }
            return Err(BlobError::LeaseExpired);
        }
        Ok(())
    }

    fn expire_read_if_needed(
        &mut self,
        lease_id: ReadLeaseId,
        now_ms: u64,
    ) -> Result<(), BlobError> {
        let expired = self
            .reads
            .get(&lease_id)
            .ok_or(BlobError::LeaseNotFound)?
            .expires_at_ms
            <= now_ms;
        if expired {
            self.reads.remove(&lease_id);
            return Err(BlobError::LeaseExpired);
        }
        Ok(())
    }

    fn ensure_upload_owner(&self, lease_id: UploadLeaseId, owner: &str) -> Result<(), BlobError> {
        let state = self
            .uploads
            .get(&lease_id)
            .ok_or(BlobError::LeaseNotFound)?;
        if state.owner != owner {
            return Err(BlobError::ScopeDenied);
        }
        Ok(())
    }

    fn ensure_read_owner(&self, lease_id: ReadLeaseId, caller: &str) -> Result<(), BlobError> {
        let lease = self.reads.get(&lease_id).ok_or(BlobError::LeaseNotFound)?;
        if lease.owner != caller {
            return Err(BlobError::ScopeDenied);
        }
        Ok(())
    }
}

fn validate_token(value: &str, max_len: usize, field: &'static str) -> Result<(), BlobError> {
    if value.is_empty() || value.len() > max_len || value.chars().any(char::is_control) {
        return Err(BlobError::InvalidArgument(field));
    }
    Ok(())
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'a' + value - 10) as char,
        _ => unreachable!(),
    }
}

fn sha256_bytes(input: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut padded = Vec::with_capacity(input.len() + 72);
    padded.extend_from_slice(input);
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    let mut state = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    for chunk in padded.chunks_exact(64) {
        let mut schedule = [0u32; 64];
        for (index, word) in schedule[..16].iter_mut().enumerate() {
            let offset = index * 4;
            *word = u32::from_be_bytes([
                chunk[offset],
                chunk[offset + 1],
                chunk[offset + 2],
                chunk[offset + 3],
            ]);
        }
        for index in 16..64 {
            let s0 = schedule[index - 15].rotate_right(7)
                ^ schedule[index - 15].rotate_right(18)
                ^ (schedule[index - 15] >> 3);
            let s1 = schedule[index - 2].rotate_right(17)
                ^ schedule[index - 2].rotate_right(19)
                ^ (schedule[index - 2] >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(s0)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(s1);
        }

        let mut working = state;
        for index in 0..64 {
            let choice = (working[4] & working[5]) ^ ((!working[4]) & working[6]);
            let majority =
                (working[0] & working[1]) ^ (working[0] & working[2]) ^ (working[1] & working[2]);
            let sum1 = working[4].rotate_right(6)
                ^ working[4].rotate_right(11)
                ^ working[4].rotate_right(25);
            let sum0 = working[0].rotate_right(2)
                ^ working[0].rotate_right(13)
                ^ working[0].rotate_right(22);
            let temp1 = working[7]
                .wrapping_add(sum1)
                .wrapping_add(choice)
                .wrapping_add(K[index])
                .wrapping_add(schedule[index]);
            let temp2 = sum0.wrapping_add(majority);
            working[7] = working[6];
            working[6] = working[5];
            working[5] = working[4];
            working[4] = working[3].wrapping_add(temp1);
            working[3] = working[2];
            working[2] = working[1];
            working[1] = working[0];
            working[0] = temp1.wrapping_add(temp2);
        }
        for index in 0..8 {
            state[index] = state[index].wrapping_add(working[index]);
        }
    }

    let mut digest = [0u8; 32];
    for (index, word) in state.into_iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> BlobStore {
        BlobStore::new(BlobQuota::new(32, 16, 16, 4, 4, 100)).expect("valid quota")
    }

    #[test]
    fn digest_is_content_address_and_ref_is_immutable_metadata() {
        let content = b"anyway blob";
        let reference = BlobRef::from_content(content, "text/plain", BlobScope::Shared).unwrap();
        assert_eq!(reference.size(), content.len() as u64);
        assert_eq!(reference.media_type(), "text/plain");
        assert_eq!(reference.digest(), BlobDigest::sha256(content));
        assert_eq!(
            reference.digest().to_hex(),
            "f2bc2c1e383ee4708650c7dcf7245fd43cd57d7764b4d5b46626202a9ffea839"
        );
    }

    #[test]
    fn upload_commit_deduplicates_and_enforces_size() {
        let mut store = store();
        let scope = BlobScope::private("plugin.a").unwrap();
        let lease = store
            .begin_upload(
                "plugin.a",
                scope.clone(),
                "application/octet-stream",
                4,
                0,
                50,
            )
            .unwrap();
        store.upload_chunk(lease, "plugin.a", b"ab", 1).unwrap();
        assert_eq!(
            store.commit_upload(lease, "plugin.a", 2),
            Err(BlobError::UploadIncomplete {
                expected: 4,
                actual: 2
            })
        );
        store.upload_chunk(lease, "plugin.a", b"cd", 3).unwrap();
        let first = store.commit_upload(lease, "plugin.a", 4).unwrap();
        assert_eq!(store.stored_bytes(), 4);

        let second_lease = store
            .begin_upload("plugin.a", scope, "application/octet-stream", 4, 5, 50)
            .unwrap();
        store
            .upload_chunk(second_lease, "plugin.a", b"abcd", 6)
            .unwrap();
        let second = store.commit_upload(second_lease, "plugin.a", 7).unwrap();
        assert_eq!(first.digest(), second.digest());
        assert_eq!(store.stored_blob_count(), 1);
        assert_eq!(store.inflight_bytes(), 0);
    }

    #[test]
    fn read_lease_checks_scope_pins_blob_and_expires() {
        let mut store = store();
        let lease = store
            .begin_upload(
                "plugin.a",
                BlobScope::private("plugin.a").unwrap(),
                "text/plain",
                3,
                0,
                50,
            )
            .unwrap();
        store.upload_chunk(lease, "plugin.a", b"abc", 1).unwrap();
        let reference = store.commit_upload(lease, "plugin.a", 2).unwrap();
        assert_eq!(
            store.open_read(&reference, "plugin.b", None, 3, 20),
            Err(BlobError::ScopeDenied)
        );
        let read = store
            .open_read(&reference, "plugin.a", None, 3, 20)
            .unwrap();
        assert_eq!(store.read_chunk(read, "plugin.a", 1, 2, 4).unwrap(), b"bc");
        let report = store.sweep(10, 0);
        assert_eq!(report.deleted_blobs, 0);
        assert_eq!(
            store.read_chunk(read, "plugin.a", 0, 1, 24),
            Err(BlobError::LeaseExpired)
        );
        let report = store.sweep(25, 0);
        assert_eq!(report.deleted_blobs, 1);
        assert_eq!(store.stored_bytes(), 0);
    }

    #[test]
    fn scopes_round_trip_through_wire_representation() {
        assert_eq!(BlobScope::from_wire("shared").unwrap(), BlobScope::Shared);
        assert_eq!(BlobScope::from_wire("shared").unwrap().to_wire(), "shared");
        let workspace = BlobScope::from_wire("workspace:proj.1").unwrap();
        assert_eq!(workspace, BlobScope::Workspace("proj.1".to_string()));
        assert_eq!(workspace.to_wire(), "workspace:proj.1");
        let private = BlobScope::from_wire("private:plugin.a").unwrap();
        assert_eq!(private, BlobScope::Private("plugin.a".to_string()));
        assert_eq!(private.to_wire(), "private:plugin.a");
        assert!(matches!(
            BlobScope::from_wire("open:anyone"),
            Err(BlobError::InvalidArgument(_))
        ));
        assert!(matches!(
            BlobScope::from_wire("workspace"),
            Err(BlobError::InvalidArgument(_))
        ));
    }

    #[test]
    fn quotas_and_expired_upload_cleanup_are_accounted() {
        let mut store = store();
        assert_eq!(
            store.begin_upload("a", BlobScope::Shared, "text/plain", 17, 0, 10),
            Err(BlobError::QuotaExceeded("single blob limit"))
        );
        let lease = store
            .begin_upload("a", BlobScope::Shared, "text/plain", 8, 0, 10)
            .unwrap();
        assert_eq!(store.inflight_bytes(), 8);
        assert_eq!(
            store.upload_chunk(lease, "a", b"123456789", 1),
            Err(BlobError::SizeMismatch {
                expected: 8,
                actual: 9
            })
        );
        let report = store.sweep(10, 0);
        assert_eq!(report.expired_uploads, 1);
        assert_eq!(store.inflight_bytes(), 0);
        assert_eq!(store.active_upload_count(), 0);
    }
}
