//! 导出 / Export: digest、Mermaid 与机器可读报告 (spec GC-13)。
//! 导出必须稳定、可转义、可预算截断；换行固定 LF，浮点固定小数位。
//! `export_mermaid` 已实现为 GC-13 基线（确定性、可转义）；digest 仍为骨架。

use serde_json::Value;

/// 导出选项：换行、浮点小数位、预算截断。
#[derive(Clone, Copy, Debug)]
pub struct ExportOptions {
    /// 浮点坐标小数位。
    pub float_digits: usize,
    /// digest 最大字节数（超出截断并带摘要）。
    pub max_bytes: usize,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            float_digits: 2,
            max_bytes: 1_048_576,
        }
    }
}

/// 项目 digest（骨架占位）：规范输入的稳定摘要。
#[derive(Clone, Debug, Default)]
pub struct Digest {
    /// 摘要字节（截断规则 GC13-10 随实现接入）。
    pub bytes: Vec<u8>,
}

/// 生成项目 digest（骨架占位：返回空字节）。
///
/// 空字节是明确的无摘要占位，避免把规范 JSON 的前 64 字节误当成稳定 digest。
pub fn project_digest(_project: &Value, _options: &ExportOptions) -> Digest {
    Digest {
        bytes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn digest_stub_returns_empty() {
        let digest = project_digest(&Value::Object(Default::default()), &ExportOptions::default());
        assert!(digest.bytes.is_empty(), "digest stub must not return a fake hash prefix");
    }

    #[test]
    fn mermaid_sanitization_is_injective() {
        let project = json!({
            "nodes": [
                {"id": "a_b", "title": "first"},
                {"id": "a+b", "title": "second"},
                {"id": "a-b", "title": "third"}
            ],
            "edges": []
        });
        let out = export_mermaid(&project);
        // a_b 与 a+b 消毒后都会变成 a_b，需要其中一个带后缀。
        let node_lines: Vec<&str> = out.lines().filter(|l| l.contains("[\"")).collect();
        assert_eq!(node_lines.len(), 3, "all three nodes must be emitted");
        let ids: Vec<String> = node_lines
            .iter()
            .map(|l| l.split("[\"").next().unwrap().trim().to_string())
            .collect();
        let unique: std::collections::HashSet<_> = ids.iter().cloned().collect();
        assert_eq!(unique.len(), ids.len(), "sanitized mermaid ids must be unique: {ids:?}");
    }

    #[test]
    fn mermaid_node_and_edge_ids_share_pool() {
        let project = json!({
            "nodes": [{"id": "x_y", "title": "Node"}],
            "edges": [{"id": "e1", "type": "supports", "source": "x+y", "target": "x_y", "directed": true}]
        });
        let out = export_mermaid(&project);
        // x_y (node) 与 x+y (source) 碰撞，source 应得到后缀。
        assert!(out.contains("x_y["));
        assert!(out.contains("x_y__1"));
    }
}

/// 将实体 id 转换为安全的 Mermaid 节点 id：仅保留 ASCII 字母数字与 `_`/`-`，
/// 其余替换为 `_`（保证任意 id 都能渲染）。
fn mermaid_id_base(id: &str) -> String {
    let mut out = String::with_capacity(id.len());
    for ch in id.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push_str("node");
    }
    out
}

/// Mermaid ID 消毒池：保证最终 ID 单射。
///
/// 不同原始 id 可能映射到同一消毒形式（如 `a+b` 与 `a_b`），
/// 发生冲突时追加 `__1`、`__2` … 等稳定后缀，直到唯一。
#[derive(Default)]
struct MermaidIdPool {
    seen: std::collections::HashMap<String, usize>,
}

impl MermaidIdPool {
    fn sanitize(&mut self, id: &str) -> String {
        let base = mermaid_id_base(id);
        let counter = self.seen.get(&base).copied().unwrap_or(0);
        if counter == 0 {
            self.seen.insert(base.clone(), 1);
            return base;
        }
        let mut n = counter;
        let mut candidate = format!("{}__{}", base, n);
        while self.seen.contains_key(&candidate) {
            n += 1;
            candidate = format!("{}__{}", base, n);
        }
        self.seen.insert(candidate.clone(), 1);
        self.seen.insert(base, n + 1);
        candidate
    }
}

/// 转义 Mermaid 标签文本（GC13-09 基线）：`"` `&` `<` `>` `#` 转义为
/// Mermaid HTML 实体，控制空白折叠为单个空格。
fn mermaid_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '"' => out.push_str("#quot;"),
            '&' => out.push_str("#amp;"),
            '<' => out.push_str("#lt;"),
            '>' => out.push_str("#gt;"),
            '#' => out.push_str("#num;"),
            '\n' | '\r' | '\t' => out.push(' '),
            other => out.push(other),
        }
    }
    out
}

/// 导出 Mermaid flow 图（GC-13 基线实现）。
/// 节点 → `id["title"]`；边 → `source -->|type| target`（无向边用 `---`）。
/// 输出确定性：节点与边均按 id 排序，换行固定 LF，标签经 `mermaid_escape`
/// 转义；`canvas compile --output mermaid` 与本函数输出逐字节一致。
pub fn export_mermaid(project: &Value) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push("flowchart LR".to_string());
    let mut id_pool = MermaidIdPool::default();

    if let Some(nodes) = project.get("nodes").and_then(Value::as_array) {
        let mut entries: Vec<(String, String, String)> = Vec::new();
        for node in nodes {
            let Some(id) = node.get("id").and_then(Value::as_str) else {
                continue;
            };
            if id.is_empty() {
                continue;
            }
            let title = node
                .get("title")
                .and_then(Value::as_str)
                .map(mermaid_escape)
                .unwrap_or_else(|| id.to_string());
            let safe_id = id_pool.sanitize(id);
            entries.push((id.to_string(), safe_id, title));
        }
        entries.sort_by(|a, b| a.1.cmp(&b.1));
        for (_, safe_id, title) in entries {
            lines.push(format!("    {safe_id}[\"{title}\"]"));
        }
    }

    if let Some(edges) = project.get("edges").and_then(Value::as_array) {
        let mut entries: Vec<(String, String)> = Vec::new();
        for edge in edges {
            let Some(id) = edge.get("id").and_then(Value::as_str) else {
                continue;
            };
            let (Some(source), Some(target)) = (
                edge.get("source").and_then(Value::as_str),
                edge.get("target").and_then(Value::as_str),
            ) else {
                continue;
            };
            if id.is_empty() || source.is_empty() || target.is_empty() {
                continue;
            }
            let edge_type = edge.get("type").and_then(Value::as_str).unwrap_or("edge");
            let directed = edge.get("directed").and_then(Value::as_bool).unwrap_or(true);
            let arrow = if directed { "-->" } else { "---" };
            let safe_source = id_pool.sanitize(source);
            let safe_target = id_pool.sanitize(target);
            let line = format!(
                "    {} {}{} {}",
                safe_source,
                arrow,
                format_args!("|{}|", mermaid_escape(edge_type)),
                safe_target
            );
            entries.push((id.to_string(), line));
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        for (_, line) in entries {
            lines.push(line);
        }
    }

    lines.join("\n") + "\n"
}
