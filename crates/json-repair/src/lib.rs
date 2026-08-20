//! 可审计的 LLM JSON 边界提取、有限修复与严格反序列化。
//!
//! 这个模块只做可以由输入本身确定的修复。它不会创建实体、锚点、置信度
//! 或任何论文事实；无法安全判断时返回 [`RepairOutcome::NeedsRecovery`]，
//! 交给上层的 recovery 流程处理。

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

/// 稳定的审计严重级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditSeverity {
    Info,
    Warning,
    Error,
}

/// 一条确定性修复或失败审计记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    /// 稳定机器可读代码，不随本地化改变。
    pub code: String,
    /// JSONPath 风格路径；无法定位到字段时使用 `$`。
    pub path: String,
    pub before_summary: String,
    pub after_summary: String,
    pub severity: AuditSeverity,
    /// true 表示是否由确定性规则产生/检测，不表示内容可信。
    pub deterministic: bool,
}

/// 一次修复的完整审计报告。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditReport {
    pub entries: Vec<AuditEntry>,
}

impl AuditReport {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn has_errors(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.severity == AuditSeverity::Error)
    }

    fn push(
        &mut self,
        code: &'static str,
        path: impl Into<String>,
        before_summary: impl Into<String>,
        after_summary: impl Into<String>,
        severity: AuditSeverity,
        deterministic: bool,
    ) {
        self.entries.push(AuditEntry {
            code: code.to_string(),
            path: path.into(),
            before_summary: before_summary.into(),
            after_summary: after_summary.into(),
            severity,
            deterministic,
        });
    }
}

/// 修复行为配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepairOptions {
    /// 是否启用下方明确列出的字段别名，不进行任意 snake_case 转换。
    pub allow_field_aliases: bool,
    /// 是否启用明确登记的安全缺省字段（目前只有 VariableRegistryEntry.isNew=false）。
    pub allow_safe_defaults: bool,
}

impl Default for RepairOptions {
    fn default() -> Self {
        Self {
            allow_field_aliases: true,
            allow_safe_defaults: true,
        }
    }
}

impl RepairOptions {
    /// 仅做 JSON 边界和语法层修复，不做字段变换。
    pub const fn syntax_only() -> Self {
        Self {
            allow_field_aliases: false,
            allow_safe_defaults: false,
        }
    }
}

/// JSON 修复/解析失败的稳定分类。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JsonRepairErrorKind {
    EmptyInput,
    NoJsonValue,
    MultipleJsonValues,
    UnbalancedDelimiter,
    UnterminatedString,
    InvalidControlCharacter,
    InvalidJson,
    MissingRequiredField,
    TypeMismatch,
    UnsupportedRepair,
}

/// 不触发猜测的错误描述。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonRepairError {
    pub kind: JsonRepairErrorKind,
    pub message: String,
    pub deterministic: bool,
}

impl fmt::Display for JsonRepairError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for JsonRepairError {}

/// 只完成 JSON 层修复后的文本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairedJson {
    pub json: String,
    pub audit: AuditReport,
}

/// 已完成严格 serde 解析的结果。
#[derive(Debug, Clone)]
pub struct ParsedJson<T> {
    pub value: T,
    pub repaired_json: String,
    pub audit: AuditReport,
}

/// 统一的成功/恢复分支 API。
#[derive(Debug, Clone)]
pub enum RepairOutcome<T> {
    Parsed(ParsedJson<T>),
    NeedsRecovery {
        /// 这是最后一个可审计候选文本，不代表可以直接使用。
        repaired_json: String,
        audit: AuditReport,
        error: JsonRepairError,
    },
}

impl<T> RepairOutcome<T> {
    pub fn needs_recovery(&self) -> bool {
        matches!(self, Self::NeedsRecovery { .. })
    }

    pub fn audit(&self) -> &AuditReport {
        match self {
            Self::Parsed(parsed) => &parsed.audit,
            Self::NeedsRecovery { audit, .. } => audit,
        }
    }
}

