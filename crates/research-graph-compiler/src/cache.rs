//! 增量编译、缓存与资源预算 / Incremental compile, cache & budgets (spec GC-14)。
//! 语义输出与全量编译逐位等价；缓存按 contentRootHash 分层失效、
//! 编译器版本隔离、损坏缓存丢弃重算。当前为骨架。

/// 缓存键：编译器版本 + contentRootHash（语义区）分层。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheKey {
    /// 编译器版本（缓存版本隔离，GC14-11）。
    pub compiler_version: u32,
    /// 语义区根哈希（contentRoot，GC14-02）。
    pub content_root_hash: String,
}

/// 资源预算：内存、工作量、深度上限（GC14-07/08）。
#[derive(Clone, Copy, Debug)]
pub struct ResourceLimits {
    /// 最大节点/边处理量。
    pub max_work: usize,
    /// 路径枚举最大深度。
    pub max_depth: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_work: 1_000_000,
            max_depth: 32,
        }
    }
}

/// 取消令牌（骨架占位）：SCC 阶段可安全终止且不写缓存（GC14-06）。
#[derive(Clone, Debug, Default)]
pub struct CancelToken {
    cancelled: bool,
}

impl CancelToken {
    /// 检查是否已取消。
    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

/// 缓存层（骨架占位）：命中/失效策略随增量编译接入。
#[derive(Clone, Debug, Default)]
pub struct CompileCache {}
