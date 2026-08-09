//! 规范化 / Canonicalization (§3.4, spec GC-02)。
//! 规范化是一切哈希、Diff 与跨机器一致性的根：对象键排序（含嵌套 data）、
//! 数组规范化后排序、数字规范序列化、文本 NFC 归一化 + 空白折叠。
//! Canonicalization is the root of every hash, diff, and cross-machine
//! consistency guarantee.

use serde_json::{Number, Value};
use std::io::Write as _;
use unicode_normalization::UnicodeNormalization;

/// 文本规范化：NFC 归一化 + 折叠空白（任意 Unicode 空白序列 → 单个空格，并去首尾）。
/// Text normalization: NFC + whitespace folding (any run of Unicode whitespace
/// collapses to one space, with leading/trailing whitespace trimmed).
pub fn normalize_text(input: &str) -> String {
    input
        .nfc()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// 键规范化：仅 NFC 归一化，不折叠空白 —— 键是结构标识，合并键会掩盖数据错误。
/// Key normalization: NFC only, no whitespace folding — keys are structural
/// identifiers and merging them could hide data errors.
pub fn normalize_key(input: &str) -> String {
    input.nfc().collect()
}

/// 数字规范序列化：整数值去掉小数尾（1.0 → 1，-0.0 → 0）；非整数用最短往返表示。
/// Canonical number serialization: integral floats drop their decimal tail
/// (1.0 → 1, -0.0 → 0); non-integral floats use the shortest round-trip form.
pub fn canonical_number(number: &Number) -> String {
    if let Some(float) = number.as_f64() {
        // 2^53 以内可精确转换；超出则回退到 serde_json 的原始表示（保留大整数）。
        if float.is_finite() && float.fract() == 0.0 && float.abs() < 9_007_199_254_740_992.0 {
            return (float as i64).to_string();
        }
    }
    number.to_string()
}

/// 语义有序字段：数组保持元素顺序（路径步骤、步骤序列等），不参与集合排序。
/// Semantically ordered fields: arrays keep element order — never sorted.
pub const SEQUENCE_FIELDS: &[&str] = &["pathSteps", "steps", "path"];

/// 值 → 规范 JSON 字节 / Value → canonical JSON bytes:
/// - 对象：键 NFC 归一化后按字典序排序（含嵌套 data），值递归；
/// - 数组：元素规范化后按规范字节排序（顺序不敏感，集合语义）；
/// - 数字：`canonical_number`；字符串：`normalize_text`。
pub fn canonicalize(value: &Value) -> Vec<u8> {
    canonicalize_mode(value, true)
}

/// 字段感知的规范化：`SEQUENCE_FIELDS` 中的数组保持元素顺序（GC02-04），
/// 其余字段按集合语义排序（GC02-03）。递归进对象后仍按集合语义处理。
/// Field-aware canonicalization: arrays under `SEQUENCE_FIELDS` keep their
/// element order; every other array is sorted as a set.
pub fn canonicalize_field(field: &str, value: &Value) -> Vec<u8> {
    canonicalize_mode(value, !SEQUENCE_FIELDS.contains(&field))
}

fn canonicalize_mode(value: &Value, sort_arrays: bool) -> Vec<u8> {
    match value {
        Value::Null => b"null".to_vec(),
        Value::Bool(true) => b"true".to_vec(),
        Value::Bool(false) => b"false".to_vec(),
        Value::Number(number) => canonical_number(number).into_bytes(),
        Value::String(text) => quoted_string(&normalize_text(text)),
        Value::Array(items) => {
            let mut canonical_items: Vec<Vec<u8>> = items.iter().map(canonicalize).collect();
            if sort_arrays {
                canonical_items.sort();
            }
            let mut out =
                Vec::with_capacity(canonical_items.iter().map(Vec::len).sum::<usize>() + 2);
            out.push(b'[');
            for (index, bytes) in canonical_items.iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                out.extend_from_slice(bytes);
            }
            out.push(b']');
            out
        }
        Value::Object(map) => {
            // 键先 NFC 归一化，再按字典序排序；归一化冲突时后者覆盖，保证规范 JSON 合法。
            let mut entries: Vec<(String, &Value)> = Vec::with_capacity(map.len());
            for (key, entry) in map {
                let normalized = normalize_key(key);
                match entries
                    .iter_mut()
                    .find(|(existing, _)| *existing == normalized)
                {
                    Some((_, existing_value)) => *existing_value = entry,
                    None => entries.push((normalized, entry)),
                }
            }
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut out = Vec::with_capacity(map.len() * 8);
            out.push(b'{');
            for (index, (key, entry)) in entries.iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                out.extend_from_slice(&quoted_string(key));
                out.push(b':');
                // 字段感知:SEQUENCE_FIELDS(pathSteps 等)保序,其余按集合语义。
                // 之前统一走 canonicalize,canonicalize_field 成了死代码,
                // 步骤顺序不同的链会撞 hash。
                out.extend_from_slice(&canonicalize_field(key, entry));
            }
            out.push(b'}');
            out
        }
    }
}

/// 带 JSON 转义的带引号字符串字节 / Quoted, JSON-escaped string bytes.
fn quoted_string(text: &str) -> Vec<u8> {
    // serde_json can only fail here if the input Value contains a map with
    // non-string keys, which Value::String never does. Unwrap is acceptable.
    serde_json::to_vec(&Value::String(text.to_string())).unwrap_or_else(|_| {
        // Fallback: manually quote/escape the string so canonicalization never
        // panics, even if serde somehow changes its invariants.
        let mut out = Vec::with_capacity(text.len() + 2);
        out.push(b'"');
        for byte in text.bytes() {
            match byte {
                b'\\' | b'"' => {
                    out.push(b'\\');
                    out.push(byte);
                }
                b'\n' => out.extend_from_slice(b"\\n"),
                b'\r' => out.extend_from_slice(b"\\r"),
                b'\t' => out.extend_from_slice(b"\\t"),
                b if b < 0x20 => {
                    let _ = write!(out, "\\u{byte:04x}");
                }
                _ => out.push(byte),
            }
        }
        out.push(b'"');
        out
    })
}