/// 从模型输出提取一个 JSON 对象/数组边界。
///
/// 该函数按 Unicode 字符边界扫描，花括号/方括号出现在字符串内容中时不会
/// 被误认为容器边界。前后说明会被排除；多个并列 JSON 值则明确进入恢复分支。
pub fn extract_json_fragment(input: &str) -> Result<(String, AuditReport), JsonRecovery> {
    let mut audit = AuditReport::default();
    let mut text = input;

    if text.is_empty() {
        return Err(needs_recovery(
            String::new(),
            audit,
            JsonRepairErrorKind::EmptyInput,
            "input is empty",
        ));
    }

    if text.starts_with('\u{feff}') {
        text = &text['\u{feff}'.len_utf8()..];
        audit.push(
            "BOM_REMOVED",
            "$",
            "one leading UTF-8 BOM",
            "no BOM",
            AuditSeverity::Info,
            true,
        );
    }

    text = strip_markdown_fence(text, &mut audit);
    let (start, end, stack, in_string, escaped) = match scan_boundary(text) {
        Ok(found) => found,
        Err(error) => {
            return Err(needs_recovery(
                text.to_string(),
                audit,
                error.kind,
                error.message,
            ));
        }
    };

    if in_string {
        if escaped {
            return Err(needs_recovery(
                text[start..].to_string(),
                audit,
                JsonRepairErrorKind::UnterminatedString,
                "input ends after an escape character; closing it would change the string",
            ));
        }
        let mut fragment = text[start..].to_string();
        fragment.push('"');
        audit.push(
            "UNTERMINATED_STRING_CLOSED",
            "$",
            "unterminated JSON string at end of input",
            "closing quote appended",
            AuditSeverity::Warning,
            true,
        );
        return finish_fragment(fragment, text, start, text.len(), stack, &mut audit);
    }

    if !stack.is_empty() {
        let fragment = text[start..].to_string();
        return finish_fragment(fragment, text, start, text.len(), stack, &mut audit);
    }

    let fragment = text[start..end].to_string();
    let suffix = &text[end..];
    let trimmed_suffix = suffix.trim_start();
    if let Some(first) = trimmed_suffix.chars().next() {
        if first == '{' || first == '[' {
            audit.push(
                "MULTIPLE_JSON_VALUES",
                "$",
                "a second JSON value follows the first",
                "candidate rejected",
                AuditSeverity::Error,
                true,
            );
            return Err(needs_recovery(
                fragment,
                audit,
                JsonRepairErrorKind::MultipleJsonValues,
                "multiple top-level JSON values are ambiguous",
            ));
        }
        audit.push(
            "SURROUNDING_TEXT_IGNORED",
            "$",
            "non-JSON text around the top-level value",
            "top-level JSON fragment retained",
            AuditSeverity::Info,
            true,
        );
    }

    Ok((fragment, audit))
}

/// 执行有限、确定性的 JSON 修复，但不进行 serde 类型解析。
pub fn repair_json(input: &str, options: RepairOptions) -> RepairOutcome<Value> {
    let (fragment, mut audit) = match extract_json_fragment(input) {
        Ok(value) => value,
        Err(recovery) => return recovery.into_outcome(),
    };

    let mut candidate = fragment;
    candidate = remove_trailing_commas(candidate, &mut audit);

    if remove_terminal_comma(&mut candidate) {
        audit.push(
            "TRAILING_COMMA_REMOVED",
            "$",
            "comma at the end of an incomplete container",
            "comma removed before deterministic closure",
            AuditSeverity::Info,
            true,
        );
    }

    if let Err(error) = validate_and_close(&mut candidate, &mut audit) {
        return recovery_outcome(candidate, audit, error);
    }

    let mut value: Value = match serde_json::from_str(&candidate) {
        Ok(value) => value,
        Err(error) => {
            return needs_recovery_from_serde(candidate, audit, error).into_outcome();
        }
    };

    if options.allow_field_aliases {
        if let Err(error) = normalize_fields(&mut value, "$", &mut audit) {
            return recovery_outcome(candidate, audit, error);
        }
    }

    if options.allow_safe_defaults {
        add_safe_defaults(&mut value, "$", &mut audit);
    }

    let repaired_json = match serde_json::to_string(&value) {
        Ok(json) => json,
        Err(error) => {
            return recovery_outcome(
                candidate,
                audit,
                JsonRepairError {
                    kind: JsonRepairErrorKind::UnsupportedRepair,
                    message: format!("cannot serialize repaired JSON: {error}"),
                    deterministic: true,
                },
            );
        }
    };

    RepairOutcome::Parsed(ParsedJson {
        value,
        repaired_json,
        audit,
    })
}

