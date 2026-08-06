//! GraphPatch 计划与原子应用 / Patch planning & atomic apply (spec GC-05)。
//! 所有编辑先形成可审阅计划（新增/修改/删除/ID 联动重写/受影响范围），
//! 用户确认后原子应用；baseFileHash 过期触发乐观并发冲突。当前为骨架，
//! 生产路径由 workspace_host 的 git diff 管线接入。

use serde_json::Value;

/// GraphPatch 交换格式（骨架占位）：最终以 app/plugins/contracts.ts 的
/// 审阅门控 interchange 为契约，ID 只由编译器生成。
#[derive(Clone, Debug, Default)]
pub struct GraphPatch {
    /// 预留：针对的 baseFileHash（乐观并发控制，GC05-08）。
    pub base_file_hash: Option<String>,
}

/// 补丁计划（骨架占位）：预演将新增、修改、删除、ID 重映射与受影响范围。
#[derive(Clone, Debug, Default)]
pub struct PatchPlan {}

/// 预演补丁：展示影响范围而不修改项目（GC05-01/02/03）。
pub fn plan_patch(_base: &Value, _patch: &GraphPatch) -> PatchPlan {
    PatchPlan::default()
}

/// 原子应用补丁：失败时项目保持原样（GC05-04/05/08/09/10）。
pub fn apply_patch(_base: &Value, _plan: PatchPlan) -> Result<Value, crate::error::CompileFailure> {
    Ok(Value::Null)
}
