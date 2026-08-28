//! 场景消融 / Scenario ablation (spec GC-12)。
//! 以不可变基图加 overlay 计算直接禁用、失去可达性、替代路径、
//! 实验矩阵与影响集合。当前为骨架。

use serde_json::Value;

/// 场景对比结果（骨架占位）：直接影响与新增矛盾见证。
#[derive(Clone, Debug, Default)]
pub struct ScenarioComparison {
    /// 被直接禁用的节点/边 id。
    pub directly_disabled: Vec<String>,
    /// 因禁用失去可达性的节点 id。
    pub newly_unreachable: Vec<String>,
}

/// 对比一个场景与 base（骨架占位：暂返回空对比）。
pub fn compare_scenario(_base: &Value, _scenario: &Value) -> ScenarioComparison {
    ScenarioComparison::default()
}

/// 实验矩阵分析（骨架占位）：report 缺失组合（GC12-07）。
#[derive(Clone, Debug, Default)]
pub struct MatrixAnalysis {
    /// 缺失的组合单元描述。
    pub missing_cells: Vec<String>,
}

/// 分析实验矩阵的完整性（骨架占位）。
pub fn analyze_matrix(_scenarios: &[Value]) -> MatrixAnalysis {
    MatrixAnalysis::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_comparison_stub_is_empty() {
        let result = compare_scenario(&Value::Null, &Value::Null);
        assert!(result.directly_disabled.is_empty());
        assert!(result.newly_unreachable.is_empty());
    }

    #[test]
    fn matrix_analysis_stub_is_empty() {
        let result = analyze_matrix(&[]);
        assert!(result.missing_cells.is_empty());
    }
}