/// 从模型文本修复并严格反序列化到调用者指定的 serde 类型。
///
/// `T` 的 serde 定义决定字段和类型约束；本模块不会为了让解析通过而生成
/// 实体、锚点、置信度或事实。
pub fn parse_json_with_repair<T: DeserializeOwned>(
    input: &str,
    options: RepairOptions,
) -> RepairOutcome<T> {
    let repaired = match repair_json(input, options) {
        RepairOutcome::Parsed(parsed) => parsed,
        RepairOutcome::NeedsRecovery {
            repaired_json,
            audit,
            error,
        } => {
            return RepairOutcome::NeedsRecovery {
                repaired_json,
                audit,
                error,
            }
        }
    };

    match serde_json::from_str::<T>(&repaired.repaired_json) {
        Ok(value) => RepairOutcome::Parsed(ParsedJson {
            value,
            repaired_json: repaired.repaired_json,
            audit: repaired.audit,
        }),
        Err(error) => {
            let mut audit = repaired.audit;
            let classified = classify_serde_error(&error);
            audit.push(
                "STRICT_SERDE_PARSE_FAILED",
                "$",
                "repaired JSON did not match the requested serde type",
                format!("needs recovery: {classified:?}"),
                AuditSeverity::Error,
                true,
            );
            RepairOutcome::NeedsRecovery {
                repaired_json: repaired.repaired_json,
                audit,
                error: JsonRepairError {
                    kind: classified,
                    message: error.to_string(),
                    deterministic: true,
                },
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct JsonRecovery {
    pub repaired_json: String,
    pub audit: AuditReport,
    pub error: JsonRepairError,
}

fn recovery_outcome<T>(
    repaired_json: String,
    mut audit: AuditReport,
    error: JsonRepairError,
) -> RepairOutcome<T> {
    audit.push(
        error_code(&error.kind),
        "$",
        error.message.clone(),
        "needs recovery",
        AuditSeverity::Error,
        error.deterministic,
    );
    RepairOutcome::NeedsRecovery {
        repaired_json,
        audit,
        error,
    }
}

impl JsonRecovery {
    fn into_outcome<T>(self) -> RepairOutcome<T> {
        RepairOutcome::NeedsRecovery {
            repaired_json: self.repaired_json,
            audit: self.audit,
            error: self.error,
        }
    }
}

fn needs_recovery(
    repaired_json: String,
    mut audit: AuditReport,
    kind: JsonRepairErrorKind,
    message: impl Into<String>,
) -> JsonRecovery {
    audit.push(
        error_code(&kind),
        "$",
        "input cannot be safely normalized",
        "needs recovery",
        AuditSeverity::Error,
        true,
    );
    JsonRecovery {
        repaired_json,
        audit,
        error: JsonRepairError {
            kind,
            message: message.into(),
            deterministic: true,
        },
    }
}

fn error_code(kind: &JsonRepairErrorKind) -> &'static str {
    match kind {
        JsonRepairErrorKind::EmptyInput => "EMPTY_INPUT",
        JsonRepairErrorKind::NoJsonValue => "NO_JSON_VALUE",
        JsonRepairErrorKind::MultipleJsonValues => "MULTIPLE_JSON_VALUES",
        JsonRepairErrorKind::UnbalancedDelimiter => "UNBALANCED_DELIMITER",
        JsonRepairErrorKind::UnterminatedString => "UNTERMINATED_STRING",
        JsonRepairErrorKind::InvalidControlCharacter => "INVALID_CONTROL_CHARACTER",
        JsonRepairErrorKind::InvalidJson => "INVALID_JSON",
        JsonRepairErrorKind::MissingRequiredField => "MISSING_REQUIRED_FIELD",
        JsonRepairErrorKind::TypeMismatch => "TYPE_MISMATCH",
        JsonRepairErrorKind::UnsupportedRepair => "UNSUPPORTED_REPAIR",
    }
}

fn strip_markdown_fence<'a>(text: &'a str, audit: &mut AuditReport) -> &'a str {
    let opening = text.match_indices("```").find_map(|(index, _)| {
        let before = if index == 0 {
            None
        } else {
            text.as_bytes().get(index - 1).copied()
        };
        if before.is_none_or(|byte| byte == b'\n' || byte == b'\r') {
            Some(index)
        } else {
            None
        }
    });
    let Some(opening) = opening else { return text };
    let trimmed = &text[opening + "```".len()..];
    let Some(first_newline) = trimmed.find('\n') else {
        return text;
    };
    let body_start = first_newline + 1;
    let body = &trimmed[body_start..];
    let closing = body.match_indices("```").find_map(|(index, _)| {
        let before = if index == 0 {
            None
        } else {
            body.as_bytes().get(index - 1).copied()
        };
        if before.is_none_or(|byte| byte == b'\n' || byte == b'\r') {
            Some(index)
        } else {
            None
        }
    });
    let result = closing.map(|index| &body[..index]).unwrap_or(body);
    audit.push(
        "MARKDOWN_FENCE_REMOVED",
        "$",
        "markdown code-fence wrapper",
        "fence wrapper removed",
        AuditSeverity::Info,
        true,
    );
    result
}

fn scan_boundary(text: &str) -> Result<(usize, usize, Vec<char>, bool, bool), JsonRepairError> {
    let mut start = None;
    let mut stack: Vec<char> = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    let mut end = text.len();

    for (index, ch) in text.char_indices() {
        if start.is_none() {
            if ch == '{' || ch == '[' {
                start = Some(index);
                stack.push(ch);
            }
            continue;
        }

        if in_string {
            if ch == '\n' || ch == '\r' {
                return Err(JsonRepairError {
                    kind: JsonRepairErrorKind::InvalidControlCharacter,
                    message: "raw newline inside a JSON string cannot be repaired safely".into(),
                    deterministic: true,
                });
            }
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' | '[' => stack.push(ch),
            '}' | ']' => {
                let expected = if ch == '}' { '{' } else { '[' };
                if stack.last().copied() != Some(expected) {
                    return Err(JsonRepairError {
                        kind: JsonRepairErrorKind::UnbalancedDelimiter,
                        message: format!("closing {ch} does not match the open container"),
                        deterministic: true,
                    });
                }
                stack.pop();
                if stack.is_empty() {
                    end = index + ch.len_utf8();
                    break;
                }
            }
            _ => {}
        }
    }

    let Some(start) = start else {
        return Err(JsonRepairError {
            kind: JsonRepairErrorKind::NoJsonValue,
            message: "no JSON object or array boundary found".into(),
            deterministic: true,
        });
    };
    Ok((start, end, stack, in_string, escaped))
}

fn finish_fragment(
    mut fragment: String,
    source: &str,
    start: usize,
    _end: usize,
    stack: Vec<char>,
    audit: &mut AuditReport,
) -> Result<(String, AuditReport), JsonRecovery> {
    if remove_terminal_comma(&mut fragment) {
        audit.push(
            "TRAILING_COMMA_REMOVED",
            "$",
            "comma at the end of an incomplete container",
            "comma removed before deterministic closure",
            AuditSeverity::Info,
            true,
        );
    }
    let mut closers = String::new();
    for open in stack.iter().rev() {
        closers.push(if *open == '{' { '}' } else { ']' });
    }
    if !closers.is_empty() {
        fragment.push_str(&closers);
        audit.push(
            "UNCLOSED_CONTAINERS_CLOSED",
            "$",
            format!(
                "{} unclosed JSON container(s) after byte {start}",
                closers.chars().count()
            ),
            format!("appended deterministic closers {closers}"),
            AuditSeverity::Warning,
            true,
        );
    }
    let _ = source;
    Ok((fragment, audit.clone()))
}

fn remove_terminal_comma(candidate: &mut String) -> bool {
    let content_end = candidate.trim_end().len();
    if content_end == 0 || !candidate[..content_end].ends_with(',') {
        return false;
    }
    candidate.remove(content_end - 1);
    true
}

fn remove_trailing_commas(mut candidate: String, audit: &mut AuditReport) -> String {
    let mut in_string = false;
    let mut escaped = false;
    let mut removals = Vec::new();
    for (index, ch) in candidate.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            ',' => {
                let next = &candidate[index + ch.len_utf8()..];
                if next.trim_start().starts_with('}') || next.trim_start().starts_with(']') {
                    removals.push(index);
                }
            }
            _ => {}
        }
    }
    for index in removals.iter().rev() {
        candidate.remove(*index);
        audit.push(
            "TRAILING_COMMA_REMOVED",
            "$",
            "comma immediately before a closing container",
            "comma removed",
            AuditSeverity::Info,
            true,
        );
    }
    candidate
}

