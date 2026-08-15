//! 模拟 provider SSE：传输层只负责组装公开 content，修复层负责 JSON 边界和审计。

use semantic_pipeline::{parse_json_with_repair, RepairOptions, RepairOutcome};
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct ExtractionFixture {
    title: String,
    sections: Vec<String>,
}

#[test]
fn assembled_sse_content_is_repaired_and_audited() {
    let sse = [
        r#"data: {"choices":[{"delta":{"content":"模型正在输出：\n```json\n{\"title\":\"中文论文\","}}]}"#,
        r#"data: {"choices":[{"delta":{"content":"\"sections\":[\"方法\",\"结果\"],\n}"}}]}"#,
        "data: [DONE]",
    ];

    let content = sse
        .iter()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter(|payload| *payload != "[DONE]")
        .map(|payload| {
            #[derive(Deserialize)]
            struct Event {
                choices: Vec<Choice>,
            }
            #[derive(Deserialize)]
            struct Choice {
                delta: Delta,
            }
            #[derive(Deserialize)]
            struct Delta {
                content: Option<String>,
            }
            serde_json::from_str::<Event>(payload)
                .expect("fixture SSE event must be valid")
                .choices
                .into_iter()
                .filter_map(|choice| choice.delta.content)
                .collect::<String>()
        })
        .collect::<String>();

    let outcome = parse_json_with_repair::<ExtractionFixture>(&content, RepairOptions::default());
    let RepairOutcome::Parsed(parsed) = outcome else {
        panic!("expected parsed SSE content")
    };
    assert_eq!(parsed.value.title, "中文论文");
    assert_eq!(parsed.value.sections, vec!["方法", "结果"]);
    assert!(parsed
        .audit
        .entries
        .iter()
        .any(|entry| entry.code == "MARKDOWN_FENCE_REMOVED"));
    assert!(parsed
        .audit
        .entries
        .iter()
        .any(|entry| entry.code == "TRAILING_COMMA_REMOVED"));
}
