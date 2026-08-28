import { readSetting, type RuntimeProvider } from "./context";

export async function buildRuntimeConfig(): Promise<{ providers: RuntimeProvider[] }> {
  const format = await readSetting<"openai" | "anthropic">("api-format", "openai");
  const defaultBaseUrl = format === "anthropic" ? "https://api.moonshot.cn/anthropic" : "https://api.moonshot.cn/v1";
  return {
    providers: [
      {
        id: "kimi",
        baseUrl: await readSetting("api-url", defaultBaseUrl),
        format,
        model: await readSetting("model", "kimi-k2.6"),
        pdfTransport: await readSetting("pdf-transport", "local-text"),
        thinking: await readSetting("thinking", "enabled"),
        publicProgress: await readSetting("public-progress", "disabled"),
        allowedDomains: ["api.moonshot.cn", "api.moonshot.ai"],
        secretEnv: "ANYWAY_PLUGIN_SECRET_PROVIDER_API_KEY"
      }
    ]
  };
}
