//! Host bus blob lifecycle: `blob.list` + `blob.release`.
//!
//! Read-only enumeration and single-blob release on top of the in-memory
//! `BlobStore`. The store stays lock-free; the kernel's `RwLock<BlobStore>`
//! is held by the caller.

use std::sync::RwLock;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::kernel::blob::BlobStore;
use crate::kernel_commands::{inline_request, HostCallRequest};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlobReleaseRequest {
    pub digest: String,
}

/// `blob.list` — snapshot stored blob metadata plus store stats.
pub fn dispatch_blob_list(blobs: &RwLock<BlobStore>) -> Result<Value, String> {
    let store = blobs
        .read()
        .map_err(|_| "blob store lock is poisoned".to_string())?;
    let entries = store.list_stored();
    Ok(json!({
        "blobs": entries,
        "stats": {
            "storedBlobCount": store.stored_blob_count(),
            "storedBytes": store.stored_bytes(),
            "inflightBytes": store.inflight_bytes(),
            "activeUploadCount": store.active_upload_count(),
            "activeReadCount": store.active_read_count(),
        },
    }))
}

/// `blob.release` — remove one stored blob by hex digest; pinned blobs are
/// rejected.
pub fn dispatch_blob_release(
    request: &HostCallRequest,
    blobs: &RwLock<BlobStore>,
) -> Result<Value, String> {
    let release = inline_request::<BlobReleaseRequest>(request)
        .map_err(|error| format!("invalid blob.release request: {error}"))?;
    let mut store = blobs
        .write()
        .map_err(|_| "blob store lock is poisoned".to_string())?;
    let freed = store
        .release_stored(&release.digest)
        .map_err(|error| format!("blob.release failed: {error}"))?;
    Ok(json!({ "digest": release.digest, "freedBytes": freed }))
}
