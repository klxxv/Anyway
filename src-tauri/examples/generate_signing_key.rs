//! 生成官方 .myc 插件签名密钥对 / Generate the official .myc plugin signing keypair.
//!
//! 用法 / Usage:
//!   cargo run --example generate_signing_key -- <output-dir>
//!
//! 公钥打印到 stdout,粘贴进 signing.rs 的 BUILTIN_RESEARCH_CANVAS_PUBKEY;
//! 私钥写入 <output-dir>/researchcanvas-official-signing-key.json(必须保密、不得提交)。
//! The public key prints to stdout for BUILTIN_RESEARCH_CANVAS_PUBKEY; the
//! secret key is written to <output-dir>/researchcanvas-official-signing-key.json
//! and must stay private and uncommitted.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::SigningKey;
use rand_core::OsRng;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(".secrets"));
    fs::create_dir_all(&output)?;

    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();

    let public_b64 = BASE64.encode(verifying_key.as_bytes());
    let secret_b64 = BASE64.encode(signing_key.to_bytes());

    let key_file = output.join("researchcanvas-official-signing-key.json");
    let json = format!(
        "{{\n  \"publisher\": \"researchcanvas\",\n  \"publicKey\": \"{public_b64}\",\n  \"secretKey\": \"{secret_b64}\"\n}}\n"
    );
    fs::write(&key_file, json)?;

    println!("public key (paste into signing.rs BUILTIN_RESEARCH_CANVAS_PUBKEY):");
    println!("{public_b64}");
    println!("secret key written to {} -- keep it private", key_file.display());
    Ok(())
}
