// ── ProviderPlugin 能力声明 / ProviderPlugin capability declarations ──

/**
 * ProviderPlugin 可声明的能力全集。
 * Provider plugins may declare these capabilities; the host enforces them.
 */
export const PROVIDER_CAPABILITIES = [
  /** 发送 LLM 聊天请求（由宿主代理，Provider 不直接持有网络权限） */
  "llm.chat",
  /** 配置 API key 和模型偏好 */
  "llm.configure",
] as const;

export type ProviderCapability = (typeof PROVIDER_CAPABILITIES)[number];