fn validate_and_close(
    candidate: &mut String,
    audit: &mut AuditReport,
) -> Result<(), JsonRepairError> {
    let mut stack = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    for ch in candidate.chars() {
        if in_string {
            if ch == '\n' || ch == '\r' {
                return Err(JsonRepairError {
                    kind: JsonRepairErrorKind::InvalidControlCharacter,
                    message: "raw newline inside a JSON string cannot be repaired safely".into(),
                    deterministic: true,
                });
            }
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' | '[' => stack.push(ch),
            '}' | ']' => {
                let expected = if ch == '}' { '{' } else { '[' };
                if stack.last().copied() != Some(expected) {
                    return Err(JsonRepairError {
                        kind: JsonRepairErrorKind::UnbalancedDelimiter,
                        message: format!("closing {ch} does not match the open container"),
                        deterministic: true,
                    });
                }
                stack.pop();
            }
            _ => {}
        }
    }
    if in_string {
        if escaped {
            return Err(JsonRepairError {
                kind: JsonRepairErrorKind::UnterminatedString,
                message: "input ends after an escape character".into(),
                deterministic: true,
            });
        }
        candidate.push('"');
        audit.push(
            "UNTERMINATED_STRING_CLOSED",
            "$",
            "unterminated JSON string at end of input",
            "closing quote appended",
            AuditSeverity::Warning,
            true,
        );
    }
    for open in stack.iter().rev() {
        candidate.push(if *open == '{' { '}' } else { ']' });
    }
    if !stack.is_empty() {
        audit.push(
            "UNCLOSED_CONTAINERS_CLOSED",
            "$",
            format!("{} unclosed JSON container(s)", stack.len()),
            "deterministic closing delimiters appended",
            AuditSeverity::Warning,
            true,
        );
    }
    Ok(())
}

fn normalize_fields(
    value: &mut Value,
    path: &str,
    audit: &mut AuditReport,
) -> Result<(), JsonRepairError> {
    match value {
        Value::Array(items) => {
            for (index, item) in items.iter_mut().enumerate() {
                normalize_fields(item, &format!("{path}[{index}]"), audit)?;
            }
        }
        Value::Object(object) => {
            let keys: Vec<String> = object.keys().cloned().collect();
            for key in keys {
                let Some(mapped) = field_alias(&key) else {
                    continue;
                };
                if mapped == key {
                    continue;
                }
                if object.contains_key(mapped) {
                    return Err(JsonRepairError {
                        kind: JsonRepairErrorKind::UnsupportedRepair,
                        message: format!("field alias {key} conflicts with existing {mapped}"),
                        deterministic: true,
                    });
                }
                let Some(entry) = object.remove(&key) else {
                    continue;
                };
                object.insert(mapped.to_string(), entry);
                audit.push(
                    "FIELD_ALIAS_REWRITTEN",
                    format!("{path}.{key}"),
                    format!("field {key}"),
                    format!("field {mapped}"),
                    AuditSeverity::Info,
                    true,
                );
            }
            let keys: Vec<String> = object.keys().cloned().collect();
            for key in keys {
                normalize_fields(
                    object.get_mut(&key).expect("key collected from object"),
                    &format!("{path}.{key}"),
                    audit,
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn add_safe_defaults(value: &mut Value, path: &str, audit: &mut AuditReport) {
    match value {
        Value::Array(items) => {
            for (index, item) in items.iter_mut().enumerate() {
                add_safe_defaults(item, &format!("{path}[{index}]"), audit);
            }
        }
        Value::Object(object) => {
            if object.contains_key("tempId")
                && object.contains_key("domain")
                && object.contains_key("role")
                && !object.contains_key("isNew")
            {
                object.insert("isNew".into(), Value::Bool(false));
                audit.push(
                    "SAFE_DEFAULT_INSERTED",
                    format!("{path}.isNew"),
                    "isNew is absent on a variable registry entry",
                    "false",
                    AuditSeverity::Info,
                    true,
                );
            }
            let keys: Vec<String> = object.keys().cloned().collect();
            for key in keys {
                add_safe_defaults(
                    object.get_mut(&key).expect("key collected from object"),
                    &format!("{path}.{key}"),
                    audit,
                );
            }
        }
        _ => {}
    }
}

/// 只接受当前 semantic-pipeline IR 已知的别名；不会把任意 snake_case 猜成 camelCase。
fn field_alias(key: &str) -> Option<&'static str> {
    Some(match key {
        "abstract" | "abstract_text" => "abstractText",
        "start_anchor" => "startAnchor",
        "ref_id" => "refId",
        "total_pages" => "totalPages",
        "temp_id" => "tempId",
        "section_coverage" => "sectionCoverage",
        "total_entities" => "totalEntities",
        "claim_type" => "claimType",
        "variable_type" => "variableType",
        "methodology" => "methodology",
        "sample_size" => "sampleSize",
        "p_value" => "pValue",
        "effect_size" => "effectSize",
        "section_id" => "sectionId",
        "paragraph_id" => "paragraphId",
        "start_offset" => "startOffset",
        "end_offset" => "endOffset",
        "experiment_temp_id" => "experimentTempId",
        "variable_temp_id" => "variableTempId",
        "canonical_temp_id" => "canonicalTempId",
        "canonical_name" => "canonicalName",
        "canonical_description" => "canonicalDescription",
        "merged_temp_ids" => "mergedTempIds",
        "evidence_temp_ids" => "evidenceTempIds",
        "strength_rationale" => "strengthRationale",
        "measured_as" => "measuredAs",
        "supported_by" => "supportedBy",
        "variables" => "variables",
        "held_at" => "heldAt",
        "interaction_with" => "interactionWith",
        "iv_settings" => "ivSettings",
        "canonical_metric" => "canonicalMetric",
        "variable_registry" => "variableRegistry",
        "experiment_matrix" => "experimentMatrix",
        "merge_groups" => "mergeGroups",
        "claim_evidence_bundles" => "claimEvidenceBundles",
        "metric_alignment" => "metricAlignment",
        "dataset_registry" => "datasetRegistry",
        "main_conclusions" => "mainConclusions",
        "ablation_analysis" => "ablationAnalysis",
        "interaction_effects" => "interactionEffects",
        "missing_controls" => "missingControls",
        "internal_conflicts" => "internalConflicts",
        "synthesis_summary" => "synthesisSummary",
        "conclusion_type" => "conclusionType",
        "ablation_type" => "ablationType",
        "target_component" => "targetComponent",
        "impact_assessment" => "impactAssessment",
        "claim_temp_id" => "claimTempId",
        "effect_description" => "effectDescription",
        "risk_level" => "riskLevel",
        "recommended_control" => "recommendedControl",
        "affects_claims" => "affectsClaims",
        "claim_a" => "claimA",
        "claim_b" => "claimB",
        "conflict_description" => "conflictDescription",
        "resolution_note" => "resolutionNote",
        "paper_id" => "paperId",
        "review_required" => "reviewRequired",
        "api_version" => "apiVersion",
        _ => return None,
    })
}

fn needs_recovery_from_serde(
    candidate: String,
    audit: AuditReport,
    error: serde_json::Error,
) -> JsonRecovery {
    let kind = classify_serde_error(&error);
    needs_recovery(candidate, audit, kind, error.to_string())
}

fn classify_serde_error(error: &serde_json::Error) -> JsonRepairErrorKind {
    if error.is_eof() {
        JsonRepairErrorKind::InvalidJson
    } else if error.is_syntax() {
        JsonRepairErrorKind::InvalidJson
    } else if error.is_data() {
        let message = error.to_string();
        if message.contains("missing field") {
            JsonRepairErrorKind::MissingRequiredField
        } else {
            JsonRepairErrorKind::TypeMismatch
        }
    } else {
        JsonRepairErrorKind::UnsupportedRepair
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{ExtractionMeta, ReferenceInfo, SectionInfo, StructureExtraction};

    fn structure_json() -> String {
        r#"{
          "title":"中文论文",
          "authors":[],
          "abstractText":null,
          "sections":[],
          "references":[],
          "meta":{"language":"zh","totalPages":2}
        }"#
        .into()
    }

    #[test]
    fn removes_bom_fence_prose_and_trailing_comma_with_unicode_safe_boundary() {
        let input = format!(
            "\u{feff}模型说明：\n```json\n{},\n```\n完成",
            structure_json().trim_end_matches('}')
        );
        let outcome =
            parse_json_with_repair::<StructureExtraction>(&input, RepairOptions::default());
        let RepairOutcome::Parsed(parsed) = outcome else {
            panic!("expected parsed")
        };
        assert_eq!(parsed.value.title.as_deref(), Some("中文论文"));
        assert!(parsed.audit.entries.iter().any(|e| e.code == "BOM_REMOVED"));
        assert!(parsed
            .audit
            .entries
            .iter()
            .any(|e| e.code == "MARKDOWN_FENCE_REMOVED"));
        assert!(parsed
            .audit
            .entries
            .iter()
            .any(|e| e.code == "TRAILING_COMMA_REMOVED"));
    }

    #[test]
    fn closes_truncated_string_and_containers_only_when_serde_accepts_it() {
        let input = r#"{"title":"中文标题","authors":[],"sections":[],"references":[],"meta":{"language":"zh"#;
        let outcome =
            parse_json_with_repair::<StructureExtraction>(input, RepairOptions::default());
        let RepairOutcome::Parsed(parsed) = outcome else {
            panic!("expected parsed")
        };
        assert_eq!(parsed.value.title.as_deref(), Some("中文标题"));
        assert!(parsed
            .audit
            .entries
            .iter()
            .any(|e| e.code == "UNTERMINATED_STRING_CLOSED"));
        assert!(parsed
            .audit
            .entries
            .iter()
            .any(|e| e.code == "UNCLOSED_CONTAINERS_CLOSED"));
    }

    #[test]
    fn maps_abstract_and_explicit_snake_case_aliases() {
        let input = r#"{
          "title":"t", "authors":[], "abstract":"摘要", "sections":[], "references":[],
          "meta":{"language":"zh", "total_pages":1}
        }"#;
        let outcome =
            parse_json_with_repair::<StructureExtraction>(input, RepairOptions::default());
        let RepairOutcome::Parsed(parsed) = outcome else {
            panic!("expected parsed")
        };
        assert_eq!(parsed.value.abstract_text.as_deref(), Some("摘要"));
        assert!(parsed.repaired_json.contains("abstractText"));
        assert!(parsed.repaired_json.contains("totalPages"));
        assert!(parsed
            .audit
            .entries
            .iter()
            .any(|e| e.code == "FIELD_ALIAS_REWRITTEN"));
    }

    #[test]
    fn does_not_invent_semantic_fields_or_facts() {
        let input = r#"{"title":"t","authors":[],"references":[],"meta":{}}"#;
        let outcome =
            parse_json_with_repair::<StructureExtraction>(input, RepairOptions::default());
        let RepairOutcome::NeedsRecovery { error, audit, .. } = outcome else {
            panic!("expected recovery")
        };
        assert_eq!(error.kind, JsonRepairErrorKind::MissingRequiredField);
        assert!(audit
            .entries
            .iter()
            .any(|e| e.code == "STRICT_SERDE_PARSE_FAILED"));
    }

    #[test]
    fn rejects_mismatched_delimiters_and_multiple_values() {
        let mismatched = parse_json_with_repair::<Value>(r#"{"a":[1}"#, RepairOptions::default());
        assert!(
            matches!(mismatched, RepairOutcome::NeedsRecovery { error, .. } if error.kind == JsonRepairErrorKind::UnbalancedDelimiter)
        );

        let multiple =
            parse_json_with_repair::<Value>(r#"{"a":1}{"b":2}"#, RepairOptions::default());
        assert!(
            matches!(multiple, RepairOutcome::NeedsRecovery { error, .. } if error.kind == JsonRepairErrorKind::MultipleJsonValues)
        );
    }

    #[test]
    fn safe_default_only_adds_is_new_for_a_variable_registry_entry() {
        let input = r#"{
          "experimentMatrix":[],
          "variableRegistry":[{"tempId":"v1","name":"x","aliases":[],"domain":{"type":"continuous"},"role":"independent"}]
        }"#;
        let outcome = parse_json_with_repair::<Value>(input, RepairOptions::default());
        let RepairOutcome::Parsed(parsed) = outcome else {
            panic!("expected parsed")
        };
        assert_eq!(parsed.value["variableRegistry"][0]["isNew"], false);
        assert!(parsed
            .audit
            .entries
            .iter()
            .any(|e| e.code == "SAFE_DEFAULT_INSERTED"));
    }

    #[test]
    fn test_types_used_by_the_strict_parser_are_real_ir_types() {
        let _: Option<SectionInfo> = None;
        let _: Option<ReferenceInfo> = None;
        let _: Option<ExtractionMeta> = None;
    }
}
