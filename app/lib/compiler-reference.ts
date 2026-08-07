/**
 * TS 参考实现：v3 语义区的规范化（§3.4）。
 * TS reference implementation of the v3 canonicalization, used ONLY as the
 * comparison anchor for the TS↔Rust bit-by-bit parity gate (§15.5). It mirrors
 * `src-tauri/src/graph_compiler.rs::canonicalize` exactly so a compiler-parity
 * test can assert byte-identical output.
 *
 * 编排纪律：生产运行永远走 Rust 内核；本文件不参与任何运行时路径，仅供比对测试。
 */

/**
 * 文本规范化：NFC 归一化 + 空白折叠（任意 Unicode 空白序列 → 单个空格，并去首尾）。
 * Text normalization: NFC + whitespace folding (any run of Unicode whitespace
 * collapses to a single space, with leading/trailing whitespace trimmed).
 */
export function normalizeText(input: string): string {
  // NFC 归一化
  const nfc = input.normalize("NFC");
  // split_whitespace 等价折叠：任意空白序列 → 单空格，并去首尾
  return nfc.split(/\s+/).filter((part) => part.length > 0).join(" ");
}

/**
 * 键规范化：仅 NFC 归一化，不折叠空白。
 * Key normalization: NFC only, no whitespace folding.
 */
export function normalizeKey(input: string): string {
  return input.normalize("NFC");
}

/**
 * 数字规范序列化：整数值去掉小数尾（1.0 → 1，-0.0 → 0）；非整数用最短往返表示。
 * Canonical number serialization: integral floats drop their decimal tail; non-integral
 * floats use the shortest round-trip form.
 */
export function canonicalNumber(value: number): string {
  if (Number.isFinite(value)) {
    if (Number.isInteger(value) && Math.abs(value) < 9_007_199_254_740_992) {
      return String(value); // 1.0 → "1", -0 → "0"
    }
  }
  return String(value);
}

/** 键的 NFC 规范化 + UTF-8 字节字典序比较，与 Rust 的 `String::cmp`（字节序）一致。 */
function byteCompare(a: string, b: string): number {
  const aBytes = Buffer.from(a, "utf8");
  const bBytes = Buffer.from(b, "utf8");
  const len = Math.min(aBytes.length, bBytes.length);
  for (let i = 0; i < len; i += 1) {
    if (aBytes[i] < bBytes[i]) return -1;
    if (aBytes[i] > bBytes[i]) return 1;
  }
  return aBytes.length - bBytes.length;
}

/**
 * 值 → 规范 JSON 字符串 / Value → canonical JSON text.
 * 返回的字符串与 Rust `canonicalize` 的 UTF-8 字节序列一致（除 serde 对
 * 极少数控制字符的转义差异——夹具不含此类输入）。
 */
export function canonicalize(value: unknown): string {
  if (value === null) return "null";
  if (typeof value === "boolean") return value ? "true" : "false";
  if (typeof value === "number") return canonicalNumber(value);
  if (typeof value === "string") {
    return JSON.stringify(normalizeText(value));
  }
  if (Array.isArray(value)) {
    const canonicalItems = value.map((item) => canonicalize(item));
    canonicalItems.sort(byteCompare);
    return `[${canonicalItems.join(",")}]`;
  }
  if (typeof value === "object") {
    const object = value as Record<string, unknown>;
    const entries: Array<[string, unknown]> = Object.keys(object).map((key) => {
      // 键 NFC 归一化；归一化冲突时后者覆盖（与 Rust 一致）
      const normalized = normalizeKey(key);
      return [normalized, object[key]] as [string, unknown];
    });
    // 归一化冲突去重（保留最后一个）
    const entryMap = new Map<string, unknown>();
    for (const [key, val] of entries) entryMap.set(key, val);
    const sortedKeys = [...entryMap.keys()].sort(byteCompare);
    const parts = sortedKeys.map((key) => {
      return `${JSON.stringify(key)}:${canonicalize(entryMap.get(key))}`;
    });
    return `{${parts.join(",")}}`;
  }
  throw new Error(`cannot canonicalize value of type ${typeof value}`);
}
