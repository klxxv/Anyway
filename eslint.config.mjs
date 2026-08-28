import { defineConfig, globalIgnores } from "eslint/config";

const eslintConfig = defineConfig([
  globalIgnores([
    ".next/**",
    ".playwright-cli/**",
    "dist/**",
    "build/**",
    "crates/*/target/**",
    "node_modules/**",
    "plugins/sdk/rust/target/**",
    "src-tauri/target/**",
  ]),
]);

export default eslintConfig;
