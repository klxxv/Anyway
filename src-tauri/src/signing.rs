//! Ed25519 签名验证与公钥信任根管理 / Ed25519 signature verification and public-key trust-root management.
//!
//! `.myc` 插件的 plugin.yml 可携带一个 `signature` 字段，存储发布者对清单内容的 Ed25519 签名。
//! 安装器在校验阶段提取该字段，使用发布者公钥验证，验签失败则拒绝安装。
//!
//! A `.myc` plugin's plugin.yml may carry a `signature` field holding the publisher's
//! Ed25519 signature over the manifest content. The installer extracts the field,
//! verifies it against the publisher's public key, and rejects invalid signatures.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    path::Path,
};

/// 信任的发布者公钥映射（发布者 ID → Ed25519 验证密钥）/ Trusted publisher public keys.
pub type TrustedKeys = HashMap<String, VerifyingKey>;

/// Research Canvas 官方内置公钥（base64 编码的 32 字节 Ed25519 公钥）。
/// 在正式的密钥轮换流程建立之前，用于校验官方发布的 .myc 插件包。
///
/// Research Canvas built-in public key (base64-encoded 32-byte Ed25519 public key).
/// Used to verify official .myc packages until a formal key rotation process is established.
const BUILTIN_RESEARCH_CANVAS_PUBKEY: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

/// 信任根配置文件名 / Trust-root configuration file name.
const TRUSTED_KEYS_FILE: &str = "trusted-keys.json";

// ---------------------------------------------------------------------------
// 签名有效载荷构建 / Signature payload construction
// ---------------------------------------------------------------------------

/// 计算清单的签名有效载荷（不含 signature 字段的 JSON 序列化的 SHA-256）。
/// 此设计避免了 YAML 格式不确定性的问题——签名覆盖的是确定的 JSON 形式。
///
/// Computes the signature payload for a manifest: SHA-256 of its JSON serialization
/// with the signature field removed. Using JSON avoids YAML formatting ambiguity.
pub fn manifest_payload(manifest_json_without_signature: &serde_json::Value) -> Vec<u8> {
    let json_bytes =
        serde_json::to_vec(manifest_json_without_signature).expect("JSON serialization is infallible");
    Sha256::digest(&json_bytes).to_vec()
}

// ---------------------------------------------------------------------------
// 密钥加载 / Key loading
// ---------------------------------------------------------------------------

/// 从 base64 字符串解码 Ed25519 公钥 / Decode an Ed25519 public key from base64.
pub fn decode_public_key(b64: &str) -> Result<VerifyingKey, String> {
    let bytes = BASE64
        .decode(b64)
        .map_err(|error| format!("Invalid base64 public key: {error}"))?;
    let arr: &[u8; 32] = bytes
        .first_chunk::<32>()
        .ok_or_else(|| "Ed25519 public key must be exactly 32 bytes".to_string())?;
    VerifyingKey::from_bytes(arr)
        .map_err(|error| format!("Invalid Ed25519 public key: {error}"))
}

/// 从 base64 字符串解码 Ed25519 签名 / Decode an Ed25519 signature from base64.
pub fn decode_signature(b64: &str) -> Result<Signature, String> {
    let bytes = BASE64
        .decode(b64)
        .map_err(|error| format!("Invalid base64 signature: {error}"))?;
    let arr: &[u8; 64] = bytes
        .first_chunk::<64>()
        .ok_or_else(|| "Ed25519 signature must be exactly 64 bytes".to_string())?;
    Ok(Signature::from_bytes(arr))
}

// ---------------------------------------------------------------------------
// 信任根管理 / Trust root management
// ---------------------------------------------------------------------------

/// 加载内置信任根 / Load built-in trust root.
pub fn load_builtin_trusted_keys() -> TrustedKeys {
    let mut keys = HashMap::new();
    // 若内置公钥为占位值则跳过 / Skip if the built-in key is a placeholder.
    if let Ok(key) = decode_public_key(BUILTIN_RESEARCH_CANVAS_PUBKEY) {
        keys.insert("researchcanvas".to_string(), key);
    }
    keys
}

