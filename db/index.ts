import { env } from "cloudflare:workers";
import { drizzle } from "drizzle-orm/d1";
import * as schema from "./schema";

/**
 * 仅在实际访问数据时读取部署绑定，避免构建期依赖 Cloudflare 环境。
 * Resolves the deployment binding only at database access time, avoiding build-time Cloudflare dependency.
 */
export function getDb() {
  if (!env.DB) {
    throw new Error(
      "Cloudflare D1 binding `DB` is unavailable. Set the `d1` field in .openai/hosting.json to `DB` or let your control plane inject the real binding values before using the database."
    );
  }

  return drizzle(env.DB, { schema });
}