/// 从磁盘加载社区信任的公钥 / Load community-trusted keys from disk.
///
/// 文件格式 / File format (trusted-keys.json):
/// ```json
/// {
///   "publisher-id": "<base64-encoded-ed25519-public-key>",
///   ...
/// }
/// ```
pub fn load_file_trusted_keys(base: &Path) -> Result<TrustedKeys, String> {
    let path = base.join(TRUSTED_KEYS_FILE);
    if !path.is_file() {
        return Ok(HashMap::new());
    }
    let text =
        fs::read_to_string(&path).map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    let map: HashMap<String, String> =
        serde_json::from_str(&text).map_err(|error| format!("Invalid {}: {error}", TRUSTED_KEYS_FILE))?;
    let mut keys = HashMap::with_capacity(map.len());
    for (publisher, b64) in map {
        let key = decode_public_key(&b64).map_err(|error| {
            format!("Invalid key for publisher '{publisher}' in {TRUSTED_KEYS_FILE}: {error}")
        })?;
        keys.insert(publisher, key);
    }
    Ok(keys)
}

/// 合并内置与文件信任根（文件中的条目可覆盖内置条目）。
/// Merge built-in and file trust roots (file entries override built-in entries).
pub fn load_all_trusted_keys(base: &Path) -> Result<TrustedKeys, String> {
    let mut keys = load_builtin_trusted_keys();
    let file_keys = load_file_trusted_keys(base)?;
    keys.extend(file_keys);
    Ok(keys)
}

// ---------------------------------------------------------------------------
// 查找发布者公钥 / Lookup publisher public key
// ---------------------------------------------------------------------------

/// 在信任根中查找发布者的公钥 / Look up a publisher's public key in the trust roots.
pub fn find_public_key<'a>(
    publisher: &str,
    trusted_keys: &'a TrustedKeys,
) -> Result<&'a VerifyingKey, String> {
    // 精确匹配 / Exact match
    if let Some(key) = trusted_keys.get(publisher) {
        return Ok(key);
    }
    // 大小写不敏感回退 / Case-insensitive fallback
    let lower = publisher.to_lowercase();
    for (name, key) in trusted_keys {
        if name.to_lowercase() == lower {
            return Ok(key);
        }
    }
    Err(format!(
        "No trusted public key found for publisher '{publisher}'. \
         Add it to {TRUSTED_KEYS_FILE} in the plugins directory."
    ))
}

// ---------------------------------------------------------------------------
// 签名验证 / Signature verification
// ---------------------------------------------------------------------------

/// 使用发布者公钥验证插件清单签名。
/// 签名覆盖清单的 JSON 序列化（不含 signature 字段）的 SHA-256 哈希值。
///
/// Verify a plugin manifest signature against the publisher's public key.
/// The signature covers the SHA-256 hash of the JSON-serialized manifest
/// (with the signature field removed).
pub fn verify_manifest_signature(
    publisher: &str,
    manifest_without_signature: &serde_json::Value,
    signature_b64: &str,
    trusted_keys: &TrustedKeys,
) -> Result<(), String> {
    let public_key = find_public_key(publisher, trusted_keys)?;
    let signature = decode_signature(signature_b64)?;
    let payload = manifest_payload(manifest_without_signature);

    public_key
        .verify(&payload, &signature)
        .map_err(|error| format!("Ed25519 signature verification failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use rand_core::OsRng;

    fn make_test_keys() -> (SigningKey, VerifyingKey) {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        (signing_key, verifying_key)
    }

    fn signature_to_b64(sig: &Signature) -> String {
        BASE64.encode(sig.to_bytes())
    }

    #[test]
    fn round_trip_valid_signature() {
        let (signing_key, verifying_key) = make_test_keys();

        let manifest: serde_json::Value = serde_json::json!({
            "apiVersion": "researchcanvas.dev/v1alpha1",
            "kind": "ThemePlugin",
            "metadata": {
                "id": "test.theme",
                "name": "Test Theme",
                "version": "1.0.0",
                "publisher": "test-publisher",
                "developer": "Test",
                "description": "A test theme."
            },
            "spec": {
                "engine": "declarative",
                "entry": "theme.json",
                "capabilities": ["theme.register"],
                "permissions": []
            }
        });

        let payload = manifest_payload(&manifest);
        let signature = signing_key.sign(&payload);

        let mut trusted = HashMap::new();
        trusted.insert("test-publisher".to_string(), verifying_key);

        verify_manifest_signature(
            "test-publisher",
            &manifest,
            &signature_to_b64(&signature),
            &trusted,
        )
        .expect("valid signature should pass");
    }

    #[test]
    fn tampered_manifest_rejected() {
        let (signing_key, verifying_key) = make_test_keys();

        let original: serde_json::Value = serde_json::json!({
            "apiVersion": "researchcanvas.dev/v1alpha1",
            "kind": "ThemePlugin",
            "metadata": {
                "id": "test.theme",
                "name": "Test Theme",
                "version": "1.0.0",
                "publisher": "test-publisher",
                "developer": "Test",
                "description": "Original description."
            },
            "spec": {
                "engine": "declarative",
                "entry": "theme.json",
                "capabilities": ["theme.register"],
                "permissions": []
            }
        });

        let tampered: serde_json::Value = serde_json::json!({
            "apiVersion": "researchcanvas.dev/v1alpha1",
            "kind": "ThemePlugin",
            "metadata": {
                "id": "evil.plugin",
                "name": "Evil Plugin",
                "version": "1.0.0",
                "publisher": "test-publisher",
                "developer": "Hacker",
                "description": "Tampered description!"
            },
            "spec": {
                "engine": "declarative",
                "entry": "theme.json",
                "capabilities": ["theme.register"],
                "permissions": []
            }
        });

        let payload = manifest_payload(&original);
        let signature = signing_key.sign(&payload);

        let mut trusted = HashMap::new();
        trusted.insert("test-publisher".to_string(), verifying_key);

        let result = verify_manifest_signature(
            "test-publisher",
            &tampered,
            &signature_to_b64(&signature),
            &trusted,
        );
        assert!(result.is_err(), "tampered manifest must be rejected");
        assert!(
            result
                .unwrap_err()
                .contains("signature verification failed"),
            "error message should mention signature verification"
        );
    }

    #[test]
    fn unknown_publisher_rejected() {
        let (signing_key, _verifying_key) = make_test_keys();

        let manifest: serde_json::Value = serde_json::json!({
            "apiVersion": "researchcanvas.dev/v1alpha1",
            "kind": "ThemePlugin",
            "metadata": {
                "id": "test.theme",
                "name": "Test Theme",
                "version": "1.0.0",
                "publisher": "unknown-publisher",
                "developer": "Test",
                "description": "A test theme."
            },
            "spec": {
                "engine": "declarative",
                "entry": "theme.json",
                "capabilities": ["theme.register"],
                "permissions": []
            }
        });

        let payload = manifest_payload(&manifest);
        let signature = signing_key.sign(&payload);

        let trusted: TrustedKeys = HashMap::new(); // empty trust store

        let result = verify_manifest_signature(
            "unknown-publisher",
            &manifest,
            &signature_to_b64(&signature),
            &trusted,
        );
        assert!(result.is_err(), "unknown publisher must be rejected");
        assert!(
            result.unwrap_err().contains("No trusted public key found"),
            "should mention missing trusted key"
        );
    }

    #[test]
    fn decode_invalid_public_key_rejected() {
        assert!(decode_public_key("not-base64!!!").is_err());
        assert!(decode_public_key("c2hvcnQ=").is_err()); // too short (4 bytes)
    }

    #[test]
    fn decode_invalid_signature_rejected() {
        assert!(decode_signature("not-base64!!!").is_err());
        assert!(decode_signature("c2hvcnQ=").is_err()); // too short
    }

    #[test]
    fn case_insensitive_publisher_lookup() {
        let (_signing_key, verifying_key) = make_test_keys();
        let mut trusted = HashMap::new();
        trusted.insert("ResearchCanvas".to_string(), verifying_key);

        let key = find_public_key("researchcanvas", &trusted)
            .expect("case-insensitive lookup should succeed");
        assert_eq!(key.as_bytes(), verifying_key.as_bytes());
    }
}
